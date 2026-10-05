use super::ctx::*;
use crate::error::{Result, RivError};
use crate::format::Format;
use crate::i18n::lang;
use chrono::NaiveDate;

pub async fn today(event: &str, date: Option<String>, format: &str) -> Result<i32> {
    let date = match date {
        Some(d) => Some(
            NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|_| RivError::validation("--date expects YYYY-MM-DD"))?,
        ),
        None => None,
    };
    let format: Format = format.parse().map_err(RivError::validation)?;
    let db = open_db()?;
    print!("{}", crate::today::today_text(&make_api(), &db, event, date, format, lang()).await?);
    Ok(0)
}

pub fn prep(
    event: &str,
    id: Option<String>,
    lang_arg: Option<String>,
    json: bool,
    attach: Option<Vec<String>>,
) -> Result<i32> {
    let db = open_db()?;
    if let Some(a) = attach {
        let [sid, path] = a.as_slice() else { return Err(RivError::validation("--attach expects <id> <path>")) };
        if !std::path::Path::new(path).is_file() {
            return Err(RivError::validation(format!("note file not found: {path}")));
        }
        let found = db.find_sessions(event, sid)?;
        let Some(f) = found.first().filter(|_| found.len() == 1) else {
            return Err(RivError::validation(format!("`{sid}` does not match exactly one session; use a sessionId")));
        };
        db.attach_prep(event, &f.session.session_id, path)?;
        println!("attached {path} to {}", f.session.code());
        return Ok(0);
    }
    let id = id.ok_or_else(|| RivError::validation("usage: riv prep <sessionId|code> [--lang ja|en] [--json]"))?;
    let l = match lang_arg.as_deref() {
        Some("ja") => crate::i18n::Lang::Ja,
        Some("en") => crate::i18n::Lang::En,
        Some(o) => return Err(RivError::validation(format!("unknown --lang `{o}` (ja|en)"))),
        None => lang(),
    };
    let pack = crate::prep::build_pack(&db, event, &id, l)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pack)?);
    } else {
        println!("{}  {}", pack.session.code, pack.session.title);
        println!("{}", pack.session.start);
        println!("\nqueries:");
        pack.queries.iter().for_each(|q| println!("  {q}"));
        println!("\nrelated:");
        pack.related.iter().for_each(|r| println!("  {}  {}  {}", r.code, r.title, r.start));
        println!("\nsave the note to: {}", pack.save_path);
        println!("then: riv prep --attach {} {}", pack.session.id, pack.save_path);
    }
    Ok(0)
}
