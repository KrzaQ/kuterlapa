//! Engines: programs that run a language model for a profile. Each call is
//! one request without tools: a system prompt, a user message, an answer.

use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

pub fn complete(profile: &Profile, system: &str, user: &str) -> Result<String> {
    let mut command = match profile.engine.as_str() {
        "claude" => claude(profile, system, user),
        "pi" => pi(profile, system, user),
        other => bail!("unknown engine {other:?} in profile {:?}", profile.name),
    };
    // Run outside any project, so no project's CLAUDE.md or AGENTS.md
    // changes the answer.
    command.current_dir(paths::state_dir()).stdin(Stdio::null());
    let output = command.output().with_context(|| format!("starting {}", profile.engine))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {}", profile.engine, error.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn claude(profile: &Profile, system: &str, user: &str) -> Command {
    let mut c = Command::new("claude");
    c.args(["-p", "--model", &profile.model, "--tools", "", "--no-session-persistence"]);
    c.args(["--system-prompt", system]);
    if let Some(effort) = &profile.effort {
        c.args(["--effort", effort]);
    }
    c.arg(user);
    c
}

fn pi(profile: &Profile, system: &str, user: &str) -> Command {
    let mut c = Command::new("pi");
    c.args(["-p", "--no-session", "--no-tools", "--no-context-files", "--model", &profile.model]);
    c.args(["--system-prompt", system]);
    c.args(["--thinking", profile.effort.as_deref().unwrap_or("off")]);
    c.arg(user);
    c
}
