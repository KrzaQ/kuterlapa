//! Model profiles: which engine and model answer, in
//! ~/.config/kuterlapa/profiles.json. One button cycles through them; the
//! current one is remembered in ~/.local/state/kuterlapa/profile.

use crate::paths;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Shown in the overlay.
    pub name: String,
    /// "claude" (Claude Code, `claude -p`) or "pi".
    pub engine: String,
    /// Passed to the engine's --model.
    pub model: String,
    /// Claude Code's --effort (low, medium, high), or pi's --thinking.
    #[serde(default)]
    pub effort: Option<String>,
}

fn default_profiles() -> Vec<Profile> {
    vec![Profile { name: "Claude".into(), engine: "claude".into(), model: "sonnet".into(), effort: None }]
}

pub fn load() -> Vec<Profile> {
    let path = paths::config_dir().join("profiles.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return default_profiles();
    };
    match serde_json::from_str::<Vec<Profile>>(&text) {
        Ok(list) if !list.is_empty() => list,
        Ok(_) => default_profiles(),
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            default_profiles()
        }
    }
}

fn state_file() -> std::path::PathBuf {
    paths::state_dir().join("profile")
}

/// The current profile, by the name saved last time; the first one if the
/// name is gone.
pub fn current(list: &[Profile]) -> usize {
    let saved = fs::read_to_string(state_file()).unwrap_or_default();
    list.iter().position(|p| p.name == saved.trim()).unwrap_or(0)
}

pub fn save(profile: &Profile) {
    let _ = fs::write(state_file(), &profile.name);
}
