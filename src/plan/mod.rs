//! Plans: a pure diff between desired state and observed schedule. Building a plan never writes to the API.

mod build;
mod render;

pub use build::{PlanInput, build_plan, make_plan};
pub use render::render_plan;

use crate::api::{Schedule, Session};
use crate::error::{Result, RivError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
pub const PLAN_TTL_MINUTES: i64 = 30;
pub const FLAG_SEAT_LOSS: &str = "accept-seat-loss";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActionKind {
    #[serde(rename = "cancel")]
    Cancel,
    #[serde(rename = "reserve")]
    #[default]
    Reserve,
    #[serde(rename = "favorite")]
    Favorite,
    #[serde(rename = "unfavorite")]
    Unfavorite,
    #[serde(rename = "block.create")]
    BlockCreate,
    #[serde(rename = "block.update")]
    BlockUpdate,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionKind::Cancel => "cancel",
            ActionKind::Reserve => "reserve",
            ActionKind::Favorite => "favorite",
            ActionKind::Unfavorite => "unfavorite",
            ActionKind::BlockCreate => "block.create",
            ActionKind::BlockUpdate => "block.update",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub seq: u32,
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<u32>,
    /// The session was managed as `reserved` before but is no longer observed (may be full now).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rereserve: bool,
    // block.* fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personal_time_id: Option<String>,
}

impl Action {
    /// What the action targets, for the journal: a sessionId or a block key.
    pub fn target(&self) -> String {
        self.session_id.clone().or_else(|| self.key.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub schema_version: u32,
    pub plan_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub event: String,
    pub account: String,
    pub desired_hash: String,
    pub observed_hash: String,
    pub catalog_version: String,
    pub actions: Vec<Action>,
    pub unmanaged: Vec<String>,
    pub alternatives: BTreeMap<String, Vec<String>>,
    pub warnings: Vec<String>,
    pub required_flags: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
    pub fn expired_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at).map(|e| now >= e).unwrap_or(true)
    }
}

/// Hash of the schedule's meaning: reserved ids, favorite ids, and personal time as (title,start,end).
pub fn observed_hash(s: &Schedule) -> String {
    let mut reserved = s.reserved.clone();
    reserved.sort();
    let mut favorites = s.favorites.clone();
    favorites.sort();
    let mut pt: Vec<_> = s.personal_time.iter().map(|p| (&p.title, &p.start_date_time, &p.end_date_time)).collect();
    pt.sort();
    let canon = serde_json::json!({ "reserved": reserved, "favorites": favorites, "personalTime": pt });
    format!("sha256:{}", hex::encode(Sha256::digest(canon.to_string().as_bytes())))
}

/// Read access to catalog facts the planner needs. Implemented over SQLite and over plain vectors in tests.
pub trait CatalogView {
    fn session(&self, id: &str) -> Option<Session>;
    /// Other sessions with the same title (including `s` itself).
    fn same_title(&self, s: &Session) -> Vec<Session>;
    fn tz(&self) -> chrono_tz::Tz;
}

pub struct DbCatalog<'a> {
    pub db: &'a crate::db::Db,
    pub event_id: String,
    pub tz: chrono_tz::Tz,
}

impl CatalogView for DbCatalog<'_> {
    fn session(&self, id: &str) -> Option<Session> {
        self.db
            .find_sessions(&self.event_id, id)
            .ok()?
            .into_iter()
            .find(|s| s.session.session_id == id)
            .map(|s| s.session)
    }
    fn same_title(&self, s: &Session) -> Vec<Session> {
        self.db.sessions_by_title(&self.event_id, &s.title).unwrap_or_default().into_iter().map(|x| x.session).collect()
    }
    fn tz(&self) -> chrono_tz::Tz {
        self.tz
    }
}

pub fn plan_path(plan_id: &str) -> PathBuf {
    crate::paths::plans_dir().join(format!("{plan_id}.json"))
}

pub fn save_plan(plan: &Plan, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let p = dir.join(format!("{}.json", plan.plan_id));
    std::fs::write(&p, serde_json::to_vec_pretty(plan)?)?;
    Ok(p)
}

pub fn load_plan(plan_id: &str, dir: &Path) -> Result<Plan> {
    // A planId is a ULID; refuse anything that could escape the plans directory.
    if plan_id.is_empty() || !plan_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(RivError::plan_rejected(format!("invalid plan id `{plan_id}`")));
    }
    let p = dir.join(format!("{plan_id}.json"));
    let bytes = std::fs::read(&p)
        .map_err(|_| RivError::plan_rejected(format!("no approved plan `{plan_id}`: run `riv plan` first")))?;
    serde_json::from_slice(&bytes).map_err(|e| RivError::plan_rejected(format!("plan file is unreadable: {e}")))
}
