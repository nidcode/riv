use super::ctx::*;
use super::plan_cmds::DEFAULT_SPEC;
use crate::api::EventsApi;
use crate::apply::{ApplyRequest, VerifyReport, apply, render_report, verify};
use crate::error::{Result, RivError};
use crate::plan::service::{event_tz_of, read_desired};
use crate::plan::{DbCatalog, Plan, render_plan};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

pub struct ApplyArgs {
    pub plan: Option<String>,
    pub resume: Option<String>,
    pub spec: Option<PathBuf>,
    pub accept_seat_loss: bool,
    pub yes: bool,
    pub json: bool,
}

pub async fn run_apply(a: ApplyArgs) -> Result<i32> {
    let spec = a.spec.unwrap_or_else(|| PathBuf::from(DEFAULT_SPEC));
    let db = open_db()?;
    let api = make_api();
    let account = crate::auth::account_id();
    let req = ApplyRequest {
        plan_id: a.plan.as_deref(),
        resume_run: a.resume.as_deref(),
        spec_path: &spec,
        accept_seat_loss: a.accept_seat_loss,
        now: chrono::Utc::now(),
        account: &account,
    };
    let interactive = std::io::stdin().is_terminal() && !a.yes;
    let desired = read_desired(&spec)?;
    let catalog = DbCatalog { db: &db, event_id: desired.event.clone(), tz: event_tz_of(&db, &desired) };
    let confirm = |plan: &Plan| -> bool {
        if !interactive {
            return true;
        }
        print!("{}", render_plan(plan, &catalog));
        print!("Apply this plan? [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
    };
    let report = apply(&api, &db, &crate::paths::plans_dir(), &req, &confirm).await?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_report(&report, &catalog));
    }
    Ok(report.exit_code())
}

pub async fn run_verify(spec: Option<PathBuf>, json: bool) -> Result<i32> {
    let spec = spec.unwrap_or_else(|| PathBuf::from(DEFAULT_SPEC));
    let db = open_db()?;
    let desired = read_desired(&spec)?;
    let tz = event_tz_of(&db, &desired);
    let sched = make_api().get_schedule(&desired.event).await.map_err(RivError::from)?;
    let rep: VerifyReport = verify(&desired, &sched, &db.block_ids(&desired.event)?, tz);
    if json {
        println!("{}", serde_json::to_string_pretty(&rep)?);
    } else {
        print_verify(&rep, &DbCatalog { db: &db, event_id: desired.event.clone(), tz });
    }
    Ok(if rep.all_ok() { 0 } else { 5 })
}

fn print_verify(rep: &VerifyReport, cat: &dyn crate::plan::CatalogView) {
    use crate::i18n::t;
    if rep.all_ok() {
        println!("{}", t("Verify: your schedule matches the spec."));
    }
    for d in rep.diffs() {
        let name = cat.session(&d.target).map(|s| s.code().to_string()).unwrap_or_else(|| d.target.clone());
        println!("{name}: expected {}, observed {}", d.expected, d.observed);
    }
    for p in &rep.pinned {
        println!("pinned (not managed): {}", cat.session(p).map(|s| s.code().to_string()).unwrap_or_else(|| p.clone()));
    }
}

pub async fn run_schedule(event: &str, json: bool) -> Result<i32> {
    let db = open_db()?;
    let tz = event_tz(&db, event);
    let sched = make_api().get_schedule(event).await.map_err(RivError::from)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&sched)?);
    } else {
        print!("{}", crate::schedule::render_schedule(&db, event, tz, &sched)?);
    }
    Ok(0)
}
