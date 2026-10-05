//! Local search: FTS5 ranking plus structured filters. Nothing here touches the network.

use crate::db::Db;
use crate::db::catalog::{StoredSession, stored_from_row};
use crate::error::Result;
use chrono::{DateTime, NaiveTime, Utc};
use rusqlite::types::Value;

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub level: Option<u32>,
    /// Weekday ("tue") or an ISO date.
    pub day: Option<String>,
    pub topic: Option<String>,
    pub service: Option<String>,
    pub venue: Option<String>,
    /// Local time window the session must fit in and not collide with `busy`.
    pub free_between: Option<(NaiveTime, NaiveTime)>,
    pub limit: usize,
}

/// Parse "13:00-15:00".
pub fn parse_window(s: &str) -> Option<(NaiveTime, NaiveTime)> {
    let (a, b) = s.split_once('-')?;
    let p = |x: &str| NaiveTime::parse_from_str(x.trim(), "%H:%M").ok();
    Some((p(a)?, p(b)?))
}

/// Split free text into FTS terms (letters/digits, any script).
fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(|t| t.to_lowercase()).collect()
}

fn fts_expr(terms: &[String], op: &str) -> String {
    terms.iter().map(|t| format!("\"{}\"*", t.replace('"', ""))).collect::<Vec<_>>().join(&format!(" {op} "))
}

// bm25 column weights, in sessions_fts column order after the two UNINDEXED ones:
// code, title, abstract, speakers, topics, services, title_l10n, abstract_l10n
const BM25: &str = "bm25(sessions_fts, 0, 0, 8.0, 5.0, 1.0, 2.0, 2.0, 3.0, 5.0, 1.0)";

fn filters(q: &SearchQuery, params: &mut Vec<Value>) -> String {
    let mut w = String::new();
    let mut add = |sql: &str, v: Value, params: &mut Vec<Value>| {
        w.push_str(" AND ");
        w.push_str(sql);
        params.push(v);
    };
    if let Some(l) = q.level {
        add("s.level_num = ?", Value::Integer(l as i64), params);
    }
    if let Some(d) = &q.day {
        if d.len() == 10 {
            add("s.day_local = ?", Value::Text(d.clone()), params);
        } else {
            add("s.dow = ?", Value::Text(d.to_lowercase().chars().take(3).collect()), params);
        }
    }
    if let Some(t) = &q.topic {
        add("s.topics LIKE ? COLLATE NOCASE", Value::Text(format!("%{t}%")), params);
    }
    if let Some(t) = &q.service {
        add("s.services LIKE ? COLLATE NOCASE", Value::Text(format!("%{t}%")), params);
    }
    if let Some(t) = &q.venue {
        add("s.venue LIKE ? COLLATE NOCASE", Value::Text(format!("%{t}%")), params);
    }
    w
}

impl Db {
    /// Ranked search. Tries AND of all terms first, then tops up with OR so natural-language
    /// questions still return something. `busy` are UTC ranges (reserved sessions, personal time).
    pub fn search(
        &self,
        event_id: &str,
        q: &SearchQuery,
        busy: &[(DateTime<Utc>, DateTime<Utc>)],
    ) -> Result<Vec<StoredSession>> {
        let tz = self
            .event(event_id)?
            .and_then(|e| e.timezone)
            .and_then(|t| crate::timeutil::parse_tz(&t))
            .unwrap_or(chrono_tz::UTC);
        let limit = if q.limit == 0 { 10 } else { q.limit };
        let ts = terms(&q.text);
        let mut out: Vec<StoredSession> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let modes: Vec<Option<&str>> = if ts.is_empty() {
            vec![None]
        } else if ts.len() == 1 {
            vec![Some("AND")]
        } else {
            vec![Some("AND"), Some("OR")]
        };
        for mode in modes {
            if out.len() >= limit {
                break;
            }
            let mut params: Vec<Value> = vec![Value::Text(event_id.to_string())];
            let (from, order) = match mode {
                Some(op) => {
                    params.insert(0, Value::Text(fts_expr(&ts, op)));
                    (
                        "sessions_fts f JOIN sessions s ON s.event_id = f.event_id AND s.session_id = f.session_id"
                            .to_string(),
                        BM25.to_string(),
                    )
                }
                None => ("sessions s".into(), "s.start_utc, s.session_id".into()),
            };
            let mut sql = format!("SELECT s.raw, s.title_l10n, s.abstract_l10n, s.l10n_locale FROM {from} WHERE ");
            if mode.is_some() {
                sql.push_str("sessions_fts MATCH ? AND s.event_id = ?");
            } else {
                sql.push_str("s.event_id = ?");
            }
            // Param order: MATCH expr (if any) first, then event id: already arranged above.
            sql.push_str(&filters(q, &mut params));
            sql.push_str(&format!(" ORDER BY {order}"));
            let mut st = self.conn.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(params.iter()), stored_from_row)?;
            for r in rows {
                let r = r?;
                if !seen.insert(r.session.session_id.clone()) {
                    continue;
                }
                if let Some(w) = q.free_between
                    && !fits(&r, w, busy, tz)
                {
                    continue;
                }
                out.push(r);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Candidates related to a session: same day, shared topic. Used by `prep`.
    pub fn related(&self, event_id: &str, base: &StoredSession, max: usize) -> Result<Vec<StoredSession>> {
        let s = &base.session;
        let day = s.session_time.as_ref().and_then(|t| t.date.clone());
        let mut found = Vec::new();
        for topic in &s.topics {
            let q = SearchQuery {
                text: String::new(),
                day: day.clone(),
                topic: Some(topic.clone()),
                limit: max + 1,
                ..Default::default()
            };
            for r in self.search(event_id, &q, &[])? {
                if r.session.session_id != s.session_id
                    && !found.iter().any(|f: &StoredSession| f.session.session_id == r.session.session_id)
                {
                    found.push(r);
                }
            }
        }
        found.truncate(max);
        Ok(found)
    }
}

fn fits(
    r: &StoredSession,
    (from, to): (NaiveTime, NaiveTime),
    busy: &[(DateTime<Utc>, DateTime<Utc>)],
    tz: chrono_tz::Tz,
) -> bool {
    let Some((s, e)) = r.session.range_utc(tz) else { return false };
    let (ls, le) = (crate::timeutil::to_local(s, tz).time(), crate::timeutil::to_local(e, tz).time());
    ls >= from && le <= to && !busy.iter().any(|(bs, be)| s < *be && *bs < e)
}
