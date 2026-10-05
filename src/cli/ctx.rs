//! Shared command context: API client, database, event time zone.

use crate::api::EventsApi;
use crate::api::http::{DEFAULT_BASE, HttpApi};
use crate::db::Db;
use crate::error::Result;

pub fn api_base() -> String {
    std::env::var("RIV_API_BASE").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| DEFAULT_BASE.to_string())
}

pub fn make_api() -> HttpApi {
    HttpApi::new(api_base(), crate::auth::provider_from_env())
}

pub fn open_db() -> Result<Db> {
    Db::open_default()
}

pub fn api_dyn() -> Box<dyn EventsApi> {
    Box::new(make_api())
}
