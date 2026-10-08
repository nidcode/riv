//! Full-catalog sync: walk ListSessions until `nextToken` disappears, then store locally.

use crate::api::{ApiError, EventsApi, ListSessionsParams, Session};
use crate::db::Db;
use crate::db::catalog::L10n;
use crate::error::{Result, RivError};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbstractsMode {
    /// No-abstract pass first (fast, searchable early), then a pass with abstracts.
    Auto,
    Always,
    Never,
}

impl std::str::FromStr for AbstractsMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => Err(format!("unknown abstracts mode `{other}` (auto|always|never)")),
        }
    }
}

impl AbstractsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub event_id: String,
    pub locale: Option<String>,
    pub abstracts: AbstractsMode,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub sessions: usize,
    pub changed: usize,
    pub removed: usize,
    pub catalog_version: String,
    pub locale_stored: Option<String>,
    pub warnings: Vec<String>,
}

const MAX_PAGES: usize = 10_000;

/// Walk every page. Pages may be short without being last: only a missing `nextToken` ends the walk.
async fn walk(
    api: &dyn EventsApi,
    event_id: &str,
    locale: Option<&str>,
    include_abstracts: bool,
    phase: &'static str,
    progress: &dyn Fn(Progress),
    mut on_page: impl FnMut(&[Session]) -> Result<()>,
) -> Result<(usize, Option<String>)> {
    let mut token: Option<String> = None;
    let mut seen = 0;
    let mut language = None;
    for page_no in 0..MAX_PAGES {
        let p = ListSessionsParams { locale: locale.map(String::from), include_abstracts, next_token: token.clone() };
        let page = api.list_sessions_page(event_id, &p).await.map_err(|e| match e {
            ApiError::Closed => RivError::general("ListSessions is closed (409)"),
            other => RivError::from(other),
        })?;
        if page_no == 0 {
            language = page.content_language.clone();
        }
        seen += page.items.len();
        on_page(&page.items)?;
        progress(Progress { phase, done: seen, total: page.total_count as usize });
        match page.next_token {
            None => return Ok((seen, language)),
            Some(t) if Some(&t) == token.as_ref() => {
                return Err(RivError::general("nextToken did not advance; aborting sync"));
            }
            Some(t) => token = Some(t),
        }
    }
    Err(RivError::general("too many pages; aborting sync"))
}

fn same_language(requested: &str, got: &str) -> bool {
    requested.eq_ignore_ascii_case(got.trim())
}

pub async fn sync(api: &dyn EventsApi, db: &Db, opts: &SyncOptions, progress: &dyn Fn(Progress)) -> Result<SyncReport> {
    let event = api.get_event(&opts.event_id).await?;
    db.upsert_event(&event)?;
    let tz = event.timezone.as_deref().and_then(crate::timeutil::parse_tz).unwrap_or(chrono_tz::UTC);
    let before = db.session_hashes(&opts.event_id)?;
    let run = db.start_sync_run(&opts.event_id, opts.locale.as_deref(), opts.abstracts.as_str())?;
    let mut report = SyncReport::default();

    let passes: &[bool] = match opts.abstracts {
        AbstractsMode::Auto => &[false, true],
        AbstractsMode::Always => &[true],
        AbstractsMode::Never => &[false],
    };
    // Final state per session, for change counting and stale-row removal.
    let mut fin: HashMap<String, Session> = HashMap::new();
    let eid = opts.event_id.as_str();
    db.begin()?;
    let result: Result<()> = async {
        for &with_abstracts in passes {
            let phase = if with_abstracts { "abstracts" } else { "catalog" };
            walk(api, eid, None, with_abstracts, phase, progress, |items| {
                for s in items {
                    let mut s = s.clone();
                    if !with_abstracts && s.abstract_.is_none() {
                        // Never erase a stored abstract just because this pass omitted it.
                        s.abstract_ = fin
                            .get(&s.session_id)
                            .and_then(|p| p.abstract_.clone())
                            .or(db.stored_abstract(eid, &s.session_id)?);
                    }
                    db.upsert_session(eid, &s, tz, None)?;
                    fin.insert(s.session_id.clone(), s);
                }
                Ok(())
            })
            .await?;
        }
        // Localized pass: only stored if the server really answered in that language.
        if let Some(loc) = &opts.locale {
            let with_abstracts = !matches!(opts.abstracts, AbstractsMode::Never);
            let mut pending: Vec<(String, L10n)> = Vec::new();
            let (_, lang) = walk(api, eid, Some(loc), with_abstracts, "locale", progress, |items| {
                for s in items {
                    pending.push((
                        s.session_id.clone(),
                        L10n { locale: loc.clone(), title: Some(s.title.clone()), abstract_: s.abstract_.clone() },
                    ));
                }
                Ok(())
            })
            .await?;
            match lang {
                Some(l) if same_language(loc, &l) => {
                    for (id, l10n) in pending {
                        if let Some(s) = fin.get(&id) {
                            db.upsert_session(eid, s, tz, Some(&l10n))?;
                        }
                    }
                    report.locale_stored = Some(loc.clone());
                }
                other => report.warnings.push(format!(
                    "requested locale {loc} but the server answered {}; keeping en-US only",
                    other.as_deref().unwrap_or("(no Content-Language)")
                )),
            }
        }
        // An empty answer for an event that had sessions is not "the catalog is empty": refuse to wipe the local copy.
        if fin.is_empty() && !before.is_empty() {
            return Err(RivError::general(format!(
                "the API returned 0 sessions for `{eid}` but {} are stored locally; the local catalog was kept. \
                 This is usually temporary (service or sign-in/registration problem): check `riv doctor` and try again later",
                before.len()
            )));
        }
        let keep: HashSet<String> = fin.keys().cloned().collect();
        report.removed = db.delete_sessions_except(eid, &keep)?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => db.commit()?,
        Err(e) => {
            db.rollback();
            return Err(e);
        }
    }
    report.sessions = fin.len();
    report.changed = fin
        .values()
        .filter(|s| before.get(&s.session_id).is_none_or(|h| *h != crate::db::catalog::session_hash(s)))
        .count();
    report.catalog_version = db.finish_sync_run(run, fin.len(), report.changed)?;
    Ok(report)
}
