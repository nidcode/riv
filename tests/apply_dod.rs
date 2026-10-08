//! Brief §9 definition of done, reproduced against the in-process mock server.

use chrono::{Duration, Utc};
use riv::api::http::HttpApi;
use riv::api::{EventsApi, Session};
use riv::apply::{ActionStatus, ApplyReport, ApplyRequest, apply};
use riv::auth::StaticToken;
use riv::db::Db;
use riv::error::{ErrorCode, RivError};
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};
use riv::plan::Plan;
use riv::plan::service::create_plan;
use riv::sync::{AbstractsMode, SyncOptions, sync};
use std::path::PathBuf;
use std::sync::Arc;

const EVENT: &str = "demo-reinvent";
const ACCT: &str = "acct";

struct Env {
    server: MockServer,
    api: HttpApi,
    db: Db,
    _dir: tempfile::TempDir,
    spec: PathBuf,
    plans: PathBuf,
}

async fn env(scenario: &str) -> Env {
    riv::i18n::set_override(Some(riv::i18n::Lang::En));
    let server = MockServer::start(0, Scenario::parse(scenario)).await.expect("mock");
    let api = HttpApi::new(server.base_url.clone(), Arc::new(StaticToken(MOCK_TOKEN.into()))).with_sleep_scale(0.0);
    let db = Db::open_memory().expect("db");
    let opts = SyncOptions { event_id: EVENT.into(), locale: None, abstracts: AbstractsMode::Never };
    sync(&api, &db, &opts, &|_| {}).await.expect("sync");
    let dir = tempfile::tempdir().expect("tmp");
    let spec = dir.path().join("design.md");
    let plans = dir.path().join("plans");
    Env { server, api, db, _dir: dir, spec, plans }
}

fn tz() -> chrono_tz::Tz {
    chrono_tz::America::Los_Angeles
}

fn bookable(s: &Session) -> bool {
    s.is_reservable == Some(true) && !s.seat_availability.is_some_and(|a| a.is_full())
}

fn overlap(a: &Session, b: &Session) -> bool {
    match (a.range_utc(tz()), b.range_utc(tz())) {
        (Some((s1, e1)), Some((s2, e2))) => s1 < e2 && s2 < e1,
        _ => false,
    }
}

/// `n` bookable sessions that do not overlap each other.
fn disjoint(env: &Env, n: usize) -> Vec<String> {
    let mut out: Vec<&Session> = Vec::new();
    for s in env.server.state.sessions.iter().filter(|s| bookable(s)) {
        if out.iter().all(|o| !overlap(o, s)) {
            out.push(s);
        }
        if out.len() == n {
            break;
        }
    }
    assert_eq!(out.len(), n, "mock catalog should hold {n} disjoint bookable sessions");
    out.iter().map(|s| s.session_id.clone()).collect()
}

fn overlapping_pair(env: &Env) -> (String, String) {
    let ss: Vec<&Session> = env.server.state.sessions.iter().filter(|s| bookable(s)).collect();
    for a in &ss {
        for b in &ss {
            if a.session_id < b.session_id && overlap(a, b) {
                return (a.session_id.clone(), b.session_id.clone());
            }
        }
    }
    panic!("no overlapping pair");
}

fn spec_text(rows: &[String], blocks: &str) -> String {
    let rows = if rows.is_empty() { "  []".to_string() } else { rows.join("\n") };
    format!(
        "# Design\n\n```yaml\n# riv:desired-state v1\nevent: {EVENT}\ntimezone: America/Los_Angeles\nsessions:\n{rows}\nblocks:\n{blocks}\n```\n"
    )
}

fn write_spec(e: &Env, rows: &[String]) {
    std::fs::write(&e.spec, spec_text(rows, "  []")).expect("write spec");
}

fn want(id: &str, w: &str) -> String {
    format!("  - {{id: {id}, want: {w}}}")
}

async fn plan(e: &Env) -> Plan {
    create_plan(&e.api, &e.db, &e.spec, &e.plans, ACCT).await.expect("plan").plan
}

async fn run_apply(e: &Env, p: &Plan, flag: bool) -> Result<ApplyReport, RivError> {
    let req = ApplyRequest {
        plan_id: Some(&p.plan_id),
        resume_run: None,
        spec_path: &e.spec,
        accept_seat_loss: flag,
        now: Utc::now(),
        account: ACCT,
        lang: riv::i18n::Lang::En,
    };
    apply(&e.api, &e.db, &e.plans, &req, &|_| true).await
}

async fn run_resume(e: &Env, run: &str) -> Result<ApplyReport, RivError> {
    let req = ApplyRequest {
        plan_id: None,
        resume_run: Some(run),
        spec_path: &e.spec,
        accept_seat_loss: false,
        now: Utc::now(),
        account: ACCT,
        lang: riv::i18n::Lang::En,
    };
    apply(&e.api, &e.db, &e.plans, &req, &|_| true).await
}

fn writes(e: &Env) -> Vec<String> {
    e.server.state.writes()
}

async fn reserved(e: &Env) -> Vec<String> {
    let mut r = e.api.get_schedule(EVENT).await.expect("schedule").reserved;
    r.sort();
    r
}

#[tokio::test]
async fn dod1_manually_reserved_sessions_are_left_alone() {
    let e = env("").await;
    let ids = disjoint(&e, 2);
    e.server.state.reserve_manually(&ids[0]);
    write_spec(&e, &[want(&ids[1], "reserved")]);
    let p = plan(&e).await;
    assert_eq!(p.unmanaged, vec![ids[0].clone()]);
    let r = run_apply(&e, &p, false).await.expect("apply");
    assert!(r.is_clean());
    assert!(writes(&e).iter().all(|w| !w.starts_with("cancel")), "{:?}", writes(&e));
    let mut expect = ids.clone();
    expect.sort();
    assert_eq!(reserved(&e).await, expect);
}

#[tokio::test]
async fn dod2_deleting_a_row_does_not_cancel() {
    let e = env("").await;
    let ids = disjoint(&e, 1);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    run_apply(&e, &p, false).await.expect("apply");
    write_spec(&e, &[]);
    let p2 = plan(&e).await;
    assert!(p2.actions.is_empty());
    assert_eq!(p2.unmanaged, ids);
    assert_eq!(reserved(&e).await, ids);
    // explicit want: none does cancel a managed reservation
    write_spec(&e, &[want(&ids[0], "none")]);
    let p3 = plan(&e).await;
    assert_eq!(p3.actions.len(), 1);
    run_apply(&e, &p3, false).await.expect("apply none");
    assert!(reserved(&e).await.is_empty());
}

#[tokio::test]
async fn dod3_reapplying_the_same_state_writes_nothing() {
    let e = env("").await;
    let ids = disjoint(&e, 3);
    write_spec(&e, &[want(&ids[0], "reserved"), want(&ids[1], "reserved"), want(&ids[2], "favorite")]);
    let p = plan(&e).await;
    assert_eq!(p.actions.len(), 3);
    run_apply(&e, &p, false).await.expect("apply");
    let before = writes(&e).len();
    let again = plan(&e).await;
    assert!(again.actions.is_empty(), "{:?}", again.actions);
    let r = run_apply(&e, &again, false).await.expect("noop apply");
    assert!(r.is_clean() && r.outcomes.is_empty());
    assert_eq!(writes(&e).len(), before, "no further writes");
}

#[tokio::test]
async fn dod4_unknown_outcome_is_never_resent_without_reconciling() {
    // The write is applied, then the connection drops.
    let e = env("drop-after-write").await;
    let ids = disjoint(&e, 1);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    let r = run_apply(&e, &p, false).await.expect("apply returns a report");
    assert_eq!(r.outcomes[0].status, ActionStatus::Unknown);
    assert_eq!(r.exit_code(), 5);
    assert_eq!(writes(&e).len(), 1, "exactly one write was sent");
    let resumed = run_resume(&e, &r.run_id).await.expect("resume");
    assert_eq!(resumed.outcomes[0].status, ActionStatus::Done, "reconciled from the schedule");
    assert_eq!(writes(&e).len(), 1, "reconciliation found it applied: nothing re-sent");
    assert_eq!(reserved(&e).await, ids);
}

#[tokio::test]
async fn dod4b_unknown_that_was_not_applied_is_sent_again_after_reconciling() {
    let e = env("edge-html-500").await;
    let ids = disjoint(&e, 1);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    let r = run_apply(&e, &p, false).await.expect("apply");
    assert_eq!(r.outcomes[0].status, ActionStatus::Unknown);
    assert!(reserved(&e).await.is_empty());
    let resumed = run_resume(&e, &r.run_id).await.expect("resume");
    assert_eq!(resumed.outcomes[0].status, ActionStatus::Done);
    assert_eq!(writes(&e).len(), 1);
    assert_eq!(reserved(&e).await, ids);
}

#[tokio::test]
async fn unknown_stops_later_writes() {
    let e = env("drop-after-write").await;
    let ids = disjoint(&e, 1);
    // a cancel (managed) comes first, then reserve+favorite; the first write drops, nothing else may be sent
    let fav = e.server.state.sessions.iter().find(|s| s.session_id != ids[0]).expect("s").session_id.clone();
    write_spec(&e, &[want(&ids[0], "reserved"), want(&fav, "favorite")]);
    let p = plan(&e).await;
    let r = run_apply(&e, &p, false).await.expect("apply");
    assert_eq!(writes(&e).len(), 1);
    assert_eq!(r.count(ActionStatus::Planned), 1, "the favorite was held back");
    let resumed = run_resume(&e, &r.run_id).await.expect("resume");
    assert!(resumed.is_clean(), "{:?}", resumed.outcomes);
    assert_eq!(writes(&e).len(), 2);
}

#[tokio::test]
async fn dod5_replace_needs_the_seat_loss_flag() {
    let e = env("").await;
    let (a, b) = overlapping_pair(&e);
    e.server.state.reserve_manually(&a);
    write_spec(&e, &[format!("  - {{id: {b}, want: reserved, replaces: {a}}}")]);
    let p = plan(&e).await;
    assert_eq!(p.required_flags, vec!["accept-seat-loss".to_string()]);
    let err = run_apply(&e, &p, false).await.expect_err("must refuse without the flag");
    assert_eq!(err.code, ErrorCode::PlanRejected);
    assert!(writes(&e).is_empty());
    let r = run_apply(&e, &p, true).await.expect("apply with flag");
    assert!(r.is_clean(), "{:?}", r.outcomes);
    assert_eq!(reserved(&e).await, vec![b]);
    assert_eq!(writes(&e).first().map(String::as_str), Some(format!("cancel {a}").as_str()));
}

#[tokio::test]
async fn failed_replacement_tries_to_restore_the_original_once() {
    let e = env("").await;
    let (a, b) = overlapping_pair(&e);
    e.server.state.reserve_manually(&a);
    write_spec(&e, &[format!("  - {{id: {b}, want: reserved, replaces: {a}}}")]);
    let p = plan(&e).await;
    // someone grabs the last seat of b between plan and apply
    let mut sc = Scenario::default();
    sc.full.insert(b.clone());
    e.server.state.set_scenario(sc);
    let r = run_apply(&e, &p, true).await.expect("apply");
    assert_eq!(r.outcomes[0].status, ActionStatus::Done);
    assert_eq!(r.outcomes[1].status, ActionStatus::Failed);
    assert!(r.notes.iter().any(|n| n.contains("restored")), "{:?}", r.notes);
    assert_eq!(reserved(&e).await, vec![a.clone()], "original seat is back");
    assert_eq!(writes(&e), vec![format!("cancel {a}"), format!("reserve {b}"), format!("reserve {a}")]);
    assert_eq!(r.exit_code(), 5);
}

#[tokio::test]
async fn closed_reservations_skip_the_kind_and_continue_with_the_rest() {
    let e = env("closed-reservations").await;
    let ids = disjoint(&e, 3);
    write_spec(&e, &[want(&ids[0], "reserved"), want(&ids[1], "reserved"), want(&ids[2], "favorite")]);
    let p = plan(&e).await;
    let r = run_apply(&e, &p, false).await.expect("apply");
    let by = |k: &str| r.outcomes.iter().filter(|o| o.kind == k).map(|o| o.status).collect::<Vec<_>>();
    assert_eq!(by("reserve"), vec![ActionStatus::Skipped, ActionStatus::Skipped]);
    assert_eq!(by("favorite"), vec![ActionStatus::Done]);
    assert_eq!(r.exit_code(), 5);
    assert!(writes(&e).iter().all(|w| w.starts_with("favorite")));
}

#[tokio::test]
async fn reservations_go_out_in_batches_of_ten() {
    let e = env("").await;
    let ids = disjoint(&e, 12);
    let rows: Vec<String> = ids.iter().map(|i| want(i, "reserved")).collect();
    write_spec(&e, &rows);
    let p = plan(&e).await;
    let r = run_apply(&e, &p, false).await.expect("apply");
    assert!(r.is_clean(), "{:?}", r.outcomes);
    let w: Vec<usize> = writes(&e).iter().map(|l| l.trim_start_matches("reserve ").split(',').count()).collect();
    assert_eq!(w, vec![10, 2]);
}

#[tokio::test]
async fn stale_plans_are_refused() {
    let e = env("").await;
    let ids = disjoint(&e, 2);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    // schedule drifts
    e.server.state.reserve_manually(&ids[1]);
    let err = run_apply(&e, &p, false).await.expect_err("schedule changed");
    assert!(err.message.contains("schedule changed"), "{}", err.message);
    // spec drifts
    let e2 = env("").await;
    let ids2 = disjoint(&e2, 2);
    write_spec(&e2, &[want(&ids2[0], "reserved")]);
    let p2 = plan(&e2).await;
    write_spec(&e2, &[want(&ids2[1], "reserved")]);
    let err = run_apply(&e2, &p2, false).await.expect_err("spec changed");
    assert!(err.message.contains("spec changed"), "{}", err.message);
    // expiry
    write_spec(&e2, &[want(&ids2[0], "reserved")]);
    let p3 = plan(&e2).await;
    let req = ApplyRequest {
        plan_id: Some(&p3.plan_id),
        resume_run: None,
        spec_path: &e2.spec,
        accept_seat_loss: false,
        now: Utc::now() + Duration::minutes(31),
        account: ACCT,
        lang: riv::i18n::Lang::En,
    };
    let err = apply(&e2.api, &e2.db, &e2.plans, &req, &|_| true).await.expect_err("expired");
    assert!(err.message.contains("expired"));
    // unknown plan id / missing plan
    let req = ApplyRequest { plan_id: Some("NOPE"), ..req };
    assert_eq!(
        apply(&e2.api, &e2.db, &e2.plans, &req, &|_| true).await.expect_err("none").code,
        ErrorCode::PlanRejected
    );
    assert!(e.server.state.writes().is_empty() && e2.server.state.writes().is_empty());
}

#[tokio::test]
async fn declined_confirmation_writes_nothing() {
    let e = env("").await;
    let ids = disjoint(&e, 1);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    let req = ApplyRequest {
        plan_id: Some(&p.plan_id),
        resume_run: None,
        spec_path: &e.spec,
        accept_seat_loss: false,
        now: Utc::now(),
        account: ACCT,
        lang: riv::i18n::Lang::En,
    };
    let err = apply(&e.api, &e.db, &e.plans, &req, &|_| false).await.expect_err("declined");
    assert_eq!(err.code, ErrorCode::PlanRejected);
    assert!(writes(&e).is_empty());
}

#[tokio::test]
async fn account_mismatch_is_refused() {
    let e = env("").await;
    let ids = disjoint(&e, 1);
    write_spec(&e, &[want(&ids[0], "reserved")]);
    let p = plan(&e).await;
    let req = ApplyRequest {
        plan_id: Some(&p.plan_id),
        resume_run: None,
        spec_path: &e.spec,
        accept_seat_loss: false,
        now: Utc::now(),
        account: "someone-else",
        lang: riv::i18n::Lang::En,
    };
    assert_eq!(apply(&e.api, &e.db, &e.plans, &req, &|_| true).await.expect_err("acct").code, ErrorCode::PlanRejected);
}

#[tokio::test]
async fn throttling_is_waited_out_and_full_sessions_fail_with_a_reason() {
    let e = env("throttle:3").await;
    let ids = disjoint(&e, 1);
    let full = e
        .server
        .state
        .sessions
        .iter()
        .find(|s| s.seat_availability.is_some_and(|a| a.is_full()) && s.is_reservable == Some(true))
        .expect("a full session")
        .session_id
        .clone();
    write_spec(&e, &[want(&ids[0], "reserved"), want(&full, "reserved")]);
    let p = plan(&e).await;
    assert!(p.warnings.iter().any(|w| w.contains("full")));
    let r = run_apply(&e, &p, false).await.expect("apply");
    let f = r.outcomes.iter().find(|o| o.target == full).expect("outcome");
    assert_eq!((f.status, f.detail.as_deref()), (ActionStatus::Failed, Some("full")));
    assert_eq!(r.count(ActionStatus::Done), 1);
    assert!(!r.verify.all_ok());
    let tasks = std::fs::read_to_string(e.spec.parent().expect("dir").join("tasks.md")).expect("tasks.md");
    assert!(tasks.contains("This file is generated by riv. Do not edit."));
    assert!(tasks.contains("- [x] Reserved"), "{tasks}");
    assert!(tasks.contains("- [ ] FAILED:"), "{tasks}");
}

#[tokio::test]
async fn blocks_are_created_once_and_their_ids_remembered() {
    let e = env("").await;
    let block = "  - {key: dinner, title: Dinner, description: Community, start: \"2026-12-01T19:00\", end: \"2026-12-01T21:00\"}";
    std::fs::write(&e.spec, spec_text(&[], block)).expect("spec");
    let p = plan(&e).await;
    assert_eq!(p.actions.len(), 1);
    let r = run_apply(&e, &p, false).await.expect("apply");
    assert!(r.is_clean(), "{:?}", r.outcomes);
    let sched = e.api.get_schedule(EVENT).await.expect("schedule");
    assert_eq!(
        (sched.personal_time[0].start_date_time.as_str(), sched.personal_time[0].end_date_time.as_str()),
        ("2026-12-02T03:00:00", "2026-12-02T05:00:00")
    );
    assert_eq!(e.db.block_ids(EVENT).expect("ids").get("dinner"), Some(&sched.personal_time[0].personal_time_id));
    assert!(plan(&e).await.actions.is_empty(), "no duplicate on the next plan");
}

#[tokio::test]
async fn blocks_can_be_removed_with_want_none_and_only_ours() {
    let e = env("").await;
    let block = |want: &str| {
        format!(
            "  - {{key: dinner, want: {want}, title: Dinner, description: Community, start: \"2026-12-01T19:00\", end: \"2026-12-01T21:00\"}}"
        )
    };
    std::fs::write(&e.spec, spec_text(&[], &block("present"))).expect("spec");
    let p = plan(&e).await;
    run_apply(&e, &p, false).await.expect("create");
    assert_eq!(e.api.get_schedule(EVENT).await.expect("s").personal_time.len(), 1);
    // remove it
    std::fs::write(&e.spec, spec_text(&[], &block("none"))).expect("spec");
    let p = plan(&e).await;
    assert_eq!(p.actions.len(), 1);
    let r = run_apply(&e, &p, false).await.expect("delete");
    assert!(r.is_clean(), "{:?}", r.outcomes);
    assert!(r.verify.all_ok());
    assert!(e.api.get_schedule(EVENT).await.expect("s").personal_time.is_empty());
    assert!(e.db.block_ids(EVENT).expect("ids").is_empty(), "mapping is forgotten");
    assert!(plan(&e).await.actions.is_empty(), "idempotent");
    assert_eq!(writes(&e), vec!["pt.create Dinner".to_string(), "pt.delete pt-1".to_string()]);
}
