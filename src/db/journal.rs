//! Apply journal, managed-session set and block-id mapping.

use super::Db;
use crate::desired::Want;
use crate::error::Result;
use rusqlite::params;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ActionRow {
    pub seq: u32,
    pub kind: String,
    pub target: String,
    pub status: String,
    pub detail: Option<String>,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    pub run_id: String,
    pub plan_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub account: Option<String>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Db {
    pub fn managed_map(&self, event: &str) -> Result<HashMap<String, Want>> {
        let mut st = self.conn.prepare("SELECT session_id, last_want FROM managed WHERE event=?1")?;
        let rows = st.query_map([event], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = HashMap::new();
        for r in rows {
            let (id, w) = r?;
            let want = match w.as_str() {
                "reserved" => Want::Reserved,
                "favorite" => Want::Favorite,
                _ => Want::None,
            };
            out.insert(id, want);
        }
        Ok(out)
    }

    pub fn set_managed(&self, event: &str, session_id: &str, want: Want) -> Result<()> {
        self.conn.execute(
            "INSERT INTO managed(event,session_id,first_managed_at,last_want) VALUES (?1,?2,?3,?4)
             ON CONFLICT(event,session_id) DO UPDATE SET last_want=?4",
            params![event, session_id, now(), want.as_str()],
        )?;
        Ok(())
    }

    pub fn block_ids(&self, event: &str) -> Result<HashMap<String, String>> {
        let mut st = self.conn.prepare("SELECT key, personal_time_id FROM block_ids WHERE event=?1")?;
        let rows = st.query_map([event], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_block_id(&self, event: &str, key: &str, personal_time_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO block_ids(event,key,personal_time_id) VALUES (?1,?2,?3)
             ON CONFLICT(event,key) DO UPDATE SET personal_time_id=?3",
            params![event, key, personal_time_id],
        )?;
        Ok(())
    }

    pub fn start_run(&self, run_id: &str, plan_id: &str, account: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs(run_id,plan_id,started_at,account) VALUES (?1,?2,?3,?4)",
            params![run_id, plan_id, now(), account],
        )?;
        Ok(())
    }

    pub fn finish_run(&self, run_id: &str, summary: &str) -> Result<()> {
        self.conn
            .execute("UPDATE runs SET finished_at=?2, summary=?3 WHERE run_id=?1", params![run_id, now(), summary])?;
        Ok(())
    }

    pub fn run(&self, run_id: &str) -> Result<Option<RunRow>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT run_id,plan_id,started_at,finished_at,account FROM runs WHERE run_id=?1",
                [run_id],
                |r| {
                    Ok(RunRow {
                        run_id: r.get(0)?,
                        plan_id: r.get(1)?,
                        started_at: r.get(2)?,
                        finished_at: r.get(3)?,
                        account: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn runs_for_plan(&self, plan_id: &str) -> Result<Vec<RunRow>> {
        let mut st = self.conn.prepare("SELECT run_id,plan_id,started_at,finished_at,account FROM runs WHERE plan_id=?1 ORDER BY started_at, rowid")?;
        let rows = st.query_map([plan_id], |r| {
            Ok(RunRow {
                run_id: r.get(0)?,
                plan_id: r.get(1)?,
                started_at: r.get(2)?,
                finished_at: r.get(3)?,
                account: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn upsert_action(&self, run_id: &str, a: &ActionRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO run_actions(run_id,seq,kind,target,status,detail,attempts,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(run_id,seq) DO UPDATE SET status=?5, detail=?6, attempts=?7, updated_at=?8",
            params![run_id, a.seq, a.kind, a.target, a.status, a.detail, a.attempts, now()],
        )?;
        Ok(())
    }

    pub fn run_actions(&self, run_id: &str) -> Result<Vec<ActionRow>> {
        let mut st = self
            .conn
            .prepare("SELECT seq,kind,target,status,detail,attempts FROM run_actions WHERE run_id=?1 ORDER BY seq")?;
        let rows = st.query_map([run_id], |r| {
            Ok(ActionRow {
                seq: r.get(0)?,
                kind: r.get(1)?,
                target: r.get(2)?,
                status: r.get(3)?,
                detail: r.get(4)?,
                attempts: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}
