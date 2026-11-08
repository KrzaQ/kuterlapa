//! Where Kuterlapa keeps its files. Follows the XDG base directory layout:
//!   config  ~/.config/kuterlapa         settings the user edits
//!   data    ~/.local/share/kuterlapa    recordings, history
//!   state   ~/.local/state/kuterlapa    sockets

use std::env;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

fn base(variable: &str, fallback: &str) -> PathBuf {
    match env::var_os(variable) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
    .join("kuterlapa")
}

pub fn config_dir() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config")
}

pub fn state_dir() -> PathBuf {
    base("XDG_STATE_HOME", ".local/state")
}

/// The core's socket: buttons, dictated text, requests from the CLI.
pub fn core_socket() -> PathBuf {
    state_dir().join("core.sock")
}

/// The platform shell's socket: it types text and presses keys.
pub fn shell_socket() -> PathBuf {
    state_dir().join("shell.sock")
}

/// The speech provider's socket: start and stop recording.
pub fn speech_socket() -> PathBuf {
    state_dir().join("speech.sock")
}

/// The voice provider's socket: read text aloud.
pub fn voice_socket() -> PathBuf {
    state_dir().join("voice.sock")
}

/// Expands a leading "~/" in a path from a settings file.
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}
