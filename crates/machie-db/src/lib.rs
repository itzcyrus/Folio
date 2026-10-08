//! Machie persistence layer: SQLite (bundled, single file) with forward-only migrations.
//!
//! Rules of the house:
//! - The DB carries a `user_version` pragma; every schema change is a numbered migration.
//! - Migrations run automatically on open and only move forward. If a newer app wrote
//!   this file, we refuse to open it rather than corrupt it.
//! - All data is local-first; nothing here ever leaves the machine unless a later
//!   feature explicitly says so.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database at {path} was created by a newer version of Machie (schema v{found}, this build supports up to v{max}); please upgrade Machie")]
    TooNew { path: String, found: u32, max: u32 },
}

/// Highest schema version this build understands. Bump with each migration added.
pub const SCHEMA_VERSION: u32 = 1;

pub struct Db {
    conn: rusqlite::Connection,
    path: String,
}

impl Db {
    /// Open (creating if necessary) the database at `path`, apply pragmas and migrations.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open(path)?;
        let path_str = path.display().to_string();
        Self::configure(&conn)?;
        let found = current_version(&conn);
        if found > SCHEMA_VERSION {
            return Err(DbError::TooNew {
                path: path_str,
                found,
                max: SCHEMA_VERSION,
            });
        }
        migrate(&conn, found)?;
        Ok(Self {
            conn,
            path: path_str,
        })
    }

    /// In-memory database, used by tests.
    pub fn in_memory() -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrate(&conn, 0)?;
        Ok(Self {
            conn,
            path: ":memory:".into(),
        })
    }

    fn configure(conn: &rusqlite::Connection) -> Result<(), DbError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn schema_version(&self) -> u32 {
        current_version(&self.conn)
    }

    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// Insert a session row; returns its id.
    pub fn create_session(&self, title: &str) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO session (title) VALUES (?1)",
            rusqlite::params![title],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Append one message to a session; returns the message id.
    pub fn add_message(&self, session_id: i64, role: &str, content: &str) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO message (session_id, role, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, role, content],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List sessions as (id, title, created_at, message_count), newest first.
    pub fn list_sessions(&self) -> Result<Vec<(i64, String, String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at,
                    (SELECT COUNT(*) FROM message m WHERE m.session_id = s.id)
             FROM session s ORDER BY s.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Fetch all messages of a session in order as (role, content).
    pub fn session_messages(&self, session_id: i64) -> Result<Vec<(String, String)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content FROM message WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Delete a session and (via FK cascade) its messages. Returns rows affected.
    pub fn delete_session(&self, session_id: i64) -> Result<usize, DbError> {
        Ok(self
            .conn
            .execute("DELETE FROM session WHERE id = ?1", rusqlite::params![session_id])?)
    }
}

fn current_version(conn: &rusqlite::Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0)
        .max(0) as u32
}

/// Forward-only migration runner. Each step sets `user_version` before applying.
fn migrate(conn: &rusqlite::Connection, from: u32) -> Result<(), DbError> {
    if from < 1 {
        conn.execute_batch(
            r#"
            BEGIN;
            PRAGMA user_version = 1;
            CREATE TABLE meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE session (
                id         INTEGER PRIMARY KEY,
                title      TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE message (
                id         INTEGER PRIMARY KEY,
                session_id INTEGER NOT NULL REFERENCES session(id) ON DELETE CASCADE,
                role       TEXT NOT NULL CHECK (role IN ('system','user','assistant','tool')),
                content    TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX idx_message_session ON message(session_id);
            COMMIT;
            "#,
        )?;
    }
    // Future: if from < 2 { ... }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_gets_schema_v1() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.schema_version(), 1);
    }

    #[test]
    fn session_and_message_roundtrip_with_cascade() {
        let db = Db::in_memory().unwrap();
        let sid = db.create_session("hello").unwrap();
        db.add_message(sid, "user", "hi").unwrap();
        db.add_message(sid, "assistant", "yo").unwrap();

        let msgs = db.session_messages(sid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], ("user".into(), "hi".into()));

        let list = db.list_sessions().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].3, 2); // message count

        db.delete_session(sid).unwrap();
        assert!(db.session_messages(sid).unwrap().is_empty());
    }

    #[test]
    fn bad_role_is_rejected() {
        let db = Db::in_memory().unwrap();
        let sid = db.create_session("s").unwrap();
        assert!(db.add_message(sid, "hacker", "x").is_err());
    }

    #[test]
    fn reopening_file_db_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machie.db");
        {
            let db = Db::open(&path).unwrap();
            db.create_session("persisted").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version(), 1);
        assert_eq!(db.list_sessions().unwrap().len(), 1);
    }

    #[test]
    fn refuses_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machie.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 99).unwrap();
        }
        let err = Db::open(&path).err().expect("open should fail");
        assert!(matches!(err, DbError::TooNew { found: 99, .. }), "{err}");
    }
}
