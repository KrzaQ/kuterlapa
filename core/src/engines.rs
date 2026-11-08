//! Engines: programs that run a language model for a profile. Each call is
//! one request: a system prompt, a user message, an answer. With a session,
//! Claude also gets the tools (tools.rs, through `kuterlapa mcp`) and web
//! search.

use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::Path;
use std::process::{Command, Stdio};

/// One request without tools.
pub fn complete(profile: &Profile, system: &str, user: &str) -> Result<String> {
    complete_in(profile, system, user, None, None)
}

/// Like `complete`, run in `folder`: the engine then reads that project's
/// CLAUDE.md or AGENTS.md. Without a folder it runs outside any project, so
/// no project changes the answer. With `tools`, the model may use the
/// global tools and those of the named session.
pub fn complete_in(
    profile: &Profile,
    system: &str,
    user: &str,
    folder: Option<&Path>,
    tools: Option<&str>,
) -> Result<String> {
    let mut command = match profile.engine.as_str() {
        "claude" => claude(profile, system, user, folder.is_some(), tools),
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
fn claude(profile: &Profile, system: &str, user: &str, project: bool, tools: Option<&str>) -> Command {
    let mut c = Command::new("claude");
    c.args(["-p", "--model", &profile.model, "--no-session-persistence"]);
    match tools.and_then(|session| mcp_config(session)) {
        // Claude Code's own tools stay off (no shell, no files) except web
        // search. Its permission classifier ("auto") judges each tool call;
        // tools that must ask the user do it in `kuterlapa mcp`.
        Some(config) => {
            c.args(["--tools", "WebSearch,WebFetch", "--mcp-config", &config, "--strict-mcp-config"]);
            c.args(["--permission-mode", "auto"]);
        }
        None => {
            c.args(["--tools", ""]);
        }
    }
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

/// The MCP configuration that starts this program as the tool server.
fn mcp_config(session: &str) -> Option<String> {
    let me = std::env::current_exe().ok()?;
    let server = json!({ "command": me, "args": ["mcp", "--session", session] });
    Some(json!({ "mcpServers": { "kuterlapa": server } }).to_string())
}
