//! Catalog persistence: events, sessions (+FTS), sync runs.

use super::Db;
use crate::api::{Event, Session};
use crate::error::Result;
use chrono::Datelike;
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Localized text captured when the server honored the requested locale.
#[derive(Debug, Clone, Default)]
pub struct L10n {
    pub locale: String,
    pub title: Option<String>,
    pub abstract_: Option<String>,
}

pub fn session_hash(s: &Session) -> String {
    let bytes = serde_json::to_vec(s).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn join(v: &[String]) -> String {
    v.join("; ")
}

fn weekday(date: &str) -> Option<&'static str> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some(["mon", "tue", "wed", "thu", "fri", "sat", "sun"][d.weekday().num_days_from_monday() as usize])
}

fn level_num(level: &Option<String>) -> Option<i64> {
    let digits: String = level.as_deref()?.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

impl Db {
    pub fn upsert_event(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events(event_id,name,timezone,authentication_required,raw) VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(event_id) DO UPDATE SET name=?2,timezone=?3,authentication_required=?4,raw=?5",
            params![e.event_id, e.name, e.timezone, e.authentication_required, serde_json::to_string(e)?],
        )?;
        Ok(())
    }

    pub fn event(&self, event_id: &str) -> Result<Option<Event>> {
        let raw: Option<String> =
            self.conn.query_row("SELECT raw FROM events WHERE event_id=?1", [event_id], |r| r.get(0)).optional()?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
    }

    /// Insert or replace one session and its FTS row. `tz` is the event zone for UTC columns.
    pub fn upsert_session(&self, event_id: &str, s: &Session, tz: chrono_tz::Tz, l10n: Option<&L10n>) -> Result<()> {
        let range = s.range_utc(tz);
        let st = s.session_time.as_ref();
        let date = st.and_then(|t| t.date.clone());
        let raw = serde_json::to_string(s)?;
        let speakers = s.speakers.iter().filter_map(|x| x.name.clone()).collect::<Vec<_>>().join("; ");
        let (topics, services) = (join(&s.topics), join(&s.services));
        let (tl, al, loc) = match l10n {
            Some(l) => (l.title.clone(), l.abstract_.clone(), Some(l.locale.clone())),
            None => {
                // Keep previously stored localized text when this pass carries none.
                let prev: Option<(Option<String>, Option<String>, Option<String>)> = self
                    .conn
                    .query_row(
                        "SELECT title_l10n, abstract_l10n, l10n_locale FROM sessions WHERE event_id=?1 AND session_id=?2",
                        params![event_id, s.session_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                prev.unwrap_or((None, None, None))
            }
        };
        self.conn.execute(
            "INSERT INTO sessions(event_id,session_id,code,title,abstract,type,level,level_num,venue,room,day_local,dow,
                start_utc,end_utc,reservable,seat,topics,services,speakers,title_l10n,abstract_l10n,l10n_locale,raw,raw_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)
             ON CONFLICT(event_id,session_id) DO UPDATE SET code=?3,title=?4,abstract=?5,type=?6,level=?7,level_num=?8,venue=?9,
                room=?10,day_local=?11,dow=?12,start_utc=?13,end_utc=?14,reservable=?15,seat=?16,topics=?17,services=?18,
                speakers=?19,title_l10n=?20,abstract_l10n=?21,l10n_locale=?22,raw=?23,raw_hash=?24",
            params![
                event_id, s.session_id, s.code(), s.title, s.abstract_, s.type_, s.level, level_num(&s.level), s.venue, s.room,
                date, date.as_deref().and_then(weekday), range.map(|r| r.0.to_rfc3339()), range.map(|r| r.1.to_rfc3339()),
                s.is_reservable, s.seat_availability.map(|a| serde_json::to_value(a).ok().and_then(|v| v.as_str().map(String::from))),
                topics, services, speakers, tl, al, loc, raw, session_hash(s)
            ],
        )?;
        self.conn.execute("DELETE FROM sessions_fts WHERE event_id=?1 AND session_id=?2", params![event_id, s.session_id])?;
        self.conn.execute(
            "INSERT INTO sessions_fts(event_id,session_id,code,title,abstract,speakers,topics,services,title_l10n,abstract_l10n)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![event_id, s.session_id, s.code(), s.title, s.abstract_, speakers, topics, services, tl, al],
        )?;
        Ok(())
    }

    /// Hashes of all stored sessions, to count changes after a sync.
    pub fn session_hashes(&self, event_id: &str) -> Result<HashMap<String, String>> {
        let mut st = self.conn.prepare("SELECT session_id, raw_hash FROM sessions WHERE event_id=?1")?;
        let rows = st.query_map([event_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The stored abstract (for preserving it across abstract-less passes).
    pub fn stored_abstract(&self, event_id: &str, session_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT abstract FROM sessions WHERE event_id=?1 AND session_id=?2", params![event_id, session_id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    pub fn delete_sessions_except(&self, event_id: &str, keep: &std::collections::HashSet<String>) -> Result<usize> {
        let existing: Vec<String> = {
            let mut st = self.conn.prepare("SELECT session_id FROM sessions WHERE event_id=?1")?;
            st.query_map([event_id], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?
        };
        let mut n = 0;
        for id in existing.iter().filter(|i| !keep.contains(*i)) {
            self.conn.execute("DELETE FROM sessions WHERE event_id=?1 AND session_id=?2", params![event_id, id])?;
            self.conn.execute("DELETE FROM sessions_fts WHERE event_id=?1 AND session_id=?2", params![event_id, id])?;
            n += 1;
        }
        Ok(n)
    }

    pub fn begin(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN")?;
        Ok(())
    }
    pub fn commit(&self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }
    pub fn rollback(&self) {
        let _ = self.conn.execute_batch("ROLLBACK");
    }

    pub fn start_sync_run(&self, event_id: &str, locale: Option<&str>, abstracts: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sync_runs(event_id,started_at,locale,abstracts) VALUES (?1,?2,?3,?4)",
            params![event_id, chrono::Utc::now().to_rfc3339(), locale, abstracts],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_sync_run(&self, id: i64, count: usize, changed: usize) -> Result<String> {
        let version = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.conn.execute(
            "UPDATE sync_runs SET finished_at=?2, session_count=?3, changed_count=?4, catalog_version=?5 WHERE id=?1",
            params![id, version, count as i64, changed as i64, version],
        )?;
        Ok(version)
    }

    /// Latest finished sync for an event: (catalog_version, session_count, locale).
    pub fn last_sync(&self, event_id: &str) -> Result<Option<(String, i64, Option<String>)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT catalog_version, session_count, locale FROM sync_runs WHERE event_id=?1 AND finished_at IS NOT NULL
                 ORDER BY id DESC LIMIT 1",
                [event_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    pub fn session_count(&self, event_id: &str) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM sessions WHERE event_id=?1", [event_id], |r| r.get(0))?)
    }

    /// Look up by sessionId first, then by short code (case-insensitive). Codes may repeat (-R1/-R2): all are returned.
    pub fn find_sessions(&self, event_id: &str, id_or_code: &str) -> Result<Vec<StoredSession>> {
        let mut st = self.conn.prepare(
            "SELECT raw, title_l10n, abstract_l10n, l10n_locale FROM sessions
             WHERE event_id=?1 AND (session_id=?2 OR code=?2 COLLATE NOCASE) ORDER BY start_utc, session_id",
        )?;
        let rows = st.query_map(params![event_id, id_or_code], stored_from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn all_sessions(&self, event_id: &str) -> Result<Vec<StoredSession>> {
        let mut st = self.conn.prepare(
            "SELECT raw, title_l10n, abstract_l10n, l10n_locale FROM sessions WHERE event_id=?1 ORDER BY start_utc, session_id",
        )?;
        let rows = st.query_map([event_id], stored_from_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// A stored session with its parsed raw JSON and optional localized text.
#[derive(Debug, Clone)]
pub struct StoredSession {
    pub session: Session,
    pub l10n: Option<L10n>,
}

pub(crate) fn stored_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSession> {
    let raw: String = r.get(0)?;
    let session: Session = serde_json::from_str(&raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let locale: Option<String> = r.get(3)?;
    let l10n = locale.map(|locale| L10n { locale, title: r.get(1).ok().flatten(), abstract_: r.get(2).ok().flatten() });
    Ok(StoredSession { session, l10n })
}
