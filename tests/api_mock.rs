//! HttpApi against the in-process mock server.

use riv::api::http::HttpApi;
use riv::api::*;
use riv::auth::{NoToken, StaticToken};
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};
use std::sync::Arc;

async fn server(sc: Scenario) -> MockServer {
    MockServer::start(0, sc).await.expect("mock starts")
}

fn api(s: &MockServer) -> HttpApi {
    HttpApi::new(s.base_url.clone(), Arc::new(StaticToken(MOCK_TOKEN.into()))).with_sleep_scale(0.0)
}

#[tokio::test]
async fn lists_events_without_auth() {
    let s = server(Scenario::default()).await;
    let api = HttpApi::new(s.base_url.clone(), Arc::new(NoToken));
    let ev = api.list_events().await.expect("events");
    assert_eq!(ev.len(), 2);
    assert!(ev.iter().any(|e| e.event_id == "demo-reinvent" && e.authentication_required));
}

#[tokio::test]
async fn walks_all_pages_with_variable_sizes() {
    let s = server(Scenario::default()).await;
    let api = api(&s);
    let mut tok = None;
    let mut sizes = vec![];
    let mut total = 0;
    loop {
        let p = api
            .list_sessions_page("demo-reinvent", &ListSessionsParams { include_abstracts: false, next_token: tok.clone(), locale: None })
            .await
            .expect("page");
        assert!(p.items.iter().all(|i| i.abstract_.is_none()));
        sizes.push(p.items.len());
        total += p.items.len();
        tok = p.next_token;
        if tok.is_none() {
            break;
        }
    }
    assert_eq!(total, 120);
    assert!(sizes.iter().collect::<std::collections::BTreeSet<_>>().len() > 1, "page sizes should vary: {sizes:?}");
}

#[tokio::test]
async fn registered_event_requires_token() {
    let s = server(Scenario::default()).await;
    let api = HttpApi::new(s.base_url.clone(), Arc::new(NoToken));
    assert_eq!(api.get_schedule("demo-reinvent").await.unwrap_err(), ApiError::Unauthorized);
}

#[tokio::test]
async fn reserve_reports_per_session_and_conflicts() {
    let s = server(Scenario::default()).await;
    let api = api(&s);
    let sessions = &s.state.sessions;
    let a = sessions.iter().find(|x| x.is_reservable == Some(true) && !x.seat_availability.is_some_and(|v| v.is_full())).expect("a");
    let first = api.reserve("demo-reinvent", std::slice::from_ref(&a.session_id)).await.expect("reserve");
    assert_eq!(first.successful, vec![a.session_id.clone()]);
    let again = api.reserve("demo-reinvent", std::slice::from_ref(&a.session_id)).await.expect("200 with failure");
    assert_eq!(again.failed[0].code, BulkFailureCode::AlreadyScheduled);
    // a session overlapping `a` must clash
    let (s1, e1) = a.range_utc(chrono_tz::America::Los_Angeles).expect("range");
    let b = sessions
        .iter()
        .find(|x| x.session_id != a.session_id && x.is_reservable == Some(true) && !x.seat_availability.is_some_and(|v| v.is_full())
            && x.range_utc(chrono_tz::America::Los_Angeles).is_some_and(|(s2, e2)| s2 < e1 && s1 < e2))
        .expect("an overlapping session exists");
    let r = api.reserve("demo-reinvent", std::slice::from_ref(&b.session_id)).await.expect("reserve b");
    assert_eq!(r.failed[0].code, BulkFailureCode::ScheduleConflict);
    assert_eq!(r.failed[0].conflicts_with.as_deref(), Some(&[a.session_id.clone()][..]));
}

#[tokio::test]
async fn closed_reservations_is_409() {
    let s = server(Scenario::parse("closed-reservations")).await;
    assert_eq!(api(&s).reserve("demo-reinvent", &["mock-0001".into()]).await.unwrap_err(), ApiError::Closed);
}

#[tokio::test]
async fn throttle_is_retried_transparently() {
    let s = server(Scenario::parse("throttle:2")).await;
    let api = api(&s);
    api.get_schedule("demo-reinvent").await.expect("1st");
    // request #2 and #3 get 429, the client waits and retries until it succeeds
    api.get_schedule("demo-reinvent").await.expect("retried");
}

#[tokio::test]
async fn drop_after_write_is_unknown_but_applied() {
    let s = server(Scenario::parse("drop-after-write")).await;
    let api = api(&s);
    let id = s.state.sessions.iter().find(|x| x.is_reservable == Some(true) && !x.seat_availability.is_some_and(|v| v.is_full())).expect("s").session_id.clone();
    let err = api.reserve("demo-reinvent", std::slice::from_ref(&id)).await.unwrap_err();
    assert!(matches!(err, ApiError::Unknown(_)), "{err:?}");
    let sched = api.get_schedule("demo-reinvent").await.expect("schedule");
    assert_eq!(sched.reserved, vec![id]);
}

#[tokio::test]
async fn edge_html_500_is_unknown_and_not_applied() {
    let s = server(Scenario::parse("edge-html-500")).await;
    let api = api(&s);
    let err = api.reserve("demo-reinvent", &["mock-0001".into()]).await.unwrap_err();
    assert!(matches!(err, ApiError::Unknown(_)));
    assert!(api.get_schedule("demo-reinvent").await.expect("schedule").reserved.is_empty());
}

#[tokio::test]
async fn personal_time_roundtrip_and_validation() {
    let s = server(Scenario::default()).await;
    let api = api(&s);
    let ok = PersonalTimeInput { start_date_time: "2026-12-02T03:00:00".into(), end_date_time: "2026-12-02T05:00:00".into(), title: "Dinner".into(), description: "d".into(), location: None };
    api.create_personal_time("demo-reinvent", &ok).await.expect("create");
    let sched = api.get_schedule("demo-reinvent").await.expect("schedule");
    assert_eq!(sched.personal_time.len(), 1);
    let bad = PersonalTimeInput { start_date_time: "2026-12-02T03:00:00Z".into(), ..ok.clone() };
    assert!(matches!(api.create_personal_time("demo-reinvent", &bad).await.unwrap_err(), ApiError::BadRequest(_)));
    api.delete_personal_time("demo-reinvent", &sched.personal_time[0].personal_time_id).await.expect("delete");
}
