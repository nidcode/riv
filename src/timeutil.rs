//! The single place for time-zone and wire-format conversions.

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

/// Wire format of personal time: UTC, no offset, no `Z`.
pub const WIRE_FMT: &str = "%Y-%m-%dT%H:%M:%S";

/// Zone assumed for an event whose time zone is not known yet (re:Invent is in Las Vegas).
pub const DEFAULT_EVENT_TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

pub fn parse_tz(name: &str) -> Option<Tz> {
    name.parse::<Tz>().ok()
}

/// Local (zone) naive date-time to UTC. Ambiguous times resolve to the earlier instant.
pub fn local_to_utc(local: NaiveDateTime, tz: Tz) -> Option<DateTime<Utc>> {
    tz.from_local_datetime(&local).earliest().map(|d| d.with_timezone(&Utc))
}

pub fn to_wire(utc: DateTime<Utc>) -> String {
    utc.format(WIRE_FMT).to_string()
}

pub fn from_wire(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, WIRE_FMT).ok().map(|n| n.and_utc())
}

/// Parse "YYYY-MM-DDTHH:MM" (or with seconds) as a local time.
pub fn parse_local(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").or_else(|_| NaiveDateTime::parse_from_str(s, WIRE_FMT)).ok()
}

/// Session time parts (`date` "2025-12-02", `time` "16:00", `length` "60" minutes) to a UTC range.
pub fn session_range(
    date: &str,
    time: &str,
    length_min: Option<&str>,
    tz: Tz,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let t = NaiveTime::parse_from_str(time, "%H:%M").ok()?;
    let start = local_to_utc(d.and_time(t), tz)?;
    let mins: i64 = length_min.and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    Some((start, start + chrono::Duration::minutes(mins)))
}

pub fn to_local(utc: DateTime<Utc>, tz: Tz) -> NaiveDateTime {
    utc.with_timezone(&tz).naive_local()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_winter_is_utc_minus_8() {
        let tz = parse_tz("America/Los_Angeles").unwrap();
        let (s, e) = session_range("2026-12-01", "19:00", Some("120"), tz).unwrap();
        assert_eq!(to_wire(s), "2026-12-02T03:00:00");
        assert_eq!(to_wire(e), "2026-12-02T05:00:00");
    }

    #[test]
    fn wire_roundtrip() {
        let d = from_wire("2026-12-02T03:00:00").unwrap();
        assert_eq!(to_wire(d), "2026-12-02T03:00:00");
        assert!(from_wire("2026-12-02T03:00:00Z").is_none());
    }
}
