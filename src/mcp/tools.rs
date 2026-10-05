//! Tool logic for the local MCP server. Plain async functions returning text, so they are testable
//! without the MCP protocol. Replies stay small: the catalog never goes through the model's context.

use crate::api::EventsApi;
use crate::apply::{ApplyRequest, apply, render_report, verify};
use crate::db::Db;
use crate::error::{Result, RivError};
use crate::format::{self, DEFAULT_FIELDS, Format, cap_reply};
use crate::plan::service::{create_plan, event_tz_of, read_desired};
use crate::plan::{DbCatalog, FLAG_SEAT_LOSS};
use crate::search::{SearchQuery, parse_window};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct Tools {
    pub api: Arc<dyn EventsApi>,
    pub db_path: PathBuf,
    pub plans_dir: PathBuf,
    pub default_event: String,
    pub default_spec: PathBuf,
    pub account: String,
}

/// Run a non-`Send` future (it borrows the SQLite connection) on a blocking thread.
pub async fn on_blocking<T, F>(make: impl FnOnce() -> F + Send + 'static) -> Result<T>
where
    T: Send + 'static,
    F: Future<Output = T>,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(make()))
        .await
        .map_err(|e| RivError::general(format!("tool task failed: {e}")))
}

#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub query: String,
    pub level: Option<u32>,
    pub day: Option<String>,
    pub topic: Option<String>,
    pub service: Option<String>,
    pub venue: Option<String>,
    pub free_between: Option<String>,
    pub limit: Option<usize>,
    pub fields: Option<Vec<String>>,
    pub event: Option<String>,
}

impl Tools {
    fn event(&self, e: &Option<String>) -> String {
        e.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| self.default_event.clone())
    }

    pub async fn status(&self) -> Result<String> {
        use crate::auth::{FileTokenStore, TokenStore};
        let path = self.db_path.clone();
        let event = self.default_event.clone();
        let sync = on_blocking(move || async move {
            let db = Db::open(&path)?;
            Ok::<_, RivError>((db.last_sync(&event)?, db.session_count(&event)?))
        })
        .await??;
        let auth = if std::env::var("RIV_TOKEN").is_ok_and(|t| !t.is_empty()) {
            "static token (RIV_TOKEN)".to_string()
        } else {
            match FileTokenStore::default_location().load() {
                Ok(Some(c)) if c.expires_at > chrono::Utc::now().timestamp() => "signed in".to_string(),
                Ok(Some(_)) => "signed in (access token expired; refreshes automatically)".to_string(),
                _ => "not signed in: run `riv login`".to_string(),
            }
        };
        let sync_line = match sync.0 {
            Some((version, n, locale)) => format!(
                "synced {n} sessions, catalog version {version}{}",
                locale.map(|l| format!(", locale {l}")).unwrap_or_default()
            ),
            None => "never synced: run `riv sync`".to_string(),
        };
        Ok(format!("event: {}\nauth: {auth}\ncatalog: {sync_line} ({} stored)\n", self.default_event, sync.1))
    }

    pub async fn search(&self, p: SearchParams, fmt: Format) -> Result<String> {
        let event = self.event(&p.event);
        let window = match &p.free_between {
            Some(w) => Some(parse_window(w).ok_or_else(|| RivError::validation("freeBetween expects HH:MM-HH:MM"))?),
            None => None,
        };
        let fields: Vec<String> = p
            .fields
            .clone()
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| DEFAULT_FIELDS.iter().map(|s| s.to_string()).collect());
        if let Some(bad) = fields.iter().find(|f| !format::valid_field(f)) {
            return Err(RivError::validation(format!("unknown field `{bad}`")));
        }
        let api = self.api.clone();
        let path = self.db_path.clone();
        let q = SearchQuery {
            text: p.query,
            level: p.level,
            day: p.day,
            topic: p.topic,
            service: p.service,
            venue: p.venue,
            free_between: window,
            limit: p.limit.unwrap_or(10).clamp(1, 25),
        };
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let tz = db.event_tz(&event);
            let busy = if q.free_between.is_some() {
                crate::schedule::busy_ranges(api.as_ref(), &db, &event, tz).await.unwrap_or_default()
            } else {
                vec![]
            };
            let hits = db.search(&event, &q, &busy)?;
            if hits.is_empty() {
                return Ok::<_, RivError>("No results.\n".to_string());
            }
            let sessions: Vec<&crate::api::Session> = hits.iter().map(|h| &h.session).collect();
            let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            Ok(cap_reply(format::render_sessions(&sessions, &fields, tz, fmt)))
        })
        .await?
    }

    pub async fn session(&self, id: &str, event: Option<String>, fmt: Format) -> Result<String> {
        let event = self.event(&event);
        let (path, id) = (self.db_path.clone(), id.to_string());
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let tz = db.event_tz(&event);
            let found = db.find_sessions(&event, &id)?;
            if found.is_empty() {
                return Err(RivError::general(format!("no session `{id}` in the local catalog (run `riv sync`)")));
            }
            let mut out = String::new();
            for f in found.iter().take(3) {
                let s = &f.session;
                out.push_str(&format!(
                    "{}  {}\nid: {}\nwhen: {}  where: {} {}\n",
                    s.code(),
                    s.title,
                    s.session_id,
                    format::start_local(s, tz),
                    s.venue.clone().unwrap_or_default(),
                    s.room.clone().unwrap_or_default()
                ));
                out.push_str(&format!(
                    "type: {}  level: {}  seats: {}\n",
                    format::field(s, "type", tz),
                    format::field(s, "level", tz),
                    format::field(s, "seat", tz)
                ));
                if !s.topics.is_empty() {
                    out.push_str(&format!("topics: {}\n", s.topics.join(", ")));
                }
                if !s.services.is_empty() {
                    out.push_str(&format!("services: {}\n", s.services.join(", ")));
                }
                if fmt == Format::Ide
                    && let Some(a) = &s.abstract_
                {
                    out.push_str(&format!("\n{}\n", a.chars().take(1200).collect::<String>()));
                }
                out.push('\n');
            }
            Ok(cap_reply(out))
        })
        .await?
    }

    pub async fn schedule(&self, event: Option<String>) -> Result<String> {
        let event = self.event(&event);
        let (api, path) = (self.api.clone(), self.db_path.clone());
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let tz = db.event_tz(&event);
            let sched = api.get_schedule(&event).await?;
            Ok::<_, RivError>(cap_reply(crate::schedule::render_schedule(&db, &event, tz, &sched)?))
        })
        .await?
    }

    fn spec(&self, spec: &Option<String>) -> PathBuf {
        spec.as_deref().filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| self.default_spec.clone())
    }

    pub async fn plan(&self, spec: Option<String>) -> Result<String> {
        let (api, path, plans, account, spec) =
            (self.api.clone(), self.db_path.clone(), self.plans_dir.clone(), self.account.clone(), self.spec(&spec));
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let out = create_plan(api.as_ref(), &db, &spec, &plans, &account).await?;
            let flags = if out.plan.required_flags.is_empty() { "none".to_string() } else { out.plan.required_flags.join(", ") };
            Ok::<_, RivError>(cap_reply(format!(
                "planId: {}\nrequiredFlags: {flags}\nThis plan has NOT been applied. Show the diff to the user and wait for explicit approval.\n\n{}",
                out.plan.plan_id, out.rendered
            )))
        })
        .await?
    }

    pub async fn apply(
        &self,
        plan_id: Option<String>,
        resume_run: Option<String>,
        accept_seat_loss: bool,
        spec: Option<String>,
    ) -> Result<String> {
        if plan_id.as_deref().is_none_or(str::is_empty) && resume_run.as_deref().is_none_or(str::is_empty) {
            return Err(RivError::plan_rejected("planId is required: take it from the preceding riv_plan"));
        }
        let (api, path, plans, account, spec) =
            (self.api.clone(), self.db_path.clone(), self.plans_dir.clone(), self.account.clone(), self.spec(&spec));
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let req = ApplyRequest {
                plan_id: plan_id.as_deref().filter(|s| !s.is_empty()),
                resume_run: resume_run.as_deref().filter(|s| !s.is_empty()),
                spec_path: &spec,
                accept_seat_loss,
                now: chrono::Utc::now(),
                account: &account,
            };
            let desired = read_desired(&spec)?;
            let catalog = DbCatalog { db: &db, event_id: desired.event.clone(), tz: event_tz_of(&db, &desired) };
            // Human approval happened in the host UI (the tool-call approval); riv only checks the plan.
            let report = apply(api.as_ref(), &db, &plans, &req, &|_| true).await?;
            Ok::<_, RivError>(cap_reply(render_report(&report, &catalog)))
        })
        .await?
    }

    pub async fn verify(&self, spec: Option<String>) -> Result<String> {
        let (api, path, spec) = (self.api.clone(), self.db_path.clone(), self.spec(&spec));
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let desired = read_desired(&spec)?;
            let tz = event_tz_of(&db, &desired);
            let sched = api.get_schedule(&desired.event).await?;
            let rep = verify(&desired, &sched, &db.block_ids(&desired.event)?, tz);
            let cat = DbCatalog { db: &db, event_id: desired.event.clone(), tz };
            let mut out = if rep.all_ok() { "Your schedule matches the spec.\n".to_string() } else { String::new() };
            for d in rep.diffs() {
                use crate::plan::CatalogView;
                let name = cat.session(&d.target).map(|s| s.code().to_string()).unwrap_or_else(|| d.target.clone());
                out.push_str(&format!("{name}: expected {}, observed {}\n", d.expected, d.observed));
            }
            Ok::<_, RivError>(cap_reply(out))
        })
        .await?
    }

    pub async fn today(&self, date: Option<String>, fmt: Format) -> Result<String> {
        let date = match date.filter(|d| !d.is_empty()) {
            Some(d) => Some(
                chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .map_err(|_| RivError::validation("date expects YYYY-MM-DD"))?,
            ),
            None => None,
        };
        let (api, path, event) = (self.api.clone(), self.db_path.clone(), self.default_event.clone());
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            Ok::<_, RivError>(cap_reply(
                crate::today::today_text(api.as_ref(), &db, &event, date, fmt, crate::i18n::lang()).await?,
            ))
        })
        .await?
    }

    pub async fn prep_pack(&self, id: &str, lang: Option<String>) -> Result<String> {
        let (path, event, id) = (self.db_path.clone(), self.default_event.clone(), id.to_string());
        let l = if lang.as_deref() == Some("ja") { crate::i18n::Lang::Ja } else { crate::i18n::Lang::En };
        on_blocking(move || async move {
            let db = Db::open(&path)?;
            let pack = crate::prep::build_pack(&db, &event, &id, l)?;
            Ok::<_, RivError>(cap_reply(serde_json::to_string_pretty(&pack)?))
        })
        .await?
    }

    pub fn seat_loss_flag() -> &'static str {
        FLAG_SEAT_LOSS
    }
}
