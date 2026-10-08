use riv::api::http::HttpApi;
use riv::auth::StaticToken;
use riv::db::Db;
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};
use riv::search::SearchQuery;
use riv::sync::{AbstractsMode, SyncOptions, sync};
use std::sync::Arc;
use std::sync::Mutex;

async fn setup() -> (MockServer, HttpApi) {
    let s = MockServer::start(0, Scenario::default()).await.expect("mock");
    let api = HttpApi::new(s.base_url.clone(), Arc::new(StaticToken(MOCK_TOKEN.into()))).with_sleep_scale(0.0);
    (s, api)
}

fn opts(mode: AbstractsMode) -> SyncOptions {
    SyncOptions { event_id: "demo-reinvent".into(), locale: None, abstracts: mode }
}

#[tokio::test]
async fn full_sync_then_noop_resync() {
    let (_s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    let phases = Mutex::new(Vec::new());
    let r = sync(&api, &db, &opts(AbstractsMode::Auto), &|p| phases.lock().expect("lock").push(p.phase))
        .await
        .expect("sync");
    assert_eq!(r.sessions, 120);
    assert_eq!(r.changed, 120);
    {
        let ph = phases.lock().expect("lock");
        assert!(ph.contains(&"catalog") && ph.contains(&"abstracts"));
    }
    assert_eq!(db.session_count("demo-reinvent").expect("count"), 120);
    let with_abs =
        db.all_sessions("demo-reinvent").expect("all").iter().filter(|s| s.session.abstract_.is_some()).count();
    assert_eq!(with_abs, 120);
    let r2 = sync(&api, &db, &opts(AbstractsMode::Auto), &|_| {}).await.expect("resync");
    assert_eq!(r2.changed, 0, "unchanged catalog must report 0 changes");
}

#[tokio::test]
async fn never_mode_keeps_stored_abstracts() {
    let (_s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    sync(&api, &db, &opts(AbstractsMode::Always), &|_| {}).await.expect("sync");
    let r = sync(&api, &db, &opts(AbstractsMode::Never), &|_| {}).await.expect("sync never");
    assert_eq!(r.changed, 0);
}

#[tokio::test]
async fn locale_mismatch_warns_and_stores_nothing() {
    let (_s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    let o = SyncOptions { locale: Some("ja-JP".into()), ..opts(AbstractsMode::Auto) };
    let r = sync(&api, &db, &o, &|_| {}).await.expect("sync");
    assert!(r.locale_stored.is_none());
    assert_eq!(r.warnings.len(), 1);
}

#[tokio::test]
async fn search_and_filters() {
    let (_s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    sync(&api, &db, &opts(AbstractsMode::Auto), &|_| {}).await.expect("sync");
    let hits = db
        .search("demo-reinvent", &SearchQuery { text: "bedrock".into(), limit: 5, ..Default::default() }, &[])
        .expect("search");
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.session.services.iter().any(|s| s.contains("Bedrock"))
        || h.session.title.contains("Bedrock")
        || h.session.abstract_.as_deref().is_some_and(|a| a.contains("Bedrock"))));
    let lvl = db
        .search(
            "demo-reinvent",
            &SearchQuery { text: String::new(), level: Some(300), limit: 200, ..Default::default() },
            &[],
        )
        .expect("level");
    assert!(!lvl.is_empty() && lvl.iter().all(|h| h.session.level.as_deref().is_some_and(|l| l.starts_with("300"))));
    let r1 = db.find_sessions("demo-reinvent", "aim301-r1").expect("find");
    assert!(r1.len() <= 1);
    let any_r1 = db
        .all_sessions("demo-reinvent")
        .expect("all")
        .into_iter()
        .find(|s| s.session.code().ends_with("-R1"))
        .expect("r1");
    let found = db.find_sessions("demo-reinvent", &any_r1.session.code().to_lowercase()).expect("by code");
    assert_eq!(found[0].session.session_id, any_r1.session.session_id);
    let tue = db
        .search("demo-reinvent", &SearchQuery { day: Some("tue".into()), limit: 200, ..Default::default() }, &[])
        .expect("tue");
    assert!(tue.iter().all(|h| h.session.session_time.as_ref().and_then(|t| t.date.as_deref()) == Some("2026-12-01")));
    // multi-word natural language falls back to OR instead of returning nothing
    let nl = db
        .search(
            "demo-reinvent",
            &SearchQuery { text: "how do I secure serverless zzzzqq".into(), limit: 5, ..Default::default() },
            &[],
        )
        .expect("nl");
    assert!(!nl.is_empty());
}

#[tokio::test]
async fn generated_questions_are_deterministic_single_target_and_avoid_the_title() {
    let (_s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    sync(&api, &db, &opts(AbstractsMode::Always), &|_| {}).await.expect("sync");
    let a = riv::bench::generate_questions(&db, "demo-reinvent", 10).expect("gen");
    let b = riv::bench::generate_questions(&db, "demo-reinvent", 10).expect("gen");
    assert_eq!(a.len(), 10);
    assert_eq!(a.iter().map(|q| &q.query).collect::<Vec<_>>(), b.iter().map(|q| &q.query).collect::<Vec<_>>());
    for q in &a {
        assert_eq!(q.expected.len(), 1);
        let s = db.find_sessions("demo-reinvent", &q.expected[0]).expect("find").remove(0).session;
        let title = s.title.to_lowercase();
        // the first words are abstract words that are not part of the title
        assert!(q.query.split_whitespace().take(1).all(|w| !title.contains(w)), "{} / {}", q.query, s.title);
    }
}

#[tokio::test]
async fn an_empty_answer_never_wipes_the_local_catalog() {
    let (s, api) = setup().await;
    let db = Db::open_memory().expect("db");
    sync(&api, &db, &opts(AbstractsMode::Auto), &|_| {}).await.expect("first sync");
    assert_eq!(db.session_count("demo-reinvent").expect("count"), 120);
    s.state.set_scenario(Scenario::parse("empty-catalog"));
    let err = sync(&api, &db, &opts(AbstractsMode::Auto), &|_| {}).await.expect_err("must refuse");
    assert!(err.message.contains("local catalog was kept"), "{}", err.message);
    assert_eq!(db.session_count("demo-reinvent").expect("count"), 120, "nothing was deleted");
    // search still works
    assert!(
        !db.search("demo-reinvent", &SearchQuery { text: "bedrock".into(), limit: 3, ..Default::default() }, &[])
            .expect("search")
            .is_empty()
    );
    // a first sync into an empty database with an empty answer is allowed (nothing to lose)
    let fresh = Db::open_memory().expect("db");
    let r = sync(&api, &fresh, &opts(AbstractsMode::Auto), &|_| {}).await.expect("fresh sync");
    assert_eq!(r.sessions, 0);
}
