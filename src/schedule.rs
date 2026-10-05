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
