use chrono::NaiveDate;
use riv::api::http::HttpApi;
use riv::api::{PersonalTime, Schedule, SeatAvailability, Session, SessionTime};
use riv::auth::StaticToken;
use riv::db::Db;
use riv::format::Format;
use riv::i18n::Lang;
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};
use riv::plan::CatalogView;
use riv::sync::{AbstractsMode, SyncOptions, sync};
use riv::today::{TodayInput, Venues, build_today, render_today};
use std::collections::HashSet;
use std::sync::Arc;

struct Cat(Vec<Session>);
impl CatalogView for Cat {
    fn session(&self, id: &str) -> Option<Session> {
        self.0.iter().find(|s| s.session_id == id).cloned()
    }
    fn same_title(&self, _: &Session) -> Vec<Session> {
        vec![]
    }
    fn tz(&self) -> chrono_tz::Tz {
        chrono_tz::America::Los_Angeles
    }
}

fn s(id: &str, code: &str, time: &str, venue: &str) -> Session {
    Session {
        session_id: id.into(),
        title: format!("Title {code}"),
        abbreviation: Some(code.into()),
        venue: Some(venue.into()),
        room: Some("R1".into()),
        is_reservable: Some(true),
        seat_availability: Some(SeatAvailability::Available),
        session_time: Some(SessionTime {
            date: Some("2026-12-01".into()),
            time: Some(time.into()),
            length: Some("60".into()),
            timezone: None,
        }),
        ..Default::default()
    }
}

fn venues(json: &str) -> Venues {
    serde_json::from_str(json).expect("venues")
}

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 12, 1).expect("date")
}

#[test]
fn walking_hints_come_only_from_configured_pairs() {
    let cat = Cat(vec![
        s("a", "AIM301", "10:00", "Venetian"),
        s("b", "SVS302", "13:00", "Wynn"),
        s("c", "SEC201", "15:00", "MGM Grand"),
    ]);
    let sched = Schedule {
        reserved: vec!["b".into(), "a".into(), "c".into()],
        favorites: vec![],
        // 2026-12-02T03:00Z = Dec 1 19:00 PST
        personal_time: vec![PersonalTime {
            personal_time_id: "p".into(),
            start_date_time: "2026-12-02T03:00:00".into(),
            end_date_time: "2026-12-02T05:00:00".into(),
            title: "Dinner".into(),
            description: "d".into(),
            location: Some("Wynn".into()),
        }],
    };
    let v = venues(r#"{"sameVenueMinutes": 5, "walkMinutes": [{"from": "Venetian", "to": "Wynn", "minutes": 15}]}"#);
    let prep: HashSet<String> = ["b".to_string()].into();
    let view = build_today(&TodayInput { date: day(), schedule: &sched, catalog: &cat, venues: &v, prep_notes: &prep });
    assert_eq!(view.items.iter().map(|i| i.code.as_str()).collect::<Vec<_>>(), vec!["AIM301", "SVS302", "SEC201", ""]);
    let [first, second, third, dinner] = &view.items[..] else { panic!("4 items") };
    assert_eq!(first.walk_minutes, None, "first item of the day has no origin: unknown");
    assert_eq!((second.walk_minutes, second.leave_by.as_deref()), (Some(15), Some("12:45")));
    assert!(second.prep_note);
    assert_eq!(third.walk_minutes, None, "Wynn -> MGM Grand is not configured: never invented");
    assert_eq!(dinner.start, "19:00");
    // other days are excluded
    let other = build_today(&TodayInput {
        date: NaiveDate::from_ymd_opt(2026, 12, 2).expect("d"),
        schedule: &sched,
        catalog: &cat,
        venues: &v,
        prep_notes: &prep,
    });
    assert!(other.items.is_empty());
}

#[test]
fn phone_format_is_short_and_tableless_ide_has_a_table() {
    let ss: Vec<Session> =
        (0..15).map(|n| s(&format!("s{n}"), &format!("C{n:03}"), &format!("{:02}:00", 6 + n), "Venetian")).collect();
    let sched = Schedule { reserved: ss.iter().map(|x| x.session_id.clone()).collect(), ..Default::default() };
    let cat = Cat(ss);
    let v = venues(r#"{"sameVenueMinutes": 5, "walkMinutes": []}"#);
    let none = HashSet::new();
    let view = build_today(&TodayInput { date: day(), schedule: &sched, catalog: &cat, venues: &v, prep_notes: &none });
    let phone = render_today(&view, Format::Phone, Lang::En);
    assert!(phone.lines().count() <= 12, "{phone}");
    assert!(!phone.contains('|'));
    assert!(phone.contains("walk 5 min · leave by 06:55") || phone.contains("walk: unknown"));
    assert!(phone.contains("more"));
    let ide = render_today(&view, Format::Ide, Lang::En);
    assert!(ide.contains("| Time |"));
    let ja = render_today(&view, Format::Phone, Lang::Ja);
    assert!(ja.contains("徒歩"), "{ja}");
    let empty = render_today(
        &build_today(&TodayInput {
            date: day(),
            schedule: &Schedule::default(),
            catalog: &cat,
            venues: &v,
            prep_notes: &none,
        }),
        Format::Phone,
        Lang::Ja,
    );
    assert!(empty.contains("予定はありません"));
}

#[tokio::test]
async fn prep_pack_from_the_mock_catalog() {
    let server = MockServer::start(0, Scenario::default()).await.expect("mock");
    let api = HttpApi::new(server.base_url.clone(), Arc::new(StaticToken(MOCK_TOKEN.into()))).with_sleep_scale(0.0);
    let db = Db::open_memory().expect("db");
    sync(
        &api,
        &db,
        &SyncOptions { event_id: "demo-reinvent".into(), locale: None, abstracts: AbstractsMode::Auto },
        &|_| {},
    )
    .await
    .expect("sync");
    let r1 = db
        .all_sessions("demo-reinvent")
        .expect("all")
        .into_iter()
        .find(|x| x.session.code().ends_with("-R1"))
        .expect("an R1 session");
    let pack = riv::prep::build_pack(&db, "demo-reinvent", r1.session.code(), Lang::Ja).expect("pack");
    assert_eq!(pack.session.id, r1.session.session_id);
    assert!(pack.queries.iter().any(|q| q.contains("re:Invent 2025")), "{:?}", pack.queries);
    assert!(
        pack.queries.iter().any(|q| q.contains("DevelopersIO")) && pack.queries.iter().any(|q| q.contains("Qiita"))
    );
    assert!(pack.related.len() <= 5);
    assert!(pack.prior_year_candidates.iter().any(|h| h.contains("-R2")), "{:?}", pack.prior_year_candidates);
    assert!(pack.note_template.contains("3行で") && pack.note_template.contains("関連"));
    assert!(pack.save_path.starts_with(".kiro/specs/reinvent-2026/prep/") && pack.save_path.ends_with(".md"));
    let en = riv::prep::build_pack(&db, "demo-reinvent", &r1.session.session_id, Lang::En).expect("en");
    assert!(!en.queries.iter().any(|q| q.contains("Qiita")));
    assert!(riv::prep::build_pack(&db, "demo-reinvent", "NOPE", Lang::En).is_err());
}
