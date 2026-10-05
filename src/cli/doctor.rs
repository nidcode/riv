//! `riv doctor`: a read-only health check.

use super::ctx::*;
use crate::api::EventsApi;
use crate::auth::{FileTokenStore, TokenStore};
use crate::error::Result;
use serde::Serialize;

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

fn check(name: &'static str, ok: bool, detail: impl Into<String>) -> Check {
    Check { name, ok, detail: detail.into() }
}

pub async fn run(event: &str, json: bool) -> Result<i32> {
    let mut checks =
        vec![check("riv", true, format!("version {} on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS))];

    let auth = if std::env::var("RIV_TOKEN").is_ok_and(|t| !t.is_empty()) {
        check("auth", true, "using RIV_TOKEN (no sign-in)")
    } else {
        match FileTokenStore::default_location().load() {
            Ok(Some(c)) => {
                let left = c.expires_at - chrono::Utc::now().timestamp();
                let detail = if left > 0 {
                    format!("signed in; access token valid for {} min", left / 60)
                } else {
                    "signed in; access token expired (refreshes automatically)".into()
                };
                check("auth", true, detail)
            }
            Ok(None) => check("auth", false, "not signed in: run `riv login`"),
            Err(e) => check("auth", false, e.to_string()),
        }
    };
    checks.push(auth);

    let sync = match open_db().and_then(|db| Ok((db.last_sync(event)?, db.session_count(event)?))) {
        Ok((Some((version, n, locale)), stored)) => check(
            "catalog",
            true,
            format!(
                "{event}: {n} sessions, version {version}{} ({stored} stored)",
                locale.map(|l| format!(", locale {l}")).unwrap_or_default()
            ),
        ),
        Ok((None, _)) => check("catalog", false, format!("{event} not synced: run `riv sync --event {event}`")),
        Err(e) => check("catalog", false, e.to_string()),
    };
    checks.push(sync);

    let base = api_base();
    let reach = match tokio::time::timeout(std::time::Duration::from_secs(10), make_api().list_events()).await {
        Ok(Ok(evs)) => check("api", true, format!("{base} reachable ({} events)", evs.len())),
        Ok(Err(e)) => check("api", false, format!("{base}: {e}")),
        Err(_) => check("api", false, format!("{base}: timed out")),
    };
    checks.push(reach);

    let venues = crate::paths::venues_path();
    checks.push(if venues.exists() {
        check("venues.json", true, venues.display().to_string())
    } else {
        check(
            "venues.json",
            false,
            format!("missing (optional; copy config/venues.example.json to {} for walking times)", venues.display()),
        )
    });
    checks.push(check("data dir", true, crate::paths::data_dir().display().to_string()));

    if json {
        println!("{}", serde_json::to_string_pretty(&checks)?);
    } else {
        for c in &checks {
            println!("{} {:<12} {}", if c.ok { "ok " } else { "!! " }, c.name, c.detail);
        }
    }
    Ok(0)
}
