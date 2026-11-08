//! Sessions: named contexts for work, one folder each in
//! ~/.config/kuterlapa/sessions/<name>/:
//!
//!   context.md      what the session is about; the model reads it
//!   dictionary.txt  words for speech recognition in this session
//!   memory/         the models' notes about this session's topic
//!   session.toml    optional settings:
//!                     folder = "~/code/my-project"   # the project folder
//!                     # The window in front picks this session when the
//!                     # session is chosen automatically:
//!                     apps = ["com.googlecode.iterm2"]  # bundle ids or names
//!                     titles = ["bill", "rent"]         # parts of the title
//!
//! The current session's name is kept in ~/.local/state/kuterlapa/session.
//! Without one, the core uses "general". How it is chosen is in
//! session-mode: "sticky" (the default) keeps it until the user switches;
//! "auto" picks it from the window in front each time a recording starts.

use crate::{paths, shell};
use anyhow::{bail, Result};
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
    apps: Vec<String>,
    titles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct SessionFile {
    folder: Option<String>,
    apps: Vec<String>,
    titles: Vec<String>,
}

fn sessions_dir() -> PathBuf {
    paths::config_dir().join("sessions")
}

fn dir(name: &str) -> PathBuf {
    sessions_dir().join(name)
}

fn state_file() -> PathBuf {
    paths::state_dir().join("session")
}

fn mode_file() -> PathBuf {
    paths::state_dir().join("session-mode")
}

pub fn current_name() -> String {
    let name = fs::read_to_string(state_file()).unwrap_or_default().trim().to_string();
    if name.is_empty() { DEFAULT.to_string() } else { name }
}

pub fn current() -> Session {
    load(&current_name())
}

/// True when the window in front picks the session.
pub fn automatic() -> bool {
    fs::read_to_string(mode_file()).unwrap_or_default().trim() == "auto"
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
        apps: settings.apps,
        titles: settings.titles,
    }
}

/// All sessions, by name.
pub fn all() -> Vec<Session> {
    let mut names: Vec<String> = fs::read_dir(sessions_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if !names.iter().any(|n| n == DEFAULT) {
        names.push(DEFAULT.to_string());
    }
    names.sort();
    names.iter().map(|n| load(n)).collect()
}

/// A name for a session folder: lowercase letters, digits, - and _.
fn clean(name: &str) -> Result<String> {
    let name: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() || name.len() > 40 {
        bail!("a session name needs 1 to 40 letters, digits, - or _");
    }
    Ok(name)
}

/// Makes `name` the current session. `sticky`: keep it until the user
/// switches; otherwise the window in front picks sessions from now on.
pub fn switch(name: &str, sticky: bool) -> Result<String> {
    let name = clean(name)?;
    if name != DEFAULT && !dir(&name).is_dir() {
        let known: Vec<String> = all().into_iter().map(|s| s.name).collect();
        bail!("no session {name}; sessions: {}", known.join(", "));
    }
    fs::create_dir_all(paths::state_dir())?;
    fs::write(state_file(), format!("{name}\n"))?;
    fs::write(mode_file(), if sticky { "sticky\n" } else { "auto\n" })?;
    Ok(name)
}

/// Lets the window in front pick the session from now on.
pub fn set_automatic() -> Result<()> {
    fs::create_dir_all(paths::state_dir())?;
    fs::write(mode_file(), "auto\n")?;
    Ok(())
}

/// Creates a session folder with its context and rules.
pub fn create(name: &str, context: &str, folder: Option<&str>, apps: &[String], titles: &[String]) -> Result<String> {
    let name = clean(name)?;
    let dir = dir(&name);
    if dir.exists() {
        bail!("session {name} exists already");
    }
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("context.md"), format!("{}\n", context.trim()))?;
    let mut toml = String::new();
    if let Some(folder) = folder.filter(|f| !f.trim().is_empty()) {
        toml.push_str(&format!("folder = {:?}\n", folder.trim()));
    }
    if !apps.is_empty() {
        toml.push_str(&format!("apps = {apps:?}\n"));
    }
    if !titles.is_empty() {
        toml.push_str(&format!("titles = {titles:?}\n"));
    }
    if !toml.is_empty() {
        fs::write(dir.join("session.toml"), toml)?;
    }
    Ok(name)
}

impl Session {
    /// How well this session fits a window: 0 when it does not, more for
    /// each kind of rule that fits.
    fn score(&self, bundle: &str, app: &str, title: &str) -> u32 {
        if self.apps.is_empty() && self.titles.is_empty() {
            return 0;
        }
        let lower = title.to_lowercase();
        let app_fits = self.apps.iter().any(|a| a.eq_ignore_ascii_case(bundle) || a.eq_ignore_ascii_case(app));
        let title_fits = self.titles.iter().any(|t| lower.contains(&t.to_lowercase()));
        if (!self.apps.is_empty() && !app_fits) || (!self.titles.is_empty() && !title_fits) {
            return 0;
        }
        u32::from(app_fits) + u32::from(title_fits)
    }

    /// The rules, for a list of sessions.
    pub fn rules(&self) -> String {
        let mut parts = Vec::new();
        if !self.apps.is_empty() {
            parts.push(format!("apps {}", self.apps.join(", ")));
        }
        if !self.titles.is_empty() {
            parts.push(format!("titles {}", self.titles.join(", ")));
        }
        parts.join("; ")
    }
}

/// When sessions are automatic, picks the session for the window in front
/// and makes it current: the best fit, or "general" when none fits.
/// Returns the new name when it changed.
pub fn follow_window() -> Option<String> {
    if !automatic() {
        return None;
    }
    let window = shell::send("window");
    if window.starts_with("error") {
        return None;
    }
    let mut parts = window.splitn(3, '\t');
    let (bundle, app, title) = (parts.next()?, parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let best = all()
        .into_iter()
        .map(|s| (s.score(bundle, app, title), s.name))
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score)
        .map(|(_, name)| name)
        .unwrap_or_else(|| DEFAULT.to_string());
    if best == current_name() {
        return None;
    }
    let _ = fs::write(state_file(), format!("{best}\n"));
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(apps: &[&str], titles: &[&str]) -> Session {
        Session {
            name: "s".into(),
            context: String::new(),
            folder: None,
            apps: apps.iter().map(|s| s.to_string()).collect(),
            titles: titles.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn scores() {
        let iterm = session(&["com.googlecode.iterm2"], &[]);
        assert_eq!(iterm.score("com.googlecode.iterm2", "iTerm2", "zsh"), 1);
        assert_eq!(iterm.score("com.google.Chrome", "Google Chrome", "zsh"), 0);
        let bills = session(&["Google Chrome"], &["bill"]);
        assert_eq!(bills.score("com.google.Chrome", "Google Chrome", "Bills - Sheets"), 2);
        assert_eq!(bills.score("com.google.Chrome", "Google Chrome", "News"), 0);
        assert_eq!(session(&[], &[]).score("a", "b", "c"), 0);
    }
}
