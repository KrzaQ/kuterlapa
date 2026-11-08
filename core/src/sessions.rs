//! Sessions: named contexts for work, one folder each in
//! ~/.config/kuterlapa/sessions/<name>/:
//!
//!   context.md     what the session is about; the model reads it
//!   session.toml   optional settings, for example the project folder:
//!                    folder = "~/code/my-project"
//!
//! The current session's name is kept in ~/.local/state/kuterlapa/session.
//! Without one, the core uses "general".

use crate::paths;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

pub const DEFAULT: &str = "general";

#[derive(Debug, Clone)]
pub struct Session {
    pub name: String,
    pub context: String,
    /// The project folder; engines run there, so they see its CLAUDE.md.
    pub folder: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct SessionFile {
    folder: Option<String>,
}

fn dir(name: &str) -> PathBuf {
    paths::config_dir().join("sessions").join(name)
}

fn state_file() -> PathBuf {
    paths::state_dir().join("session")
}

pub fn current_name() -> String {
    let name = fs::read_to_string(state_file()).unwrap_or_default().trim().to_string();
    if name.is_empty() { DEFAULT.to_string() } else { name }
}

pub fn current() -> Session {
    load(&current_name())
}

pub fn load(name: &str) -> Session {
    let dir = dir(name);
    let context = fs::read_to_string(dir.join("context.md")).unwrap_or_default();
    let settings: SessionFile = fs::read_to_string(dir.join("session.toml"))
        .ok()
        .and_then(|t| toml::from_str(&t).map_err(|e| eprintln!("session {name}: {e}")).ok())
        .unwrap_or_default();
    Session {
        name: name.to_string(),
        context,
        folder: settings.folder.map(|f| paths::expand(&f)).filter(|p| p.is_dir()),
    }
}
