//! apply / resume / verify: executes an approved plan against the API and keeps the journal.

mod exec;
mod tasks;
pub mod verify;

pub use tasks::{BANNER, render_tasks};
pub use verify::{VerifyReport, verify};

use crate::api::{EventsApi, PersonalTime, Schedule};
use crate::db::Db;
use crate::desired::{Desired, Want, desired_hash};
use crate::error::{Result, RivError};
use crate::plan::service::{event_tz_of, read_desired};
use crate::plan::{ActionKind, CatalogView, DbCatalog, FLAG_SEAT_LOSS, Plan, load_plan, observed_hash};
use chrono::{DateTime, Utc};
use exec::Exec;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ActionStatus {
    Planned,
    Done,
    Already,
    Failed,
    Unknown,
    Skipped,
}

impl ActionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionStatus::Planned => "PLANNED",
            ActionStatus::Done => "DONE",
            ActionStatus::Already => "ALREADY",
            ActionStatus::Failed => "FAILED",
            ActionStatus::Unknown => "UNKNOWN",
            ActionStatus::Skipped => "SKIPPED",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "DONE" => Self::Done,
            "ALREADY" => Self::Already,
            "FAILED" => Self::Failed,
            "UNKNOWN" => Self::Unknown,
            "SKIPPED" => Self::Skipped,
            _ => Self::Planned,
        }
    }
}

/// One plan action plus its live state.
#[derive(Debug, Clone)]
pub struct Slot {
    pub action: crate::plan::Action,
    pub status: ActionStatus,
    pub detail: Option<String>,
    pub attempts: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub seq: u32,
    pub kind: String,
    pub target: String,
    pub code: Option<String>,
    pub status: ActionStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    pub run_id: String,
    pub plan_id: String,
    pub outcomes: Vec<Outcome>,
    pub notes: Vec<String>,
    pub stopped: Option<String>,
    pub verify: VerifyReport,
}

impl ApplyReport {
    pub fn has_unknown(&self) -> bool {
        self.outcomes.iter().any(|o| o.status == ActionStatus::Unknown)
    }
    pub fn is_clean(&self) -> bool {
        self.stopped.is_none()
            && self.outcomes.iter().all(|o| matches!(o.status, ActionStatus::Done | ActionStatus::Already))
    }
    /// 0 success, 5 partial failure.
    pub fn exit_code(&self) -> i32 {
        if self.is_clean() { 0 } else { 5 }
    }
    pub fn count(&self, s: ActionStatus) -> usize {
        self.outcomes.iter().filter(|o| o.status == s).count()
    }
}

pub struct ApplyRequest<'a> {
    /// Required unless `resume_run` is given.
    pub plan_id: Option<&'a str>,
    pub resume_run: Option<&'a str>,
    pub spec_path: &'a Path,
    pub accept_seat_loss: bool,
    pub now: DateTime<Utc>,
    pub account: &'a str,
    /// Language for generated text (tasks.md). Passed in so output never depends on ambient settings.
    pub lang: crate::i18n::Lang,
}

fn slots_of(plan: &Plan) -> Vec<Slot> {
    plan.actions
        .iter()
        .map(|a| Slot { action: a.clone(), status: ActionStatus::Planned, detail: None, attempts: 0 })
        .collect()
}

/// Resolve UNKNOWN actions against the schedule: confirmed ones become DONE, the rest go back to PLANNED
/// so only the missing part is sent again.
fn reconcile(slots: &mut [Slot], s: &Schedule) {
    let has = |title: &Option<String>, st: &Option<String>, en: &Option<String>| {
        s.personal_time.iter().any(|p: &PersonalTime| {
            Some(&p.title) == title.as_ref()
                && Some(&p.start_date_time) == st.as_ref()
                && Some(&p.end_date_time) == en.as_ref()
        })
    };
    for slot in slots.iter_mut().filter(|x| x.status == ActionStatus::Unknown) {
        let a = &slot.action;
        let sid = a.session_id.clone().unwrap_or_default();
        let applied = match a.kind {
            ActionKind::Reserve => s.reserved.contains(&sid),
            ActionKind::Cancel => !s.reserved.contains(&sid),
            ActionKind::Favorite => s.favorites.contains(&sid),
            ActionKind::Unfavorite => !s.favorites.contains(&sid),
            ActionKind::BlockCreate | ActionKind::BlockUpdate => has(&a.title, &a.start_utc, &a.end_utc),
            ActionKind::BlockDelete => {
                !s.personal_time.iter().any(|p| Some(&p.personal_time_id) == a.personal_time_id.as_ref())
            }
        };
        if applied {
            slot.status = ActionStatus::Done;
            slot.detail = Some("confirmed by the schedule after an unknown outcome".into());
        } else {
            slot.status = ActionStatus::Planned;
            slot.detail = Some("not found in the schedule; sending again".into());
        }
    }
}

fn check_flags(plan: &Plan, accept_seat_loss: bool) -> Result<()> {
    if plan.required_flags.iter().any(|f| f == FLAG_SEAT_LOSS) && !accept_seat_loss {
        return Err(RivError::plan_rejected(format!(
            "this plan replaces reservations and may lose a seat; review the diff and pass --{FLAG_SEAT_LOSS} to apply"
        )));
    }
    Ok(())
}

fn check_account(plan: &Plan, account: &str) -> Result<()> {
    if plan.account != account {
        return Err(RivError::plan_rejected("the plan was made for a different account; run `riv plan` again"));
    }
    Ok(())
}

fn check_spec(plan: &Plan, desired: &Desired) -> Result<()> {
    if desired_hash(desired) != plan.desired_hash {
        return Err(RivError::plan_rejected("the spec changed since the plan was made; run `riv plan` again"));
    }
    Ok(())
}

/// Execute an approved plan (or resume a run). `confirm` is asked once before any write (TTY prompt in the CLI).
pub async fn apply(
    api: &dyn EventsApi,
    db: &Db,
    plans_dir: &Path,
    req: &ApplyRequest<'_>,
    confirm: &dyn Fn(&Plan) -> bool,
) -> Result<ApplyReport> {
    let (plan, run_id, mut slots, resuming) = match req.resume_run {
        Some(run) => {
            let row = db.run(run)?.ok_or_else(|| RivError::plan_rejected(format!("no such run `{run}`")))?;
            let plan = load_plan(&row.plan_id, plans_dir)?;
            let prior = db.run_actions(run)?;
            let mut slots = slots_of(&plan);
            for s in &mut slots {
                if let Some(p) = prior.iter().find(|p| p.seq == s.action.seq) {
                    s.status = ActionStatus::parse(&p.status);
                    s.detail = p.detail.clone();
                    s.attempts = p.attempts;
                    // 409 and throttling did not execute anything: safe to try again.
                    if s.status == ActionStatus::Skipped
                        && s.detail.as_deref().is_some_and(|d| d.contains("closed (409)"))
                    {
                        s.status = ActionStatus::Planned;
                    }
                }
            }
            (plan, run.to_string(), slots, true)
        }
        None => {
            let id = req
                .plan_id
                .ok_or_else(|| RivError::plan_rejected("`riv apply` needs --plan <planId> from `riv plan`"))?;
            let plan = load_plan(id, plans_dir)?;
            (plan.clone(), crate::ids::new_ulid(), slots_of(&plan), false)
        }
    };
    if !resuming && plan.expired_at(req.now) {
        return Err(RivError::plan_rejected("the plan expired (30 minutes); run `riv plan` again"));
    }
    check_account(&plan, req.account)?;
    let desired = read_desired(req.spec_path)?;
    check_spec(&plan, &desired)?;
    check_flags(&plan, req.accept_seat_loss)?;
    if desired.event != plan.event {
        return Err(RivError::plan_rejected("the spec targets a different event than the plan"));
    }

    let tz = event_tz_of(db, &desired);
    let catalog = DbCatalog { db, event_id: plan.event.clone(), tz };
    let observed = api.get_schedule(&plan.event).await?;
    if !resuming && observed_hash(&observed) != plan.observed_hash {
        return Err(RivError::plan_rejected("the schedule changed since the plan was made; run `riv plan` again"));
    }
    if resuming {
        reconcile(&mut slots, &observed);
    }
    if !confirm(&plan) {
        return Err(RivError::plan_rejected("not confirmed; nothing was changed"));
    }

    if !resuming {
        db.start_run(&run_id, &plan.plan_id, req.account)?;
    }
    let mut ex = Exec::new(api, db, &catalog, &run_id, &plan.event, slots);
    // Persist the initial (or reconciled) state so a crash leaves a resumable journal.
    for i in 0..ex.slots.len() {
        let s = &ex.slots[i];
        db.upsert_action(
            &run_id,
            &crate::db::journal::ActionRow {
                seq: s.action.seq,
                kind: s.action.kind.as_str().into(),
                target: s.action.target(),
                status: s.status.as_str().into(),
                detail: s.detail.clone(),
                attempts: s.attempts,
            },
        )?;
    }
    let run_result = ex.run().await;
    let (slots, notes, stopped) = (ex.slots, ex.notes, ex.stopped);

    // Final read-back: verify intent against reality, record managed sessions and block ids, regenerate tasks.md.
    let final_schedule = api.get_schedule(&plan.event).await.unwrap_or(observed);
    record_state(db, &desired, &final_schedule, &slots, &plan.event)?;
    let verify_report = verify(&desired, &final_schedule, &db.block_ids(&plan.event)?, tz);
    let outcomes = outcomes_of(&slots, &catalog);
    if let Some(dir) = req.spec_path.parent()
        && dir.is_dir()
    {
        let _ = std::fs::write(
            dir.join("tasks.md"),
            render_tasks(&desired, &final_schedule, &catalog, &outcomes, req.lang),
        );
    }
    let summary = format!(
        "{} done, {} already, {} failed, {} unknown, {} skipped",
        count(&slots, ActionStatus::Done),
        count(&slots, ActionStatus::Already),
        count(&slots, ActionStatus::Failed),
        count(&slots, ActionStatus::Unknown),
        count(&slots, ActionStatus::Skipped)
    );
    db.finish_run(&run_id, &summary)?;
    run_result?;
    Ok(ApplyReport { run_id, plan_id: plan.plan_id, outcomes, notes, stopped, verify: verify_report })
}

fn count(slots: &[Slot], s: ActionStatus) -> usize {
    slots.iter().filter(|x| x.status == s).count()
}

fn outcomes_of(slots: &[Slot], cat: &dyn CatalogView) -> Vec<Outcome> {
    slots
        .iter()
        .map(|s| Outcome {
            seq: s.action.seq,
            kind: s.action.kind.as_str().into(),
            target: s.action.target(),
            code: s
                .action
                .session_id
                .as_deref()
                .and_then(|id| cat.session(id))
                .map(|x| x.code().to_string())
                .or_else(|| s.action.code.clone()),
            status: s.status,
            detail: s.detail.clone(),
        })
        .collect()
}

/// Update `managed` for every non-pinned desired session whose intent is now observed, and map block keys to ids.
fn record_state(db: &Db, d: &Desired, sched: &Schedule, slots: &[Slot], event: &str) -> Result<()> {
    for ds in d.sessions.iter().filter(|s| !s.pin) {
        let reserved = sched.reserved.contains(&ds.id);
        let favorite = sched.favorites.contains(&ds.id);
        let satisfied = match ds.want {
            Want::Reserved => reserved,
            Want::Favorite => favorite || reserved,
            Want::None => !reserved && !favorite,
        };
        if satisfied {
            db.set_managed(
                event,
                &ds.id,
                if ds.want == Want::Favorite && reserved { Want::Reserved } else { ds.want },
            )?;
        }
    }
    let known = db.block_ids(event)?;
    for b in &d.blocks {
        if b.want == crate::desired::BlockWant::None {
            if let Some(id) = known.get(&b.key)
                && !sched.personal_time.iter().any(|p| &p.personal_time_id == id)
            {
                db.remove_block_id(event, &b.key)?;
            }
            continue;
        }
        if known.contains_key(&b.key) && slots.iter().all(|s| s.action.key.as_deref() != Some(&b.key)) {
            continue;
        }
        let Some((st, en)) = b.utc_range(crate::timeutil::parse_tz(&d.timezone).unwrap_or(chrono_tz::UTC)) else {
            continue;
        };
        let taken: Vec<&String> = known.values().collect();
        if let Some(p) = sched.personal_time.iter().find(|p| {
            p.title == b.title
                && p.start_date_time == st
                && p.end_date_time == en
                && !taken.contains(&&p.personal_time_id)
        }) {
            db.set_block_id(event, &b.key, &p.personal_time_id)?;
        }
    }
    Ok(())
}

/// Text report for the terminal.
pub fn render_report(r: &ApplyReport, cat: &dyn CatalogView) -> String {
    use crate::i18n::t;
    let mut out = format!("Run {}  (plan {})\n", r.run_id, r.plan_id);
    for o in &r.outcomes {
        let name = o.code.clone().unwrap_or_else(|| o.target.clone());
        let title = cat.session(&o.target).map(|s| format!("  {}", s.title)).unwrap_or_default();
        out.push_str(&format!(
            "{:<8} {:<12} {}{}{}\n",
            o.status.as_str(),
            o.kind,
            name,
            title,
            o.detail.as_deref().map(|d| format!("  [{d}]")).unwrap_or_default()
        ));
    }
    for n in &r.notes {
        out.push_str(&format!("! {n}\n"));
    }
    if let Some(s) = &r.stopped {
        out.push_str(&format!("{} {s}\n", t("Stopped:")));
    }
    if r.has_unknown() {
        out.push_str(&format!(
            "{}\n",
            crate::i18n::tf(
                "Some outcomes are unknown. Reconcile and resend only what is missing: riv apply --resume {run}",
                &[("run", &r.run_id)]
            )
        ));
    }
    let diffs: Vec<_> = r.verify.diffs().collect();
    if diffs.is_empty() {
        out.push_str(&format!("{}\n", t("Verify: your schedule matches the spec.")));
    } else {
        out.push_str(&format!("{}\n", t("Verify: differences remain:")));
        for d in diffs {
            let name = cat.session(&d.target).map(|s| s.code().to_string()).unwrap_or_else(|| d.target.clone());
            out.push_str(&format!("  {name}: expected {}, observed {}\n", d.expected, d.observed));
        }
    }
    out
}
