//! Exports conversations to Markdown files, one per conversation, in
//! ~/Documents/Kuterlapa/<session>/<date>-rozmowa-<id>.md. An export again
//! replaces the file, so it always holds the whole conversation.
//!
//!   kuterlapa export [session] [--all]
//!
//! Without a session, the current one; without --all, its newest
//! conversation. Models use the conversation_export tool (mcp.rs).

use crate::paths;
use crate::sessions;
use crate::store::Store;
use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;

fn folder(session: &str) -> PathBuf {
    paths::home().join("Documents").join("Kuterlapa").join(session)
}

/// Writes one conversation; returns the file.
fn write(store: &Store, session: &sessions::Session, id: i64, created: &str) -> Result<PathBuf> {
    let messages = store.messages_full(id, usize::MAX >> 1)?;
    let mut md = format!("# {} · rozmowa {id}\n\n_Od {created}_\n", session.label);
    for (role, text, profile, at) in messages {
        let who = if role == "user" { "Ty".to_string() } else { profile.unwrap_or_else(|| "Model".into()) };
        md.push_str(&format!("\n---\n\n**{who}** · {at}\n\n{}\n", text.trim()));
    }
    let dir = folder(&session.name);
    fs::create_dir_all(&dir)?;
    let day = created.split(' ').next().unwrap_or("");
    let path = dir.join(format!("{day}-rozmowa-{id}.md"));
    fs::write(&path, md)?;
    Ok(path)
}

/// Exports the newest conversation of a session, or all of them. Returns
/// the files written.
pub fn export(store: &Store, session: Option<&str>, all: bool) -> Result<Vec<PathBuf>> {
    let name = match session {
        Some(name) if !name.trim().is_empty() => sessions::slug(name)?,
        _ => sessions::current_name(),
    };
    let session = sessions::load(&name);
    let conversations = store.conversations(&name)?;
    if conversations.is_empty() {
        bail!("session {name} has no conversations");
    }
    let chosen: Vec<_> = if all { conversations } else { conversations.into_iter().last().into_iter().collect() };
    chosen.iter().map(|(id, created)| write(store, &session, *id, created)).collect()
}

/// `kuterlapa export [session] [--all]`
pub fn run(args: &[String]) -> Result<()> {
    let all = args.iter().any(|a| a == "--all");
    let session = args.iter().find(|a| !a.starts_with("--")).map(String::as_str);
    let store = Store::open()?;
    for path in export(&store, session, all)? {
        println!("{}", path.display());
    }
    Ok(())
}
