//! Plan orchestration: read the spec, observe the schedule, build and save the plan. Writes nothing to the API.

use super::*;
use crate::api::EventsApi;
use crate::db::Db;
use crate::desired::{Desired, parse_markdown};

pub struct PlanOutcome {
    pub plan: Plan,
    pub rendered: String,
    pub path: PathBuf,
}

pub fn read_desired(spec_path: &Path) -> Result<Desired> {
    let md = std::fs::read_to_string(spec_path)
        .map_err(|e| RivError::validation(format!("cannot read spec {}: {e}", spec_path.display())))?;
    parse_markdown(&md)
}

pub fn event_tz_of(db: &Db, d: &Desired) -> chrono_tz::Tz {
    db.event(&d.event)
        .ok()
        .flatten()
        .and_then(|e| e.timezone)
        .and_then(|t| crate::timeutil::parse_tz(&t))
        .or_else(|| crate::timeutil::parse_tz(&d.timezone))
        .unwrap_or(chrono_tz::America::Los_Angeles)
}

pub async fn create_plan(
    api: &dyn EventsApi,
    db: &Db,
    spec_path: &Path,
    plans_dir: &Path,
    account: &str,
) -> Result<PlanOutcome> {
    let desired = read_desired(spec_path)?;
    let Some((catalog_version, _, _)) = db.last_sync(&desired.event)? else {
        return Err(RivError::general(format!(
            "no local catalog for `{}`: run `riv sync --event {}` first",
            desired.event, desired.event
        )));
    };
    let schedule = api.get_schedule(&desired.event).await?;
    let catalog = DbCatalog { db, event_id: desired.event.clone(), tz: event_tz_of(db, &desired) };
    let plan = make_plan(&PlanInput {
        desired: &desired,
        schedule: &schedule,
        managed: &db.managed_map(&desired.event)?,
        block_ids: &db.block_ids(&desired.event)?,
        catalog: &catalog,
        now: chrono::Utc::now(),
        plan_id: crate::ids::new_ulid(),
        catalog_version,
        account: account.to_string(),
    })?;
    let path = save_plan(&plan, plans_dir)?;
    let rendered = render_plan(&plan, &catalog);
    Ok(PlanOutcome { plan, rendered, path })
}
