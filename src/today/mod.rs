//! `riv today`: the day's schedule in venue-local time with walking hints. Pure over its inputs.

use crate::api::Schedule;
use crate::format::Format;
use crate::i18n::{Lang, t_in};
use crate::plan::CatalogView;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Walking minutes between venues. Values are user-edited estimates; missing pairs stay unknown.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Venues {
    #[serde(default)]
    pub same_venue_minutes: Option<u32>,
    #[serde(default)]
    pub walk_minutes: Vec<VenuePair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VenuePair {
    pub from: String,
    pub to: String,
    pub minutes: u32,
}

impl Venues {
    pub fn load(path: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Minutes between two venues, or None when not configured (never guessed).
    pub fn walk(&self, from: &str, to: &str) -> Option<u32> {
        let norm = |s: &str| s.trim().to_lowercase();
        let (f, t) = (norm(from), norm(to));
        if f == t {
            return self.same_venue_minutes;
        }
        self.walk_minutes
            .iter()
            .find(|p| (norm(&p.from) == f && norm(&p.to) == t) || (norm(&p.from) == t && norm(&p.to) == f))
            .map(|p| p.minutes)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TodayItem {
    pub start: String,
    pub end: String,
    pub kind: &'static str,
    pub code: String,
    pub title: String,
    pub venue: Option<String>,
    pub room: Option<String>,
    pub walk_minutes: Option<u32>,
    pub leave_by: Option<String>,
    pub prep_note: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TodayView {
    pub date: NaiveDate,
    pub items: Vec<TodayItem>,
}

pub struct TodayInput<'a> {
    pub date: NaiveDate,
    pub schedule: &'a Schedule,
    pub catalog: &'a dyn CatalogView,
    pub venues: &'a Venues,
    /// sessionIds that have an attached prep note.
    pub prep_notes: &'a HashSet<String>,
}

struct Raw {
    start: DateTime<Utc>,
    item: TodayItem,
}

pub fn build_today(i: &TodayInput<'_>) -> TodayView {
    let tz = i.catalog.tz();
    let day = |d: DateTime<Utc>| crate::timeutil::to_local(d, tz).date();
    let hm = |d: DateTime<Utc>| crate::timeutil::to_local(d, tz).format("%H:%M").to_string();
    let mut raws: Vec<Raw> = Vec::new();
    for id in &i.schedule.reserved {
        let Some(s) = i.catalog.session(id) else { continue };
        let Some((st, en)) = s.range_utc(tz) else { continue };
        if day(st) != i.date {
            continue;
        }
        raws.push(Raw {
            start: st,
            item: TodayItem {
                start: hm(st),
                end: hm(en),
                kind: "session",
                code: s.code().to_string(),
                title: s.title.clone(),
                venue: s.venue.clone(),
                room: s.room.clone(),
                walk_minutes: None,
                leave_by: None,
                prep_note: i.prep_notes.contains(id),
            },
        });
    }
    for p in &i.schedule.personal_time {
        let (Some(st), Some(en)) =
            (crate::timeutil::from_wire(&p.start_date_time), crate::timeutil::from_wire(&p.end_date_time))
        else {
            continue;
        };
        if day(st) != i.date {
            continue;
        }
        raws.push(Raw {
            start: st,
            item: TodayItem {
                start: hm(st),
                end: hm(en),
                kind: "personal",
                code: String::new(),
                title: p.title.clone(),
                venue: p.location.clone(),
                room: None,
                walk_minutes: None,
                leave_by: None,
                prep_note: false,
            },
        });
    }
    raws.sort_by_key(|r| r.start);
    // Walking hint: from the previous item's venue (unknown for the first item of the day).
    let mut prev_venue: Option<String> = None;
    for r in &mut raws {
        if let (Some(from), Some(to)) = (&prev_venue, &r.item.venue) {
            r.item.walk_minutes = i.venues.walk(from, to);
        }
        if let Some(m) = r.item.walk_minutes {
            let leave = crate::timeutil::to_local(r.start - Duration::minutes(i64::from(m)), tz).time();
            r.item.leave_by = Some(leave.format("%H:%M").to_string());
        }
        if r.item.venue.is_some() {
            prev_venue = r.item.venue.clone();
        }
    }
    TodayView { date: i.date, items: raws.into_iter().map(|r| r.item).collect() }
}

fn walk_text(lang: Lang, it: &TodayItem) -> String {
    match (it.walk_minutes, &it.leave_by) {
        (Some(m), Some(by)) => {
            t_in(lang, "walk {n} min · leave by {time}").replace("{n}", &m.to_string()).replace("{time}", by)
        }
        _ => t_in(lang, "walk: unknown"),
    }
}

pub fn render_today(v: &TodayView, format: Format, lang: Lang) -> String {
    let header = v.date.format("%a %m/%d").to_string();
    if v.items.is_empty() {
        return format!("{header}\n{}\n", t_in(lang, "Nothing scheduled on this day."));
    }
    let prep = t_in(lang, "prep note available");
    match format {
        Format::Phone => {
            // At most 12 lines including the header; no tables.
            let mut out = format!("{header}\n");
            let room = 10;
            for it in v.items.iter().take(if v.items.len() > 11 { room } else { 11 }) {
                let name = if it.code.is_empty() { it.title.clone() } else { format!("{} {}", it.code, it.title) };
                let venue = it.venue.clone().map(|x| format!(" @{x}")).unwrap_or_default();
                let note = if it.prep_note { format!(" [{prep}]") } else { String::new() };
                out.push_str(&format!("{} {name}{venue} ({}){note}\n", it.start, walk_text(lang, it)));
            }
            if v.items.len() > 11 {
                out.push_str(&t_in(lang, "+{n} more").replace("{n}", &(v.items.len() - room).to_string()));
                out.push('\n');
            }
            out
        }
        Format::Ide => {
            let mut out = format!(
                "### {header}\n\n| {} | {} | {} | {} |\n|---|---|---|---|\n",
                t_in(lang, "Time"),
                t_in(lang, "Session"),
                t_in(lang, "Where"),
                t_in(lang, "Walk")
            );
            for it in &v.items {
                let name = if it.code.is_empty() { it.title.clone() } else { format!("{} {}", it.code, it.title) };
                let place = [it.venue.clone(), it.room.clone()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                let note = if it.prep_note { format!(" ({prep})") } else { String::new() };
                out.push_str(&format!(
                    "| {}-{} | {}{} | {} | {} |\n",
                    it.start,
                    it.end,
                    name.replace('|', "\\|"),
                    note,
                    place,
                    walk_text(lang, it)
                ));
            }
            out
        }
    }
}

/// Parse `HH:MM`-style leave-by for tests and callers.
pub fn parse_hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// Fetch the schedule and render the day. `date` defaults to today in the event's time zone.
pub async fn today_text(
    api: &dyn crate::api::EventsApi,
    db: &crate::db::Db,
    event_id: &str,
    date: Option<NaiveDate>,
    format: Format,
    lang: Lang,
) -> crate::error::Result<String> {
    let tz = db.event_tz(event_id);
    let date = date.unwrap_or_else(|| crate::timeutil::to_local(Utc::now(), tz).date());
    let schedule = api.get_schedule(event_id).await?;
    let venues = Venues::load(&crate::paths::venues_path()).unwrap_or_default();
    let catalog = crate::plan::DbCatalog { db, event_id: event_id.to_string(), tz };
    let prep = db.prep_notes(event_id)?;
    let view =
        build_today(&TodayInput { date, schedule: &schedule, catalog: &catalog, venues: &venues, prep_notes: &prep });
    Ok(render_today(&view, format, lang))
}
