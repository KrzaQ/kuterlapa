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
CREATE TABLE IF NOT EXISTS usage (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL DEFAULT (datetime('now')),
    session TEXT,
    profile TEXT,
    purpose TEXT NOT NULL,       -- 'chat' or 'draft': what the request was for
    role TEXT NOT NULL,          -- 'model' or 'classifier' (pi-censor)
    engine TEXT NOT NULL,        -- 'claude' or 'pi'
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,      -- read anew, not from the cache
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    cache_write_1h INTEGER,
    output INTEGER NOT NULL,
    thinking INTEGER,            -- part of output, where the engine counts it
    web_searches INTEGER,
    cost REAL,                   -- US dollars, as the engine prices it
    ms INTEGER
);
CREATE INDEX IF NOT EXISTS usage_by_time ON usage(at);
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
    /// What the model did for this answer (JSON), if it was saved.
    pub work: Option<String>,
}

fn full_message(r: &rusqlite::Row) -> rusqlite::Result<FullMessage> {
    Ok(FullMessage {
        id: r.get(0)?,
        role: r.get(1)?,
        text: r.get(2)?,
        profile: r.get(3)?,
        created: r.get(4)?,
        work: r.get(5)?,
    })
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
        // Added later: what the model did for an answer (JSON: tokens,
        // seconds, tool calls), for the chat view.
        for table in ["messages", "commands"] {
            let has: bool = db
                .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = 'work'"))?
                .exists([])?;
            if !has {
                db.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN work TEXT"))?;
            }
        }
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

    pub fn add_message(
        &self,
        conversation: i64,
        role: &str,
        text: &str,
        profile: Option<&str>,
        work: Option<&str>,
    ) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO messages (conversation, role, text, profile, work) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![conversation, role, text, profile, work],
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
            "SELECT id, role, text, profile, strftime('%Y-%m-%d %H:%M', created, 'localtime'), work FROM (
                 SELECT id, role, text, profile, created, work FROM messages WHERE conversation = ?1
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
            "SELECT id, role, text, profile, strftime('%Y-%m-%d %H:%M', created, 'localtime'), work
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
        work: Option<&str>,
    ) -> Result<i64> {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO commands (thread, context_session, role, text, profile, work)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![thread.unwrap_or(0), session, role, text, profile, work],
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

/// One model's use in one request, for the `usage` table.
pub struct UsageRow<'a> {
    pub session: &'a str,
    pub profile: &'a str,
    pub purpose: &'a str,
    pub engine: &'a str,
    pub call: &'a crate::activity::Call,
}

/// The sums of one group (purpose, role, provider, model) in a period.
#[derive(Debug, Clone, Default)]
pub struct UsageSum {
    pub purpose: String,
    pub role: String,
    pub provider: String,
    pub model: String,
    pub calls: u64,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    /// None when no call in the group counted it.
    pub thinking: Option<u64>,
    pub web_searches: Option<u64>,
    pub cost: Option<f64>,
    pub ms: Option<u64>,
}

impl Store {
    pub fn add_usage(&self, row: &UsageRow) -> Result<()> {
        let c = row.call;
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO usage (session, profile, purpose, role, engine, provider, model, input, cache_read,
                                cache_write, cache_write_1h, output, thinking, web_searches, cost, ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                row.session, row.profile, row.purpose, c.role, row.engine, c.provider, c.model,
                c.input as i64, c.cache_read as i64, c.cache_write as i64, c.cache_write_1h.map(|n| n as i64),
                c.output as i64, c.thinking.map(|n| n as i64), c.web_searches.map(|n| n as i64), c.cost,
                c.ms.map(|n| n as i64)
            ],
        )?;
        Ok(())
    }

    /// Sums since a time, most tokens first. `since` is a fixed SQLite
    /// expression from usage.rs (`datetime('now', '-7 days')`); None: all.
    pub fn usage_sums(&self, since: Option<&'static str>) -> Result<Vec<UsageSum>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(&format!(
            "SELECT purpose, role, provider, model, COUNT(*), SUM(input), SUM(cache_read), SUM(cache_write),
                    SUM(output), SUM(thinking), SUM(web_searches), SUM(cost), SUM(ms)
             FROM usage WHERE at >= {} GROUP BY purpose, role, provider, model
             ORDER BY SUM(input + cache_read + cache_write + output) DESC",
            since.unwrap_or("''")
        ))?;
        let rows = stmt.query_map([], |r| {
            let n = |i: usize| -> rusqlite::Result<u64> { Ok(r.get::<_, Option<i64>>(i)?.unwrap_or(0) as u64) };
            let opt = |i: usize| -> rusqlite::Result<Option<u64>> { Ok(r.get::<_, Option<i64>>(i)?.map(|v| v as u64)) };
            Ok(UsageSum {
                purpose: r.get(0)?,
                role: r.get(1)?,
                provider: r.get(2)?,
                model: r.get(3)?,
                calls: n(4)?,
                input: n(5)?,
                cache_read: n(6)?,
                cache_write: n(7)?,
                output: n(8)?,
                thinking: opt(9)?,
                web_searches: opt(10)?,
                cost: r.get(11)?,
                ms: opt(12)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
