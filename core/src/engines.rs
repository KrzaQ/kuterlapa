//! Engines: programs that run a language model for a profile. Each call is
//! one request: a system prompt, a user message, an answer. With a session,
//! the model also gets the tools (tools.rs) through `kuterlapa mcp`: Claude
//! with --mcp-config, pi with a small extension (pi-tools.ts). Claude also
//! gets web search.

use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// A model request that is running, so that it can be cancelled.
struct Running {
    pid: u32,
    cancelled: bool,
}

static RUNNING: Mutex<Vec<Running>> = Mutex::new(Vec::new());

/// The error of a request the user cancelled.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Stops every running model request at once: the engine, its tool server
/// and the tools it runs, so nothing more happens after the user cancels.
/// The process group gets SIGTERM, and SIGKILL a second later if any of it
/// is still there.
pub fn cancel_all() {
    let groups: Vec<u32> = {
        let mut running = RUNNING.lock().unwrap();
        running.iter_mut().for_each(|r| r.cancelled = true);
        running.iter().map(|r| r.pid).collect()
    };
    for pid in groups {
        eprintln!("engine: cancelling process group {pid}");
        signal_group(pid, "TERM");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(1));
            signal_group(pid, "KILL");
        });
    }
}

fn signal_group(pid: u32, signal: &str) {
    let _ = Command::new("/bin/kill")
        .args([format!("-{signal}"), "--".into(), format!("-{pid}")])
        .stderr(Stdio::null())
        .status();
}

/// True if the error is a cancelled request.
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Cancelled>().is_some()
}

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
        "pi" => pi(profile, system, user, folder.is_some(), tools),
        other => bail!("unknown engine {other:?} in profile {:?}", profile.name),
    };
    let state = paths::state_dir();
    command.current_dir(folder.unwrap_or(&state)).stdin(Stdio::null());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    // Its own process group: cancelling stops the engine together with its
    // tool server and any tool it runs.
    command.process_group(0);
    let child = command.spawn().with_context(|| format!("starting {}", profile.engine))?;
    let pid = child.id();
    RUNNING.lock().unwrap().push(Running { pid, cancelled: false });
    let output = child.wait_with_output();
    let cancelled = {
        let mut running = RUNNING.lock().unwrap();
        let cancelled = running.iter().any(|r| r.pid == pid && r.cancelled);
        running.retain(|r| r.pid != pid);
        cancelled
    };
    if cancelled {
        bail!(Cancelled);
    }
    let output = output.with_context(|| format!("waiting for {}", profile.engine))?;
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
    match tools.and_then(|session| mcp_config(session, profile.local)) {
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

fn pi(profile: &Profile, system: &str, user: &str, project: bool, tools: Option<&str>) -> Command {
    let mut c = Command::new("pi");
    c.args(["-p", "--no-session", "--model", &profile.model]);
    let extension = tools.and_then(|_| pi_extension());
    match (tools, extension, std::env::current_exe()) {
        // Only the Kuterlapa tools: no shell, no files. The extension
        // registers the server for this run only; pi's own settings stay as
        // they are, so its extensions (for example a permission classifier)
        // still judge each call.
        (Some(session), Some(extension), Ok(me)) => {
            c.arg("-e").arg(extension);
            c.args(["--tools", "mcp__kuterlapa__*"]);
            c.env("KUTERLAPA_EXE", me).env("KUTERLAPA_SESSION", session);
            if profile.local {
                c.env("KUTERLAPA_LOCAL", "1");
            }
        }
        _ => {
            c.arg("--no-tools");
        }
    }
    if project {
        c.args(["--append-system-prompt", system]);
    } else {
        c.args(["--no-context-files", "--system-prompt", system]);
    }
    c.args(["--thinking", profile.effort.as_deref().unwrap_or("off")]);
    c.arg(user);
    c
}

/// Writes the pi extension (pi-tools.ts) to the state folder, if it is not
/// there yet or changed. Returns its path.
fn pi_extension() -> Option<std::path::PathBuf> {
    const SOURCE: &str = include_str!("pi-tools.ts");
    let path = paths::state_dir().join("pi-tools.ts");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(SOURCE) {
        if let Err(e) = std::fs::write(&path, SOURCE) {
            eprintln!("pi: cannot write {}: {e}", path.display());
            return None;
        }
    }
    Some(path)
}

/// The MCP configuration that starts this program as the tool server.
fn mcp_config(session: &str, local: bool) -> Option<String> {
    let me = std::env::current_exe().ok()?;
    let mut args = vec!["mcp", "--session", session];
    if local {
        args.push("--local");
    }
    let server = json!({ "command": me, "args": args });
    Some(json!({ "mcpServers": { "kuterlapa": server } }).to_string())
}
