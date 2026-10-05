//! Output formatting shared by CLI and MCP (`ide` vs `phone`) and one-line session rendering.

use crate::api::Session;
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Ide,
    Phone,
}

impl std::str::FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "ide" => Ok(Format::Ide),
            "phone" => Ok(Format::Phone),
            o => Err(format!("unknown format `{o}` (ide|phone)")),
        }
    }
}

pub const DEFAULT_FIELDS: &[&str] = &["id", "code", "title", "start", "venue"];

/// Local start time like `Tue 12/01 10:00`.
pub fn start_local(s: &Session, tz: Tz) -> String {
    s.range_utc(tz)
        .map(|(a, _)| crate::timeutil::to_local(a, tz).format("%a %m/%d %H:%M").to_string())
        .unwrap_or_default()
}

pub fn field(s: &Session, name: &str, tz: Tz) -> String {
    match name {
        "id" => s.session_id.clone(),
        "code" => s.code().to_string(),
        "title" => s.title.clone(),
        "start" => start_local(s, tz),
        "venue" => s.venue.clone().unwrap_or_default(),
        "room" => s.room.clone().unwrap_or_default(),
        "level" => s.level.clone().unwrap_or_default(),
        "type" => s.type_.clone().unwrap_or_default(),
        "seat" => s
            .seat_availability
            .and_then(|a| serde_json::to_value(a).ok())
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// One line per session, fields separated by two spaces.
pub fn line(s: &Session, fields: &[&str], tz: Tz) -> String {
    fields.iter().map(|f| field(s, f, tz)).collect::<Vec<_>>().join("  ")
}

pub fn valid_field(f: &str) -> bool {
    ["id", "code", "title", "start", "venue", "room", "level", "type", "seat"].contains(&f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SessionTime;

    #[test]
    fn renders_local_time_line() {
        let s = Session {
            session_id: "x1".into(),
            title: "T".into(),
            abbreviation: Some("AIM301".into()),
            session_time: Some(SessionTime {
                date: Some("2026-12-01".into()),
                time: Some("10:00".into()),
                length: Some("60".into()),
                timezone: None,
            }),
            ..Default::default()
        };
        let tz = chrono_tz::America::Los_Angeles;
        assert_eq!(line(&s, &["code", "title", "start"], tz), "AIM301  T  Tue 12/01 10:00");
    }
}
