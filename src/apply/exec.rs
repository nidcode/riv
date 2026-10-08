//! The apply executor: a state machine over the plan's actions. Every status change is journaled.

use super::{ActionStatus, Slot};
use crate::api::{ApiError, BulkFailureCode, BulkResult, EventsApi, Op, PersonalTimeInput};
use crate::db::Db;
use crate::db::journal::ActionRow;
use crate::error::{Result, RivError};
use crate::plan::{ActionKind, CatalogView};
use std::collections::HashSet;

const MAX_BATCH: usize = 10;

pub(super) fn op_of(kind: ActionKind) -> Op {
    match kind {
        ActionKind::Cancel => Op::CancelReservation,
        ActionKind::Reserve => Op::ReserveSessions,
        ActionKind::Favorite => Op::AssociateFavorites,
        ActionKind::Unfavorite => Op::DisassociateFavorite,
        ActionKind::BlockCreate => Op::CreatePersonalTime,
        ActionKind::BlockUpdate => Op::UpdatePersonalTime,
        ActionKind::BlockDelete => Op::DeletePersonalTime,
    }
}

pub(super) struct Exec<'a> {
    pub api: &'a dyn EventsApi,
    pub db: &'a Db,
    pub cat: &'a dyn CatalogView,
    pub run_id: &'a str,
    pub event: &'a str,
    pub slots: Vec<Slot>,
    /// Operations that answered 409; nothing of that kind is sent again in this run.
    closed: HashSet<ActionKind>,
    /// Why writing stopped early (unknown outcome, throttling). Remaining actions stay PLANNED.
    pub stopped: Option<String>,
    pub notes: Vec<String>,
}

impl<'a> Exec<'a> {
    pub fn new(
        api: &'a dyn EventsApi,
        db: &'a Db,
        cat: &'a dyn CatalogView,
        run_id: &'a str,
        event: &'a str,
        slots: Vec<Slot>,
    ) -> Self {
        Self { api, db, cat, run_id, event, slots, closed: HashSet::new(), stopped: None, notes: Vec::new() }
    }

    fn code(&self, id: &str) -> String {
        self.cat.session(id).map(|s| s.code().to_string()).unwrap_or_else(|| id.to_string())
    }

    fn set(&mut self, i: usize, status: ActionStatus, detail: Option<String>) -> Result<()> {
        let s = &mut self.slots[i];
        s.status = status;
        s.detail = detail;
        let row = ActionRow {
            seq: s.action.seq,
            kind: s.action.kind.as_str().to_string(),
            target: s.action.target(),
            status: status.as_str().to_string(),
            detail: s.detail.clone(),
            attempts: s.attempts,
        };
        self.db.upsert_action(self.run_id, &row)
    }

    fn deps_ok(&self, i: usize) -> bool {
        self.slots[i].action.depends_on.iter().all(|seq| {
            self.slots
                .iter()
                .find(|s| s.action.seq == *seq)
                .is_some_and(|d| matches!(d.status, ActionStatus::Done | ActionStatus::Already))
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut i = 0;
        while i < self.slots.len() {
            if self.slots[i].status != ActionStatus::Planned {
                i += 1;
                continue;
            }
            if self.stopped.is_some() {
                break;
            }
            let kind = self.slots[i].action.kind;
            if !self.deps_ok(i) {
                self.set(i, ActionStatus::Skipped, Some("a prerequisite action did not complete".into()))?;
            } else if self.closed.contains(&kind) {
                self.set(i, ActionStatus::Skipped, Some("operation closed (409)".into()))?;
            } else if matches!(kind, ActionKind::Reserve | ActionKind::Favorite) {
                let batch = self.collect_batch(i);
                self.bulk(kind, &batch).await?;
            } else {
                self.single(i).await?;
            }
            if self.slots[i].status == ActionStatus::Planned && self.stopped.is_none() {
                self.set(i, ActionStatus::Failed, Some("internal: action was not processed".into()))?;
            }
        }
        Ok(())
    }

    /// Consecutive same-kind actions, up to 10 and up to the remaining per-minute quota. An action that
    /// depends on a cancel is always sent alone, right after that cancel.
    fn collect_batch(&self, i: usize) -> Vec<usize> {
        let kind = self.slots[i].action.kind;
        if !self.slots[i].action.depends_on.is_empty() {
            return vec![i];
        }
        let room = (self.api.remaining(op_of(kind)) as usize).clamp(1, MAX_BATCH);
        let mut batch = vec![i];
        for j in i + 1..self.slots.len() {
            let s = &self.slots[j];
            if s.action.kind != kind {
                break;
            }
            if batch.len() >= room {
                break;
            }
            if s.status == ActionStatus::Planned && s.action.depends_on.is_empty() {
                batch.push(j);
            }
        }
        batch
    }

    fn ids_of(&self, batch: &[usize]) -> Vec<String> {
        batch.iter().map(|&j| self.slots[j].action.session_id.clone().unwrap_or_default()).collect()
    }

    async fn bulk(&mut self, kind: ActionKind, batch: &[usize]) -> Result<()> {
        let ids = self.ids_of(batch);
        let op = op_of(kind);
        if (self.api.remaining(op) as usize) < ids.len() {
            self.api.wait_for_quota(op).await;
        }
        for &j in batch {
            self.slots[j].attempts += 1;
        }
        let res = if kind == ActionKind::Reserve {
            self.api.reserve(self.event, &ids).await
        } else {
            self.api.associate_favorites(self.event, &ids).await
        };
        match res {
            Ok(r) => {
                self.apply_bulk_result(kind, batch, &ids, &r)?;
                for &j in batch {
                    if self.slots[j].status == ActionStatus::Failed {
                        self.maybe_restore(j).await?;
                    }
                }
                Ok(())
            }
            Err(e) => self.apply_error(batch, kind, e),
        }
    }

    fn apply_bulk_result(&mut self, kind: ActionKind, batch: &[usize], ids: &[String], r: &BulkResult) -> Result<()> {
        for (&j, id) in batch.iter().zip(ids) {
            if r.successful.contains(id) {
                self.set(j, ActionStatus::Done, None)?;
            } else if let Some(f) = r.failed.iter().find(|f| &f.session_id == id) {
                let (status, detail) = self.classify_failure(kind, f.code, f.conflicts_with.as_deref());
                self.set(j, status, Some(detail))?;
            } else {
                // Neither succeeded nor failed: we do not know what happened.
                self.set(j, ActionStatus::Unknown, Some("missing from the response".into()))?;
                self.stopped = Some("a response did not account for every session".into());
            }
        }
        Ok(())
    }

    fn classify_failure(
        &self,
        kind: ActionKind,
        code: BulkFailureCode,
        conflicts: Option<&[String]>,
    ) -> (ActionStatus, String) {
        use BulkFailureCode::*;
        match code {
            AlreadyScheduled if kind == ActionKind::Reserve => (ActionStatus::Already, "already reserved".into()),
            AlreadyFavorited => (ActionStatus::Already, "already a favorite".into()),
            SessionFull => (ActionStatus::Failed, "full".into()),
            ScheduleConflict => {
                let with: Vec<String> = conflicts.unwrap_or(&[]).iter().map(|c| self.code(c)).collect();
                (
                    ActionStatus::Failed,
                    format!(
                        "schedule conflict with {}",
                        if with.is_empty() { "an existing item".into() } else { with.join(", ") }
                    ),
                )
            }
            SessionNotReservable => (ActionStatus::Failed, "session is not reservable".into()),
            InsufficientAccess => (ActionStatus::Failed, "insufficient access (registration/ticket type)".into()),
            TimePassed => (ActionStatus::Failed, "session time has passed".into()),
            // `other`, `notFavorited`, and values added after this build: a refusal we cannot act on.
            other => (ActionStatus::Failed, format!("refused ({other:?})")),
        }
    }

    fn apply_error(&mut self, batch: &[usize], kind: ActionKind, e: ApiError) -> Result<()> {
        match e {
            ApiError::Unauthorized => {
                return Err(RivError::auth(
                    "not signed in or the token was rejected; run `riv login` and `riv apply --resume`",
                ));
            }
            ApiError::Closed => {
                self.closed.insert(kind);
                for &j in batch {
                    self.set(j, ActionStatus::Skipped, Some("operation closed (409)".into()))?;
                }
            }
            ApiError::Unknown(msg) => {
                for &j in batch {
                    self.set(j, ActionStatus::Unknown, Some(msg.clone()))?;
                }
                self.stopped = Some("outcome unknown; writes stopped until the schedule is reconciled".into());
            }
            ApiError::Throttled { retry_after_secs } => {
                // A throttled request was not executed: leave the batch PLANNED so --resume can send it.
                self.stopped = Some(format!("throttled (retry after {retry_after_secs}s)"));
            }
            other => {
                for &j in batch {
                    self.set(j, ActionStatus::Failed, Some(other.to_string()))?;
                }
            }
        }
        Ok(())
    }

    /// Replacement whose reserve failed after the cancel succeeded: try to get the original seat back, once.
    async fn maybe_restore(&mut self, j: usize) -> Result<()> {
        let dep = self.slots[j].action.depends_on.first().copied();
        let Some(c) = dep.and_then(|seq| self.slots.iter().position(|s| s.action.seq == seq)) else { return Ok(()) };
        let cancel = &self.slots[c];
        if cancel.status != ActionStatus::Done || cancel.action.risk.as_deref() != Some("seat-loss") {
            return Ok(());
        }
        let Some(orig) = cancel.action.session_id.clone() else { return Ok(()) };
        let orig_code = self.code(&orig);
        self.api.wait_for_quota(Op::ReserveSessions).await;
        let note = match self.api.reserve(self.event, std::slice::from_ref(&orig)).await {
            Ok(r) if r.successful.contains(&orig) => {
                format!("The replacement failed; the original seat {orig_code} was restored.")
            }
            Ok(_) => format!(
                "The replacement failed and the original seat {orig_code} could not be restored: the seat was lost."
            ),
            Err(ApiError::Unknown(_)) => {
                self.stopped = Some("restore outcome unknown; writes stopped".into());
                format!("The replacement failed and restoring {orig_code} has an unknown outcome: check your schedule.")
            }
            Err(e) => format!("The replacement failed and restoring {orig_code} failed ({e}): the seat was lost."),
        };
        // Restoring is best effort and never guaranteed.
        let detail = format!("{}; {note}", self.slots[j].detail.clone().unwrap_or_default());
        self.set(j, ActionStatus::Failed, Some(detail))?;
        self.notes.push(note);
        Ok(())
    }

    async fn single(&mut self, i: usize) -> Result<()> {
        let a = self.slots[i].action.clone();
        let op = op_of(a.kind);
        if self.api.remaining(op) == 0 {
            self.api.wait_for_quota(op).await;
        }
        self.slots[i].attempts += 1;
        let sid = a.session_id.clone().unwrap_or_default();
        let input = || PersonalTimeInput {
            start_date_time: a.start_utc.clone().unwrap_or_default(),
            end_date_time: a.end_utc.clone().unwrap_or_default(),
            title: a.title.clone().unwrap_or_default(),
            description: a.description.clone().unwrap_or_default(),
            location: a.location.clone(),
        };
        let res = match a.kind {
            ActionKind::Cancel => self.api.cancel_reservation(self.event, &sid).await,
            ActionKind::Unfavorite => self.api.disassociate_favorite(self.event, &sid).await,
            ActionKind::BlockCreate => self.api.create_personal_time(self.event, &input()).await,
            ActionKind::BlockUpdate => {
                self.api
                    .update_personal_time(self.event, a.personal_time_id.as_deref().unwrap_or_default(), &input())
                    .await
            }
            ActionKind::BlockDelete => {
                self.api.delete_personal_time(self.event, a.personal_time_id.as_deref().unwrap_or_default()).await
            }
            ActionKind::Reserve | ActionKind::Favorite => unreachable!("bulk kinds are handled by `bulk`"),
        };
        match res {
            Ok(()) => self.set(i, ActionStatus::Done, None),
            // Removals: 404 means it is already gone, which is the intended end state.
            Err(ApiError::NotFound)
                if matches!(a.kind, ActionKind::Cancel | ActionKind::Unfavorite | ActionKind::BlockDelete) =>
            {
                self.set(i, ActionStatus::Already, Some("already gone (404)".into()))
            }
            Err(e) => self.apply_error(&[i], a.kind, e),
        }
    }
}
