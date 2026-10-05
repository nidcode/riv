//! Schedule helpers shared by search, plan, and today: busy ranges from a `Schedule`.

use crate::api::{EventsApi, Schedule};
use crate::db::Db;
use crate::error::Result;
use chrono::{DateTime, Utc};

/// UTC ranges occupied by reserved sessions (from the local catalog) and personal time.
pub fn busy_from(
    db: &Db,
    event_id: &str,
    tz: chrono_tz::Tz,
    sched: &Schedule,
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    let mut out = Vec::new();
    for id in &sched.reserved {
        if let Some(s) = db.find_sessions(event_id, id)?.first()
            && let Some(r) = s.session.range_utc(tz)
        {
            out.push(r);
        }
    }
    for p in &sched.personal_time {
        if let (Some(a), Some(b)) =
            (crate::timeutil::from_wire(&p.start_date_time), crate::timeutil::from_wire(&p.end_date_time))
        {
            out.push((a, b));
        }
    }
    Ok(out)
}

pub async fn busy_ranges(
    api: &dyn EventsApi,
    db: &Db,
    event_id: &str,
    tz: chrono_tz::Tz,
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    let sched = api.get_schedule(event_id).await?;
    busy_from(db, event_id, tz, &sched)
}

/// Local "Tue 12/01 10:00-11:00" for a UTC range.
pub fn range_label(r: (DateTime<Utc>, DateTime<Utc>), tz: chrono_tz::Tz) -> String {
    let (s, e) = (crate::timeutil::to_local(r.0, tz), crate::timeutil::to_local(r.1, tz));
    format!("{}-{}", s.format("%a %m/%d %H:%M"), e.format("%H:%M"))
}

/// Compact schedule summary: reserved, favorites, personal time, each ordered by start.
pub fn render_schedule(db: &Db, event_id: &str, tz: chrono_tz::Tz, sched: &Schedule) -> Result<String> {
    use crate::i18n::t;
    let mut out = String::new();
    let mut section = |title: String, mut rows: Vec<(Option<DateTime<Utc>>, String)>| {
        out.push_str(&format!("{title} ({})\n", rows.len()));
        rows.sort_by_key(|r| r.0);
        for (_, line) in rows {
            out.push_str(&format!("  {line}\n"));
        }
    };
    let row_for = |id: &String| -> Result<(Option<DateTime<Utc>>, String)> {
        Ok(match db.find_sessions(event_id, id)?.into_iter().find(|s| &s.session.session_id == id) {
            Some(s) => {
                let r = s.session.range_utc(tz);
                let when = r.map(|r| range_label(r, tz)).unwrap_or_default();
                (
                    r.map(|r| r.0),
                    format!(
                        "{}  {}  {}  {}",
                        s.session.code(),
                        s.session.title,
                        when,
                        s.session.venue.clone().unwrap_or_default()
                    )
                    .trim_end()
                    .to_string(),
                )
            }
            None => (None, format!("{id}  (not in the local catalog; run `riv sync`)")),
        })
    };
    section(t("Reserved"), sched.reserved.iter().map(row_for).collect::<Result<_>>()?);
    section(t("Favorites"), sched.favorites.iter().map(row_for).collect::<Result<_>>()?);
    let pt = sched
        .personal_time
        .iter()
        .map(|p| {
            let r = crate::timeutil::from_wire(&p.start_date_time).zip(crate::timeutil::from_wire(&p.end_date_time));
            (r.map(|r| r.0), format!("{}  {}", p.title, r.map(|r| range_label(r, tz)).unwrap_or_default()))
        })
        .collect();
    section(t("Personal time"), pt);
    Ok(out)
}
