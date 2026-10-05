//! Prep pack: everything an agent needs to write a prep note, built from the local catalog only.

use crate::db::Db;
use crate::db::catalog::StoredSession;
use crate::error::{Result, RivError};
use crate::format::start_local;
use crate::i18n::{Lang, t_in};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSession {
    pub id: String,
    pub code: String,
    pub title: String,
    pub abstract_: Option<String>,
    pub level: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub topics: Vec<String>,
    pub services: Vec<String>,
    pub speakers: Vec<String>,
    pub start: String,
    pub venue: Option<String>,
    pub room: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Related {
    pub id: String,
    pub code: String,
    pub title: String,
    pub start: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepPack {
    pub session: PackSession,
    pub related: Vec<Related>,
    /// Local hints for searching earlier years. The catalog only holds the current year, so these are
    /// query material, not facts; matching codes across years is a heuristic.
    pub prior_year_candidates: Vec<String>,
    pub queries: Vec<String>,
    pub note_template: String,
    pub save_path: String,
}

const STOPWORDS: &[&str] =
    &["a", "an", "the", "and", "or", "for", "with", "to", "of", "in", "on", "your", "how", "from", "using", "at", "by"];

fn keywords(title: &str, max: usize) -> String {
    title
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| !w.is_empty() && !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .take(max)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `AIM301-R2` -> `AIM301`.
fn code_base(code: &str) -> String {
    match code.rfind("-R") {
        Some(i) if code[i + 2..].chars().all(|c| c.is_ascii_digit()) && i + 2 < code.len() => code[..i].to_string(),
        _ => code.to_string(),
    }
}

fn safe_file_stem(code: &str) -> String {
    code.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub fn note_template(lang: Lang, code: &str, title: &str) -> String {
    let h = |k: &str| t_in(lang, k);
    format!(
        "# {code} — {title}\n**{}**: \n**{}**: \n**{}**: \n**{}**: \n**{}**: \n**{}**: \n",
        h("In 3 lines"),
        h("Last year's version"),
        h("What changed"),
        h("Terms to know"),
        h("Questions to ask"),
        h("Related sessions"),
    )
}

pub fn build_pack(db: &Db, event_id: &str, id_or_code: &str, lang: Lang) -> Result<PrepPack> {
    let tz = crate::cli::ctx::event_tz(db, event_id);
    let found = db.find_sessions(event_id, id_or_code)?;
    let base: StoredSession = match found.len() {
        0 => return Err(RivError::general(format!("no session `{id_or_code}` in the local catalog (run `riv sync`)"))),
        1 => found.into_iter().next().ok_or_else(|| RivError::general("no session"))?,
        n => {
            let ids: Vec<String> = found.iter().map(|f| f.session.session_id.clone()).collect();
            return Err(RivError::validation(format!(
                "`{id_or_code}` matches {n} sessions; use a sessionId: {}",
                ids.join(", ")
            )));
        }
    };
    let s = &base.session;
    let code = s.code().to_string();
    let year =
        db.event(event_id)?.and_then(|e| e.start_date.get(..4).and_then(|y| y.parse::<i32>().ok())).unwrap_or(2026);
    let prev = year - 1;

    let related = db
        .related(event_id, &base, 5)?
        .into_iter()
        .map(|r| Related {
            id: r.session.session_id.clone(),
            code: r.session.code().to_string(),
            title: r.session.title.clone(),
            start: start_local(&r.session, tz),
        })
        .collect();

    // Hints from the local catalog: other runs of the same session, and similarly titled sessions.
    let mut hints = Vec::new();
    let cb = code_base(&code);
    for r in db.sessions_by_code_base(event_id, &cb)?.into_iter().chain(db.sessions_by_title(event_id, &s.title)?) {
        let c = r.session.code().to_string();
        if r.session.session_id != s.session_id && !hints.iter().any(|h: &String| h.starts_with(&format!("{c} "))) {
            hints.push(format!("{c} (another run of the same session: {})", r.session.title));
        }
    }
    let kw = keywords(&s.title, 6);
    let similar =
        db.search(event_id, &crate::search::SearchQuery { text: kw.clone(), limit: 4, ..Default::default() }, &[])?;
    for r in similar.into_iter().filter(|r| r.session.session_id != s.session_id && r.session.title != s.title).take(3)
    {
        hints.push(format!("{} (similar title: {})", r.session.code(), r.session.title));
    }
    hints.push(format!("code base `{cb}`: session codes change between years, so use it only as a search term"));

    let speakers: Vec<String> = s.speakers.iter().filter_map(|x| x.name.clone()).collect();
    let mut queries = vec![format!("{code} re:Invent {prev}"), format!("{kw} re:Invent {prev} youtube")];
    queries.extend(speakers.iter().take(2).map(|sp| format!("{sp} re:Invent")));
    queries.extend(s.services.iter().take(2).map(|sv| format!("{sv} what's new {year}")));
    if lang == Lang::Ja {
        queries.push(format!("{code} re:Invent {prev} レポート"));
        queries.push(format!("{kw} DevelopersIO"));
        queries.push(format!("{kw} Qiita re:Invent"));
    }

    Ok(PrepPack {
        session: PackSession {
            id: s.session_id.clone(),
            code: code.clone(),
            title: s.title.clone(),
            abstract_: s.abstract_.clone(),
            level: s.level.clone(),
            type_: s.type_.clone(),
            topics: s.topics.clone(),
            services: s.services.clone(),
            speakers,
            start: start_local(s, tz),
            venue: s.venue.clone(),
            room: s.room.clone(),
        },
        related,
        prior_year_candidates: hints,
        queries,
        note_template: note_template(lang, &code, &s.title),
        save_path: format!(".kiro/specs/reinvent-2026/prep/{}.md", safe_file_stem(&code)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_base_strips_run_suffix() {
        assert_eq!(code_base("AIM301-R2"), "AIM301");
        assert_eq!(code_base("AIM301"), "AIM301");
        assert_eq!(code_base("AIM301-R"), "AIM301-R");
    }

    #[test]
    fn keywords_drop_stopwords() {
        assert_eq!(keywords("Building agentic workflows with Amazon Bedrock", 4), "Building agentic workflows Amazon");
    }

    #[test]
    fn save_path_is_filesystem_safe() {
        assert_eq!(safe_file_stem("A/B ../c"), "A_B____c");
    }
}
