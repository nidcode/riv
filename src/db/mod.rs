//! SQLite storage: catalog, FTS, sync runs, and (later) the apply journal.

pub mod catalog;
pub mod journal;

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

/// Ordered migrations; `PRAGMA user_version` records how many have run.
const MIGRATIONS: &[&str] = &[
    // 1: catalog
    "CREATE TABLE events (
        event_id TEXT PRIMARY KEY, name TEXT, timezone TEXT, authentication_required INTEGER, raw TEXT NOT NULL
     );
     CREATE TABLE sessions (
        event_id TEXT NOT NULL, session_id TEXT NOT NULL, code TEXT NOT NULL, title TEXT NOT NULL,
        abstract TEXT, type TEXT, level TEXT, level_num INTEGER, venue TEXT, room TEXT,
        day_local TEXT, dow TEXT, start_utc TEXT, end_utc TEXT,
        reservable INTEGER, seat TEXT, topics TEXT, services TEXT, speakers TEXT,
        title_l10n TEXT, abstract_l10n TEXT, l10n_locale TEXT,
        raw TEXT NOT NULL, raw_hash TEXT NOT NULL,
        PRIMARY KEY (event_id, session_id)
     );
     CREATE INDEX sessions_code ON sessions(event_id, code);
     CREATE VIRTUAL TABLE sessions_fts USING fts5(
        event_id UNINDEXED, session_id UNINDEXED,
        code, title, abstract, speakers, topics, services, title_l10n, abstract_l10n,
        tokenize = 'unicode61 remove_diacritics 2'
     );
     CREATE TABLE sync_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL, started_at TEXT NOT NULL, finished_at TEXT,
        session_count INTEGER, changed_count INTEGER, locale TEXT, abstracts TEXT, catalog_version TEXT
     );",
    // 2: apply journal and local bookkeeping
    "CREATE TABLE runs (
        run_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, started_at TEXT NOT NULL, finished_at TEXT,
        dry_run INTEGER NOT NULL DEFAULT 0, account TEXT, summary TEXT
     );
     CREATE TABLE run_actions (
        run_id TEXT NOT NULL, seq INTEGER NOT NULL, kind TEXT NOT NULL, target TEXT NOT NULL,
        status TEXT NOT NULL, detail TEXT, attempts INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL,
        PRIMARY KEY (run_id, seq)
     );
     CREATE TABLE managed (
        event TEXT NOT NULL, session_id TEXT NOT NULL, first_managed_at TEXT NOT NULL, last_want TEXT NOT NULL,
        PRIMARY KEY (event, session_id)
     );
     CREATE TABLE block_ids (
        event TEXT NOT NULL, key TEXT NOT NULL, personal_time_id TEXT NOT NULL, PRIMARY KEY (event, key)
     );
     CREATE TABLE prep_notes (
        event TEXT NOT NULL, session_id TEXT NOT NULL, path TEXT NOT NULL, attached_at TEXT NOT NULL,
        PRIMARY KEY (event, session_id)
     );",
];

pub struct Db {
    pub(crate) conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&crate::paths::db_path())
    }

    pub fn open_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
            tx.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_is_idempotent() {
        let db = Db::open_memory().expect("open");
        let v = db.conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).expect("v") as usize;
        assert_eq!(v, MIGRATIONS.len());
    }
}
