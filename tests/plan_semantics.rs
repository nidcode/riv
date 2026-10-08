//! Brief §7: the desired-state semantics, fixed as table-driven tests. All offline.

use chrono::{TimeZone, Utc};
use riv::api::{PersonalTime, Schedule, SeatAvailability, Session, SessionTime};
use riv::desired::{Want, parse};
use riv::error::ErrorCode;
use riv::plan::*;
use std::collections::HashMap;

struct VecCatalog(Vec<Session>);

impl CatalogView for VecCatalog {
    fn session(&self, id: &str) -> Option<Session> {
        self.0.iter().find(|s| s.session_id == id).cloned()
    }
    fn same_title(&self, s: &Session) -> Vec<Session> {
        self.0.iter().filter(|o| o.title == s.title).cloned().collect()
    }
    fn tz(&self) -> chrono_tz::Tz {
        chrono_tz::America::Los_Angeles
    }
}

fn sess(id: &str, code: &str, title: &str, date: &str, time: &str, seat: SeatAvailability) -> Session {
    Session {
        session_id: id.into(),
        title: title.into(),
        abbreviation: Some(code.into()),
        venue: Some("Venetian".into()),
        is_reservable: Some(true),
        seat_availability: Some(seat),
        session_time: Some(SessionTime {
            date: Some(date.into()),
            time: Some(time.into()),
            length: Some("60".into()),
            timezone: None,
        }),
        ..Default::default()
    }
}

fn catalog() -> VecCatalog {
    use SeatAvailability::*;
    VecCatalog(vec![
        sess("a1", "AIM301-R1", "Agents", "2026-12-01", "10:00", Available),
        sess("b1", "SVS302", "Serverless", "2026-12-01", "10:00", Available), // overlaps a1
        sess("c1", "DAT201", "Data", "2026-12-01", "13:00", Available),
        sess("d1", "SEC400-R1", "Full thing", "2026-12-02", "09:00", Unavailable),
        sess("d2", "SEC400-R2", "Full thing", "2026-12-03", "09:00", Available),
        sess("e1", "CMP101", "Compute", "2026-12-04", "09:00", Available),
    ])
}

fn spec(sessions: &str, blocks: &str) -> String {
    format!(
        "# riv:desired-state v1\nevent: e\ntimezone: America/Los_Angeles\nsessions:\n{sessions}\nblocks:\n{blocks}\n"
    )
}

#[derive(Default)]
struct World {
    reserved: Vec<&'static str>,
    favorites: Vec<&'static str>,
    personal: Vec<PersonalTime>,
    managed: Vec<(&'static str, Want)>,
    block_ids: Vec<(&'static str, &'static str)>,
}

fn run(yaml: &str, w: &World) -> riv::error::Result<Plan> {
    let desired = parse(yaml)?;
    let schedule = Schedule {
        reserved: w.reserved.iter().map(|s| s.to_string()).collect(),
        favorites: w.favorites.iter().map(|s| s.to_string()).collect(),
        personal_time: w.personal.clone(),
    };
    let managed: HashMap<String, Want> = w.managed.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let block_ids: HashMap<String, String> = w.block_ids.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let cat = catalog();
    make_plan(&PlanInput {
        desired: &desired,
        schedule: &schedule,
        managed: &managed,
        block_ids: &block_ids,
        catalog: &cat,
        now: Utc.with_ymd_and_hms(2026, 10, 20, 10, 0, 0).single().expect("time"),
        plan_id: "01TEST".into(),
        catalog_version: "v1".into(),
        account: "acct".into(),
    })
}

fn kinds(p: &Plan) -> Vec<(String, String)> {
    p.actions.iter().map(|a| (a.kind.as_str().to_string(), a.target())).collect()
}

fn kv(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn manually_reserved_sessions_are_untouched_but_listed() {
    let w = World { reserved: vec!["c1"], ..Default::default() };
    let p = run(&spec("  - {id: e1, want: reserved}", "  []"), &w).expect("plan");
    assert_eq!(kinds(&p), kv(&[("reserve", "e1")]));
    assert_eq!(p.unmanaged, vec!["c1".to_string()]);
}

#[test]
fn deleting_a_row_never_cancels() {
    let w = World { reserved: vec!["e1"], managed: vec![("e1", Want::Reserved)], ..Default::default() };
    let p = run(&spec("  []", "  []"), &w).expect("plan");
    assert!(p.actions.is_empty());
    assert_eq!(p.unmanaged, vec!["e1".to_string()]);
}

#[test]
fn want_none_cancels_only_managed() {
    let yaml = spec("  - {id: e1, want: none}", "  []");
    let managed =
        run(&yaml, &World { reserved: vec!["e1"], managed: vec![("e1", Want::Reserved)], ..Default::default() })
            .expect("plan");
    assert_eq!(kinds(&managed), kv(&[("cancel", "e1")]));
    assert!(managed.required_flags.is_empty(), "an explicit cancel is not a seat-loss replace");
    let unmanaged = run(&yaml, &World { reserved: vec!["e1"], ..Default::default() }).expect("plan");
    assert!(unmanaged.actions.is_empty());
    assert!(unmanaged.warnings.iter().any(|w| w.contains("not managed")));
}

#[test]
fn managed_reservation_gone_is_rereserve() {
    let w = World { managed: vec![("e1", Want::Reserved)], ..Default::default() };
    let p = run(&spec("  - {id: e1, want: reserved}", "  []"), &w).expect("plan");
    assert_eq!(p.actions.len(), 1);
    assert!(p.actions[0].rereserve);
    assert!(render_plan(&p, &catalog()).contains("! re-reserve (may be full)"));
    let fresh = run(&spec("  - {id: e1, want: reserved}", "  []"), &World::default()).expect("plan");
    assert!(!fresh.actions[0].rereserve);
}

#[test]
fn pinned_sessions_are_never_planned_but_count_for_clashes() {
    let w = World { reserved: vec!["a1"], ..Default::default() };
    // pinned a1 desired as `none` must not be cancelled; b1 overlaps it and gets a clash hint
    let p = run(&spec("  - {id: a1, want: none, pin: true}\n  - {id: b1, want: reserved}", "  []"), &w).expect("plan");
    assert_eq!(kinds(&p), kv(&[("reserve", "b1")]));
    assert!(p.warnings.iter().any(|w| w.contains("overlaps")), "{:?}", p.warnings);
    assert!(!p.unmanaged.contains(&"a1".to_string()));
    let pinned_missing =
        run(&spec("  - {id: e1, want: reserved, pin: true}", "  []"), &World::default()).expect("plan");
    assert!(pinned_missing.actions.is_empty());
}

#[test]
fn replace_is_cancel_then_reserve_and_needs_the_seat_loss_flag() {
    let w = World { reserved: vec!["a1"], ..Default::default() };
    let p = run(&spec("  - {id: b1, want: reserved, replaces: a1}", "  []"), &w).expect("plan");
    assert_eq!(kinds(&p), kv(&[("cancel", "a1"), ("reserve", "b1")]));
    assert_eq!(p.actions[0].risk.as_deref(), Some("seat-loss"));
    assert_eq!(p.actions[1].depends_on, vec![1]);
    assert_eq!(p.required_flags, vec!["accept-seat-loss".to_string()]);
    let shown = render_plan(&p, &catalog());
    assert!(shown.contains("-/+ replace (seat may be lost)"), "{shown}");
    assert!(shown.contains("--accept-seat-loss"));
    assert!(!p.warnings.iter().any(|w| w.contains("overlaps")), "replaced session must not count as a clash");
    assert!(p.unmanaged.is_empty(), "the replaced session is addressed, not unmanaged");
}

#[test]
fn replace_when_target_already_held_cancels_without_risk() {
    let w = World { reserved: vec!["a1", "b1"], ..Default::default() };
    let p = run(&spec("  - {id: b1, want: reserved, replaces: a1}", "  []"), &w).expect("plan");
    assert_eq!(kinds(&p), kv(&[("cancel", "a1")]));
    assert!(p.required_flags.is_empty());
}

#[test]
fn applying_the_same_state_again_writes_nothing() {
    let yaml = spec("  - {id: e1, want: reserved}\n  - {id: c1, want: favorite}", "  []");
    let before = run(&yaml, &World::default()).expect("plan");
    assert_eq!(before.actions.len(), 2);
    let after = run(
        &yaml,
        &World {
            reserved: vec!["e1"],
            favorites: vec!["c1"],
            managed: vec![("e1", Want::Reserved), ("c1", Want::Favorite)],
            ..Default::default()
        },
    )
    .expect("plan");
    assert!(after.actions.is_empty());
}

#[test]
fn full_session_lists_alternative_runs() {
    let p = run(&spec("  - {id: d1, want: reserved}", "  []"), &World::default()).expect("plan");
    assert_eq!(p.alternatives.get("d1"), Some(&vec!["d2".to_string()]));
    assert!(render_plan(&p, &catalog()).contains("SEC400-R2"));
}

#[test]
fn validation_errors_reject_the_plan() {
    let w = World::default();
    for (yaml, needle) in [
        (spec("  - {id: nope, want: reserved}", "  []"), "unknown sessionId"),
        (spec("  - {id: a1, want: reserved}\n  - {id: a1, want: favorite}", "  []"), "duplicate"),
        (spec("  - {id: a1, want: reserved, replaces: a1}", "  []"), "replaces itself"),
        (
            spec(
                "  []",
                "  - {key: k, title: t, description: d, start: \"2026-12-01T19:00\", end: \"2026-12-01T19:03\"}",
            ),
            "multiple of 5",
        ),
    ] {
        let e = run(&yaml, &w).expect_err("must reject");
        assert_eq!(e.code, ErrorCode::Validation);
        assert!(e.message.contains(needle), "{}", e.message);
    }
}

#[test]
fn favorites_and_blocks() {
    let block =
        "  - {key: dinner, title: Dinner, description: D, start: \"2026-12-01T19:00\", end: \"2026-12-01T21:00\"}";
    let p = run(&spec("  - {id: c1, want: favorite}", block), &World::default()).expect("plan");
    assert_eq!(kinds(&p), kv(&[("favorite", "c1"), ("block.create", "dinner")]));
    let b = p.actions.iter().find(|a| a.key.is_some()).expect("block");
    assert_eq!(
        (b.start_utc.as_deref(), b.end_utc.as_deref()),
        (Some("2026-12-02T03:00:00"), Some("2026-12-02T05:00:00"))
    );

    let existing = PersonalTime {
        personal_time_id: "pt1".into(),
        start_date_time: "2026-12-02T03:00:00".into(),
        end_date_time: "2026-12-02T05:00:00".into(),
        title: "Dinner".into(),
        description: "D".into(),
        location: None,
    };
    let same = run(
        &spec("  []", block),
        &World { personal: vec![existing.clone()], block_ids: vec![("dinner", "pt1")], ..Default::default() },
    )
    .expect("plan");
    assert!(same.actions.is_empty());
    let changed = PersonalTime { description: "old".into(), ..existing.clone() };
    let upd = run(
        &spec("  []", block),
        &World { personal: vec![changed], block_ids: vec![("dinner", "pt1")], ..Default::default() },
    )
    .expect("plan");
    assert_eq!(kinds(&upd), kv(&[("block.update", "dinner")]));
    assert_eq!(upd.actions[0].personal_time_id.as_deref(), Some("pt1"));
    // an identical entry created by hand is adopted, not duplicated
    let adopt = run(&spec("  []", block), &World { personal: vec![existing], ..Default::default() }).expect("plan");
    assert!(adopt.actions.is_empty());
    assert!(adopt.warnings.iter().any(|w| w.contains("identical")));
}

#[test]
fn hashes_are_order_insensitive_and_plan_expires_in_30_minutes() {
    let a = Schedule { reserved: vec!["x".into(), "y".into()], favorites: vec![], personal_time: vec![] };
    let b = Schedule { reserved: vec!["y".into(), "x".into()], ..Default::default() };
    assert_eq!(observed_hash(&a), observed_hash(&b));
    let c = Schedule { reserved: vec!["x".into()], ..Default::default() };
    assert_ne!(observed_hash(&a), observed_hash(&c));
    let p = run(&spec("  - {id: e1, want: reserved}", "  []"), &World::default()).expect("plan");
    assert_eq!((p.created_at.as_str(), p.expires_at.as_str()), ("2026-10-20T10:00:00Z", "2026-10-20T10:30:00Z"));
    assert!(!p.expired_at(Utc.with_ymd_and_hms(2026, 10, 20, 10, 29, 59).single().expect("t")));
    assert!(p.expired_at(Utc.with_ymd_and_hms(2026, 10, 20, 10, 30, 0).single().expect("t")));
}

#[test]
fn plan_roundtrips_through_json() {
    let w = World { reserved: vec!["a1"], ..Default::default() };
    let p = run(&spec("  - {id: b1, want: reserved, replaces: a1}", "  []"), &w).expect("plan");
    let dir = tempfile::tempdir().expect("tmp");
    save_plan(&p, dir.path()).expect("save");
    assert_eq!(load_plan(&p.plan_id, dir.path()).expect("load"), p);
    assert!(load_plan("../etc", dir.path()).is_err());
    assert!(load_plan("MISSING", dir.path()).is_err());
}

#[test]
fn block_want_none_deletes_only_blocks_riv_created() {
    let gone = "  - {key: dinner, want: none, title: Dinner, description: D, start: \"2026-12-01T19:00\", end: \"2026-12-01T21:00\"}";
    let pt = PersonalTime {
        personal_time_id: "pt1".into(),
        start_date_time: "2026-12-02T03:00:00".into(),
        end_date_time: "2026-12-02T05:00:00".into(),
        title: "Dinner".into(),
        description: "D".into(),
        location: None,
    };
    // riv created it (id remembered): delete
    let p = run(
        &spec("  []", gone),
        &World { personal: vec![pt.clone()], block_ids: vec![("dinner", "pt1")], ..Default::default() },
    )
    .expect("plan");
    assert_eq!(kinds(&p), kv(&[("block.delete", "dinner")]));
    assert_eq!(p.actions[0].personal_time_id.as_deref(), Some("pt1"));
    assert!(render_plan(&p, &catalog()).contains("- block.delete"));
    // an identical entry made by hand is not ours: warn, keep
    let p = run(&spec("  []", gone), &World { personal: vec![pt], ..Default::default() }).expect("plan");
    assert!(p.actions.is_empty());
    assert!(p.warnings.iter().any(|w| w.contains("did not create")), "{:?}", p.warnings);
    // already gone: nothing to do
    let p =
        run(&spec("  []", gone), &World { block_ids: vec![("dinner", "pt1")], ..Default::default() }).expect("plan");
    assert!(p.actions.is_empty());
}
