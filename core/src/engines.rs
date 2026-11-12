//! Engines: programs that run a language model for a profile. Each call is
//! one request: a system prompt, a user message, an answer. pi runs in its
//! RPC mode, so that its extensions can ask the user (pi-censor asks before
//! a call it is unsure of): the overlay shows such a question. With a session,
//! the model also gets the tools (tools.rs) through `kuterlapa mcp`: Claude
//! with --mcp-config, pi with a small extension (pi-tools.ts). Claude also
//! gets web search.

use crate::activity::Activity;
use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
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

/// The tools a request may use: those of a session, or only the named
/// ones (for a model that only reads, such as the draft's).
#[derive(Clone, Copy)]
pub struct ToolSet<'a> {
    pub session: &'a str,
    pub only: Option<&'a [&'a str]>,
}

/// One request for the draft: only the named Kuterlapa tools (which only
/// read), no shell, no web.
pub fn complete_reading(profile: &Profile, system: &str, user: &str, session: &str, only: &[&str]) -> Result<String> {
    run(profile, system, user, None, Some(ToolSet { session, only: Some(only) }), &|_| {}, "draft")
}

/// Like `complete`, run in `folder`: the engine then reads that project's
/// CLAUDE.md or AGENTS.md. Without a folder it runs outside any project, so
/// no project changes the answer. With `tools`, the model may use the
/// global tools and those of the named session.
///
/// The engine writes its work as a stream of events. `progress` gets what
/// it does while it runs (thinking, tool calls, tokens), at most ten times
/// a second, and once more at the end.
pub fn complete_in(
    profile: &Profile,
    system: &str,
    user: &str,
    folder: Option<&Path>,
    tools: Option<&str>,
    progress: &dyn Fn(&Activity),
) -> Result<String> {
    let tools = tools.map(|session| ToolSet { session, only: None });
    run(profile, system, user, folder, tools, progress, "chat")
}

/// `purpose` goes to the token statistics: "chat" or "draft".
fn run(
    profile: &Profile,
    system: &str,
    user: &str,
    folder: Option<&Path>,
    tools: Option<ToolSet>,
    progress: &dyn Fn(&Activity),
    purpose: &str,
) -> Result<String> {
    let mut command = match profile.engine.as_str() {
        "claude" => claude(profile, system, user, folder.is_some(), tools),
        "pi" => pi(profile, system, folder.is_some(), tools),
        other => bail!("unknown engine {other:?} in profile {:?}", profile.name),
    };
    let state = paths::state_dir();
    let rpc = profile.engine == "pi";
    command.current_dir(folder.unwrap_or(&state)).stdin(if rpc { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    // Its own process group: cancelling stops the engine together with its
    // tool server and any tool it runs.
    command.process_group(0);
    let mut child = command.spawn().with_context(|| format!("starting {}", profile.engine))?;
    let pid = child.id();
    RUNNING.lock().unwrap().push(Running { pid, cancelled: false });
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    // pi in RPC mode: the request goes in as a command, and the input
    // stays open for answers to its extensions' questions.
    let mut input = child.stdin.take();
    if let Some(stdin) = input.as_mut() {
        let prompt = json!({ "id": "prompt", "type": "prompt", "message": user });
        writeln!(stdin, "{prompt}").context("sending the request to pi")?;
    }
    let mut activity = Activity::default();
    let mut shown = std::time::Instant::now();
    let mut steps = 0;
    for line in BufReader::new(child.stdout.take().expect("piped stdout")).lines() {
        let Ok(line) = line else { break };
        if rpc && rpc_record(&line, &mut input, &mut activity) {
            progress(&activity);
        }
        if activity.line(&profile.engine, &line)
            && (shown.elapsed() >= Duration::from_millis(100) || activity.steps.len() != steps)
        {
            progress(&activity);
            shown = std::time::Instant::now();
            steps = activity.steps.len();
        }
    }
    activity.finished = Some(activity.started.elapsed());
    progress(&activity);
    // Saved even when cancelled: the tokens were used.
    let session = tools.map(|t| t.session.to_string()).unwrap_or_else(crate::sessions::current_name);
    crate::usage::record(profile, &session, purpose, &activity.calls);
    let status = child.wait();
    let errors = errors.join().unwrap_or_default();
    let cancelled = {
        let mut running = RUNNING.lock().unwrap();
        let cancelled = running.iter().any(|r| r.pid == pid && r.cancelled);
        running.retain(|r| r.pid != pid);
        cancelled
    };
    if cancelled {
        bail!(Cancelled);
    }
    let status = status.with_context(|| format!("waiting for {}", profile.engine))?;
    if let Some(error) = activity.error {
        bail!("{} failed: {error}", profile.engine);
    }
    if !status.success() {
        bail!("{} failed: {}", profile.engine, errors.trim());
    }
    activity.answer.with_context(|| format!("{} gave no answer: {}", profile.engine, errors.trim()))
}

/// `project`: keep Claude Code's own system prompt and the project's
/// CLAUDE.md, and add ours; otherwise ours replaces it.
fn claude(profile: &Profile, system: &str, user: &str, project: bool, tools: Option<ToolSet>) -> Command {
    let mut c = Command::new("claude");
    c.args(["-p", "--model", &profile.model, "--no-session-persistence"]);
    // Events while it works; the thinking as a summary (Claude Code leaves
    // it out by default).
    c.args(["--output-format", "stream-json", "--verbose", "--include-partial-messages"]);
    c.args(["--thinking-display", "summarized"]);
    match tools.and_then(|t| mcp_config(t, profile.local).map(|config| (config, t.only.is_some()))) {
        // Claude Code's own tools stay off (no files) except web search, and
        // its shell when the profile allows it. Its permission classifier
        // ("auto") judges each tool call; tools that must ask the user do it
        // in `kuterlapa mcp`.
        Some((config, reading)) => {
            let builtin = match (reading, profile.shell) {
                (true, _) => "",
                (false, true) => "WebSearch,WebFetch,Bash",
                (false, false) => "WebSearch,WebFetch",
            };
            c.args(["--tools", builtin, "--mcp-config", &config, "--strict-mcp-config"]);
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

/// Handles what only pi's RPC mode sends: answers to commands, the end of
/// the work, and questions from its extensions. Closing the input ends pi.
/// Returns true after a question, so the overlay is drawn again.
fn rpc_record(line: &str, input: &mut Option<std::process::ChildStdin>, activity: &mut Activity) -> bool {
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    match record["type"].as_str() {
        // pi's own figure of how full the context is; then pi may quit.
        Some("response") if record["command"] == "get_session_stats" => {
            let usage = &record["data"]["contextUsage"];
            activity.set_context(usage["tokens"].as_u64(), usage["contextWindow"].as_u64());
            input.take();
            true
        }
        Some("response") if record["success"] == false => {
            activity.error = Some(record["error"].as_str().unwrap_or("pi refused the request").to_string());
            input.take();
            false
        }
        // The prompt did not start any work (a command, for example).
        Some("response") if record["command"] == "prompt" && record["data"]["disposition"] == "handled" => {
            input.take();
            false
        }
        // pi will not go on by itself: the request is done. Ask how full
        // the context is before letting pi quit.
        Some("agent_settled") => {
            match input.as_mut() {
                Some(stdin) => {
                    if writeln!(stdin, "{}", json!({ "id": "stats", "type": "get_session_stats" })).is_err() {
                        input.take();
                    }
                }
                None => {}
            }
            false
        }
        Some("extension_ui_request") => {
            let Some(answer) = dialog(&record) else { return false };
            if let Some(stdin) = input.as_mut() {
                let _ = writeln!(stdin, "{answer}");
            }
            true
        }
        _ => false,
    }
}

/// pi-censor's options, as the overlay shows them. The answer is the
/// option itself.
const LABELS: [(&str, &str); 4] = [
    ("Allow once", "Pozwól raz"),
    ("Allow this exact call for this session", "Pozwól na to samo wywołanie do końca rozmowy"),
    ("Deny", "Odmów"),
    ("Show the whole call", "Pokaż całe wywołanie"),
];

/// Asks the user an extension's question in the overlay (select or
/// confirm) and returns pi's `extension_ui_response`. Other dialogs (text
/// input, an editor) are cancelled: the controller cannot type. Returns
/// None for a record that wants no answer (a notice, a status).
fn dialog(request: &Value) -> Option<Value> {
    let id = request["id"].clone();
    let cancelled = json!({ "type": "extension_ui_response", "id": id, "cancelled": true });
    let method = request["method"].as_str().unwrap_or_default();
    if !matches!(method, "select" | "confirm" | "input" | "editor") {
        return None;
    }
    // A title like "pi-censor: why\n\nthe call": the first part is the
    // question, the rest its details.
    let title = request["title"].as_str().unwrap_or_default();
    let (heading, rest) = title.split_once("\n\n").unwrap_or((title, ""));
    let detail = [rest, request["message"].as_str().unwrap_or_default()]
        .iter()
        .filter(|t| !t.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let mut question = json!({ "kind": "question", "title": "Pytanie z pi", "tool": "",
                               "description": heading, "command": detail });
    match method {
        "select" => {
            let options: Vec<&str> = request["options"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if options.is_empty() || options.len() > crate::approvals::MAX_OPTIONS {
                eprintln!("pi asked with {} options; cancelled", options.len());
                return Some(cancelled);
            }
            let labels: Vec<&str> = options
                .iter()
                .map(|o| LABELS.iter().find(|(from, _)| from == o).map(|(_, to)| *to).unwrap_or(o))
                .collect();
            question["options"] = json!(labels);
            let answer = crate::approvals::ask(&question.to_string(), || {});
            let chosen = match (options.len(), answer.as_str()) {
                (1, "yes") => Some(0),
                (_, a) => a.strip_prefix("chose ").and_then(|a| a.split(':').next()?.parse::<usize>().ok()).map(|n| n - 1),
            };
            Some(match chosen.and_then(|i| options.get(i)) {
                Some(option) => json!({ "type": "extension_ui_response", "id": id, "value": option }),
                None => cancelled,
            })
        }
        "confirm" => {
            let yes = crate::approvals::ask(&question.to_string(), || {}) == "yes";
            Some(json!({ "type": "extension_ui_response", "id": id, "confirmed": yes }))
        }
        _ => {
            eprintln!("pi asked for text ({method}): cancelled");
            Some(cancelled)
        }
    }
}

/// The request itself goes in later, as an RPC command (see run()).
fn pi(profile: &Profile, system: &str, project: bool, tools: Option<ToolSet>) -> Command {
    let mut c = Command::new("pi");
    c.args(["--mode", "rpc", "--no-session", "--model", &profile.model]);
    let extension = tools.and_then(|_| pi_extension());
    match (tools, extension, std::env::current_exe()) {
        // Only the Kuterlapa tools, and the shell when the profile allows
        // it; no file tools. The extension
        // registers the server for this run only; pi's own settings stay as
        // they are, so its extensions (for example a permission classifier)
        // still judge each call.
        (Some(set), Some(extension), Ok(me)) => {
            c.arg("-e").arg(extension);
            match set.only {
                // Only the named tools, and no shell.
                Some(names) => {
                    let list: Vec<String> = names.iter().map(|n| format!("mcp__kuterlapa__{n}")).collect();
                    c.args(["--tools", &list.join(",")]);
                    c.env("KUTERLAPA_ONLY", names.join(","));
                }
                // pi's own bash when the profile allows a shell: pi-censor
                // judges each command with its rules for shells.
                None => {
                    c.args(["--tools", if profile.shell { "bash,mcp__kuterlapa__*" } else { "mcp__kuterlapa__*" }]);
                }
            }
            c.env("KUTERLAPA_EXE", me).env("KUTERLAPA_SESSION", set.session);
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
fn mcp_config(tools: ToolSet, local: bool) -> Option<String> {
    let me = std::env::current_exe().ok()?;
    let only = tools.only.map(|names| names.join(","));
    let mut args = vec!["mcp", "--session", tools.session];
    if local {
        args.push("--local");
    }
    if let Some(only) = &only {
        args.extend(["--only", only.as_str()]);
    }
    let server = json!({ "command": me, "args": args });
    Some(json!({ "mcpServers": { "kuterlapa": server } }).to_string())
}
