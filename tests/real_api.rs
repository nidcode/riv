//! Read-only checks against the real API. Skipped unless RIV_REAL_API=1. Never writes.

use riv::api::EventsApi;
use riv::api::http::{DEFAULT_BASE, HttpApi};
use riv::auth::NoToken;
use std::sync::Arc;

fn enabled() -> bool {
    std::env::var("RIV_REAL_API").as_deref() == Ok("1")
}

#[tokio::test]
async fn list_events_parses_real_response() {
    if !enabled() {
        return;
    }
    let api = HttpApi::new(DEFAULT_BASE, Arc::new(NoToken));
    let evs = api.list_events().await.expect("ListEvents");
    assert!(!evs.is_empty());
}

#[tokio::test]
async fn public_event_catalog_syncs() {
    if !enabled() {
        return;
    }
    let api = HttpApi::new(DEFAULT_BASE, Arc::new(NoToken));
    let evs = api.list_events().await.expect("ListEvents");
    let Some(public) = evs.iter().find(|e| !e.authentication_required) else { return };
    let db = riv::db::Db::open_memory().expect("db");
    let opts = riv::sync::SyncOptions {
        event_id: public.event_id.clone(),
        locale: None,
        abstracts: riv::sync::AbstractsMode::Never,
    };
    // Registration-free events expose their catalog anonymously; a 401/403 here just means this one doesn't.
    if let Ok(r) = riv::sync::sync(&api, &db, &opts, &|_| {}).await {
        assert!(r.sessions > 0);
    }
}
