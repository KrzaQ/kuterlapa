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
CREATE TABLE IF NOT EXISTS embeddings (
    key TEXT PRIMARY KEY,        -- 'msg:<id>' or 'note:<place>:<name>'
    hash TEXT NOT NULL,          -- of the text, to notice changes
    vector BLOB NOT NULL         -- little-endian f32
);
CREATE INDEX IF NOT EXISTS messages_by_conversation ON messages(conversation, id);
CREATE INDEX IF NOT EXISTS commands_by_thread ON commands(thread, id);
";

/// Commands older than this are deleted when the store opens.
const COMMAND_DAYS: i64 = 30;

pub struct Store {
    db: Mutex<Connection>,
}

/// A message with everything the chat view and the export show.
#[derive(Debug, Clone)]
pub struct FullMessage {
    pub id: i64,
    pub role: String,
    pub text: String,
    pub profile: Option<String>,
    pub created: String,
}

fn full_message(r: &rusqlite::Row) -> rusqlite::Result<FullMessage> {
    Ok(FullMessage { id: r.get(0)?, role: r.get(1)?, text: r.get(2)?, profile: r.get(3)?, created: r.get(4)? })
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

    /// Starts a new conversation; `current_conversation` returns it from now
    /// on, as the most recently updated one.
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

    /// The ids of a session's conversations that have messages, oldest
    /// first, with when each started (local time).
    pub fn conversations(&self, session: &str) -> Result<Vec<(i64, String)>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT c.id, strftime('%Y-%m-%d %H:%M', c.created, 'localtime') FROM conversations c
             WHERE c.session = ?1 AND EXISTS (SELECT 1 FROM messages m WHERE m.conversation = c.id)
             ORDER BY c.id",
        )?;
        let rows = stmt.query_map(params![session], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// A conversation's messages for the chat view and export: id, role,
    /// text, profile and time (local), oldest first, at most `limit` of the
    /// newest.
    pub fn messages_full(&self, conversation: i64, limit: usize) -> Result<Vec<FullMessage>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT id, role, text, profile, strftime('%Y-%m-%d %H:%M', created, 'localtime') FROM (
                 SELECT id, role, text, profile, created FROM messages WHERE conversation = ?1
                 ORDER BY id DESC LIMIT ?2)
             ORDER BY id",
        )?;
        let rows = stmt.query_map(params![conversation, limit as i64], full_message)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The command threads that saw a session's context, oldest first, with
    /// when each started (local time).
    pub fn threads(&self, session: &str) -> Result<Vec<(i64, String)>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT thread, strftime('%Y-%m-%d %H:%M', MIN(created), 'localtime') FROM commands
             WHERE context_session = ?1 GROUP BY thread ORDER BY MIN(id)",
        )?;
        let rows = stmt.query_map(params![session], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// A command thread's messages, like `messages_full`.
    pub fn thread_full(&self, thread: i64) -> Result<Vec<FullMessage>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT id, role, text, profile, strftime('%Y-%m-%d %H:%M', created, 'localtime')
             FROM commands WHERE thread = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![thread], full_message)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The first thing the user said in a conversation (`command` false) or
    /// a command thread, as its title.
    pub fn first_question(&self, id: i64, command: bool) -> Result<String> {
        let db = self.db.lock().unwrap();
        let sql = if command {
            "SELECT text FROM commands WHERE thread = ?1 AND role = 'user' ORDER BY id LIMIT 1"
        } else {
            "SELECT text FROM messages WHERE conversation = ?1 AND role = 'user' ORDER BY id LIMIT 1"
        };
        Ok(db.query_row(sql, params![id], |r| r.get(0)).optional()?.unwrap_or_default())
    }

    /// The session and conversation of a message.
    pub fn message_place(&self, id: i64) -> Result<Option<(String, i64)>> {
        let db = self.db.lock().unwrap();
        Ok(db
            .query_row(
                "SELECT c.session, c.id FROM messages m JOIN conversations c ON c.id = m.conversation WHERE m.id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// All messages for search: id, session, role, text, time (local).
    pub fn all_messages(&self) -> Result<Vec<(i64, String, String, String, String)>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT m.id, c.session, m.role, m.text, strftime('%Y-%m-%d %H:%M', m.created, 'localtime')
             FROM messages m JOIN conversations c ON c.id = m.conversation ORDER BY m.id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn embedding_hashes(&self) -> Result<std::collections::HashMap<String, String>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT key, hash FROM embeddings")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn put_embedding(&self, key: &str, hash: &str, vector: &[f32]) -> Result<()> {
        let bytes: Vec<u8> = vector.iter().flat_map(|x| x.to_le_bytes()).collect();
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO embeddings (key, hash, vector) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET hash = excluded.hash, vector = excluded.vector",
            params![key, hash, bytes],
        )?;
        Ok(())
    }

    pub fn embeddings(&self) -> Result<std::collections::HashMap<String, Vec<f32>>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT key, vector FROM embeddings")?;
        let rows = stmt.query_map([], |r| {
            let bytes: Vec<u8> = r.get(1)?;
            let vector = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            Ok((r.get::<_, String>(0)?, vector))
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
