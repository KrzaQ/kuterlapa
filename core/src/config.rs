//! Settings in ~/.config/kuterlapa/config.toml. Every key is optional.
//!
//!   [speech]
//!   # The speech provider: a program that speaks the speech protocol
//!   # (docs/protocol.md). Empty disables speech.
//!   command = ["~/.local/bin/kuterlapa-speech"]
//!
//!   [voice]
//!   # The voice provider, which reads text aloud. Empty disables it.
//!   command = ["~/.local/bin/kuterlapa-voice"]
//!
//!   [embed]
//!   # The embedding provider, for search by meaning. Empty disables it.
//!   command = ["~/.local/bin/kuterlapa-embed"]
//!
//!   [hooks]
//!   # Programs to run when Kuterlapa starts or stops listening, thinking
//!   # or speaking (see hooks.rs). Empty: none.
//!   commands = []

use crate::paths;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub speech: Speech,
    pub voice: Voice,
    pub embed: Embed,
    pub hooks: Hooks,
}

/// The embedding provider, for search by meaning. Empty disables it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Embed {
    pub command: Vec<String>,
}

impl Default for Embed {
    fn default() -> Self {
        Embed { command: vec!["~/.local/bin/kuterlapa-embed".into()] }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Hooks {
    pub commands: Vec<String>,
}

/// The voice provider, which reads text aloud. Empty disables the voice.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Voice {
    pub command: Vec<String>,
}

impl Default for Voice {
    fn default() -> Self {
        Voice { command: vec!["~/.local/bin/kuterlapa-voice".into()] }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Speech {
    pub command: Vec<String>,
}

impl Default for Speech {
    fn default() -> Self {
        Speech { command: vec!["~/.local/bin/kuterlapa-speech".into()] }
    }
}

pub fn load() -> Result<Config> {
    let path = paths::config_dir().join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}
