//! Shared command context: API client, database, event time zone.

use crate::api::EventsApi;
use crate::api::http::{DEFAULT_BASE, HttpApi};
use crate::db::Db;
use crate::error::Result;
use chrono_tz::Tz;

pub fn api_base() -> String {
    std::env::var("RIV_API_BASE").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| DEFAULT_BASE.to_string())
}

pub fn make_api() -> HttpApi {
    HttpApi::new(api_base(), crate::auth::provider_from_env())
}

pub fn open_db() -> Result<Db> {
    Db::open_default()
}

/// The event's time zone from the stored event; America/Los_Angeles until the first sync.
pub fn event_tz(db: &Db, event_id: &str) -> Tz {
    db.event(event_id)
        .ok()
        .flatten()
        .and_then(|e| e.timezone)
        .and_then(|t| crate::timeutil::parse_tz(&t))
        .unwrap_or(chrono_tz::America::Los_Angeles)
}

pub fn api_dyn() -> Box<dyn EventsApi> {
    Box::new(make_api())
}
