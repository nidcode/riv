use super::ctx::*;
use crate::api::EventsApi;
use crate::error::{Result, RivError};
use crate::format::{self, DEFAULT_FIELDS};
use crate::i18n::{t, tf};
use crate::search::{SearchQuery, parse_window};
use crate::sync::{AbstractsMode, SyncOptions};
use std::io::Write;

pub async fn events(json: bool, past: bool) -> Result<i32> {
    let _ = past; // ListEvents' includePast is an optional server filter; default listing is what users need.
    let evs = make_api().list_events().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&evs)?);
        return Ok(0);
    }
    for e in evs {
        let auth = if e.authentication_required { "sign-in required" } else { "public" };
        println!("{}  {}  [{}]  {} .. {}", e.event_id, e.name, auth, e.start_date, e.end_date);
    }
    Ok(0)
}

pub async fn sync(event: &str, locale: Option<String>, abstracts: &str) -> Result<i32> {
    let mode: AbstractsMode = abstracts.parse().map_err(RivError::validation)?;
    let api = make_api();
    let db = open_db()?;
    let opts = SyncOptions { event_id: event.to_string(), locale, abstracts: mode };
    let r = crate::sync::sync(&api, &db, &opts, &|p| {
        eprint!("\r{:<10} {}/{}   ", p.phase, p.done, p.total);
        let _ = std::io::stderr().flush();
    })
    .await?;
    eprintln!();
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    println!(
        "{}",
        tf(
            "Synced {n} sessions ({changed} changed, {removed} removed). Catalog version {version}.",
            &[
                ("n", &r.sessions.to_string()),
                ("changed", &r.changed.to_string()),
                ("removed", &r.removed.to_string()),
                ("version", &r.catalog_version)
            ]
        )
    );
    Ok(0)
}

pub struct SearchArgs {
    pub query: String,
    pub event: String,
    pub level: Option<u32>,
    pub day: Option<String>,
    pub topic: Option<String>,
    pub service: Option<String>,
    pub venue: Option<String>,
    pub free_between: Option<String>,
    pub limit: usize,
    pub json: bool,
    pub fields: Option<String>,
}

pub async fn search(a: SearchArgs) -> Result<i32> {
    let db = open_db()?;
    let tz = event_tz(&db, &a.event);
    let fields_owned: Vec<String> = a
        .fields
        .as_deref()
        .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| DEFAULT_FIELDS.iter().map(|s| s.to_string()).collect());
    if let Some(bad) = fields_owned.iter().find(|f| !format::valid_field(f)) {
        return Err(RivError::validation(format!("unknown field `{bad}`")));
    }
    let window = match &a.free_between {
        Some(w) => Some(parse_window(w).ok_or_else(|| RivError::validation("--free-between expects HH:MM-HH:MM"))?),
        None => None,
    };
    let busy = if window.is_some() {
        crate::schedule::busy_ranges(&make_api(), &db, &a.event, tz).await.unwrap_or_else(|e| {
            eprintln!("warning: schedule unavailable ({e}); ignoring reservations");
            vec![]
        })
    } else {
        vec![]
    };
    let q = SearchQuery {
        text: a.query,
        level: a.level,
        day: a.day,
        topic: a.topic,
        service: a.service,
        venue: a.venue,
        free_between: window,
        limit: a.limit,
    };
    let hits = db.search(&a.event, &q, &busy)?;
    if a.json {
        let fields: Vec<&str> = fields_owned.iter().map(String::as_str).collect();
        let rows: Vec<serde_json::Value> = hits
            .iter()
            .map(|h| {
                fields
                    .iter()
                    .map(|f| (f.to_string(), serde_json::Value::String(format::field(&h.session, f, tz))))
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            })
            .collect();
        println!("{}", serde_json::to_string(&rows)?);
    } else if hits.is_empty() {
        println!("{}", t("No results."));
    } else {
        let fields: Vec<&str> = fields_owned.iter().map(String::as_str).collect();
        for h in hits {
            println!("{}", format::line(&h.session, &fields, tz));
        }
    }
    Ok(0)
}

pub fn show(event: &str, id: &str, json: bool) -> Result<i32> {
    let db = open_db()?;
    let tz = event_tz(&db, event);
    let found = db.find_sessions(event, id)?;
    if found.is_empty() {
        return Err(RivError::general(format!("no session `{id}` in the local catalog (run `riv sync`)")));
    }
    for f in found {
        let s = &f.session;
        if json {
            println!("{}", serde_json::to_string_pretty(s)?);
            continue;
        }
        println!("{}  {}", s.code(), s.title);
        println!("id: {}", s.session_id);
        println!(
            "when: {}  ({} min)  where: {} {}",
            format::start_local(s, tz),
            s.session_time.as_ref().and_then(|t| t.length.clone()).unwrap_or_default(),
            s.venue.clone().unwrap_or_default(),
            s.room.clone().unwrap_or_default()
        );
        println!(
            "type: {}  level: {}  seats: {}",
            format::field(s, "type", tz),
            format::field(s, "level", tz),
            format::field(s, "seat", tz)
        );
        if !s.topics.is_empty() {
            println!("topics: {}", s.topics.join(", "));
        }
        if !s.services.is_empty() {
            println!("services: {}", s.services.join(", "));
        }
        let sp: Vec<_> = s.speakers.iter().filter_map(|x| x.name.clone()).collect();
        if !sp.is_empty() {
            println!("speakers: {}", sp.join(", "));
        }
        if let Some(a) = &s.abstract_ {
            println!("\n{a}");
        }
        if let Some(l) = f.l10n {
            println!("\n[{}] {}", l.locale, l.title.unwrap_or_default());
        }
        println!();
    }
    Ok(0)
}
