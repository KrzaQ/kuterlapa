//! History of conversations and one-off commands, in SQLite:
//! ~/.local/share/kuterlapa/kuterlapa.db.
//!
//! Conversations belong to a session. One-off commands live in their own
//! table and only note which session's context they saw, so they never
//! clutter a session's history. Commands that follow each other while the
//! answer is still on screen share a thread.

use crate::paths;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::sync::Mutex;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    session TEXT NOT NULL,
    title TEXT,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    updated TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    conversation INTEGER NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL,          -- 'user' or 'assistant'
    text TEXT NOT NULL,
    profile TEXT,                -- the model profile that answered
    created TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS commands (
    id INTEGER PRIMARY KEY,
    thread INTEGER NOT NULL,     -- the id of the first command of the thread
    context_session TEXT,        -- whose context the model saw, if any
    role TEXT NOT NULL,
    text TEXT NOT NULL,
    profile TEXT,
    created TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS messages_by_conversation ON messages(conversation, id);
CREATE INDEX IF NOT EXISTS commands_by_thread ON commands(thread, id);
";

/// Commands older than this are deleted when the store opens.
const COMMAND_DAYS: i64 = 30;

pub struct Store {
    db: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub text: String,
}

impl Store {
    pub fn open() -> Result<Store> {
        let dir = paths::data_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join("kuterlapa.db");
        let db = Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
        db.execute_batch(SCHEMA)?;
        db.execute(
            "DELETE FROM commands WHERE created < datetime('now', ?1)",
            params![format!("-{COMMAND_DAYS} days")],
        )?;
        Ok(Store { db: Mutex::new(db) })
    }

    /// The session's most recent conversation, or a new one.
    pub fn current_conversation(&self, session: &str) -> Result<i64> {
        let db = self.db.lock().unwrap();
        let found: Option<i64> = db
            .query_row(
                "SELECT id FROM conversations WHERE session = ?1 ORDER BY updated DESC, id DESC LIMIT 1",
                params![session],
                |r| r.get(0),
            )
            .optional()?;
        match found {
            Some(id) => Ok(id),
            None => {
                db.execute("INSERT INTO conversations (session) VALUES (?1)", params![session])?;
                Ok(db.last_insert_rowid())
            }
        }
    }

    /// For "start a new conversation", which no button triggers yet.
    #[allow(dead_code)]
    pub fn new_conversation(&self, session: &str) -> Result<i64> {
        let db = self.db.lock().unwrap();
        db.execute("INSERT INTO conversations (session) VALUES (?1)", params![session])?;
        Ok(db.last_insert_rowid())
    }

    pub fn add_message(&self, conversation: i64, role: &str, text: &str, profile: Option<&str>) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO messages (conversation, role, text, profile) VALUES (?1, ?2, ?3, ?4)",
            params![conversation, role, text, profile],
        )?;
        db.execute("UPDATE conversations SET updated = datetime('now') WHERE id = ?1", params![conversation])?;
        Ok(())
    }

    /// The last `limit` messages of a conversation, oldest first.
    pub fn messages(&self, conversation: i64, limit: usize) -> Result<Vec<Message>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT role, text FROM (SELECT id, role, text FROM messages WHERE conversation = ?1
             ORDER BY id DESC LIMIT ?2) ORDER BY id",
        )?;
        let rows = stmt.query_map(params![conversation, limit as i64], |r| {
            Ok(Message { role: r.get(0)?, text: r.get(1)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Adds a command to a thread (None: start a new thread). Returns the
    /// thread id.
    pub fn add_command(
        &self,
        thread: Option<i64>,
        session: Option<&str>,
        role: &str,
        text: &str,
        profile: Option<&str>,
    ) -> Result<i64> {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO commands (thread, context_session, role, text, profile) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![thread.unwrap_or(0), session, role, text, profile],
        )?;
        let id = db.last_insert_rowid();
        let thread = thread.unwrap_or(id);
        if thread == id {
            db.execute("UPDATE commands SET thread = ?1 WHERE id = ?1", params![id])?;
        }
        Ok(thread)
    }

    pub fn thread(&self, thread: i64) -> Result<Vec<Message>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT role, text FROM commands WHERE thread = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![thread], |r| Ok(Message { role: r.get(0)?, text: r.get(1)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
