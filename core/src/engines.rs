//! Engines: programs that run a language model for a profile. Each call is
//! one request without tools: a system prompt, a user message, an answer.

use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn complete(profile: &Profile, system: &str, user: &str) -> Result<String> {
    complete_in(profile, system, user, None)
}

/// Like `complete`, run in `folder`: the engine then reads that project's
/// CLAUDE.md or AGENTS.md. Without a folder it runs outside any project, so
/// no project changes the answer.
pub fn complete_in(profile: &Profile, system: &str, user: &str, folder: Option<&Path>) -> Result<String> {
    let mut command = match profile.engine.as_str() {
        "claude" => claude(profile, system, user, folder.is_some()),
        "pi" => pi(profile, system, user, folder.is_some()),
        other => bail!("unknown engine {other:?} in profile {:?}", profile.name),
    };
    let state = paths::state_dir();
    command.current_dir(folder.unwrap_or(&state)).stdin(Stdio::null());
    let output = command.output().with_context(|| format!("starting {}", profile.engine))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {}", profile.engine, error.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `project`: keep Claude Code's own system prompt and the project's
/// CLAUDE.md, and add ours; otherwise ours replaces it.
fn claude(profile: &Profile, system: &str, user: &str, project: bool) -> Command {
    let mut c = Command::new("claude");
    c.args(["-p", "--model", &profile.model, "--tools", "", "--no-session-persistence"]);
    c.args([if project { "--append-system-prompt" } else { "--system-prompt" }, system]);
    if let Some(effort) = &profile.effort {
        c.args(["--effort", effort]);
    }
    c.arg(user);
    c
}

fn pi(profile: &Profile, system: &str, user: &str, project: bool) -> Command {
    let mut c = Command::new("pi");
    c.args(["-p", "--no-session", "--no-tools", "--model", &profile.model]);
    if project {
        c.args(["--append-system-prompt", system]);
    } else {
        c.args(["--no-context-files", "--system-prompt", system]);
    }
    c.args(["--thinking", profile.effort.as_deref().unwrap_or("off")]);
    c.arg(user);
    c
}
