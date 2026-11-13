//! Engines: programs that run a language model for a profile. Each call is
//! one request: a system prompt, a user message, an answer. Lejce starts
//! the engine and reads its work; this module turns a profile into a Lejce
//! request. pi's extensions can ask the user (pi-censor asks before a call
//! it is unsure of): the overlay shows such a question. With a session, the
//! model also gets the tools (tools.rs) through `kuterlapa mcp`. Claude
//! also gets web search.

use crate::approvals;
use crate::paths;
use crate::profiles::Profile;
use anyhow::{bail, Result};
use lejce::{Activity, Answer, Engine, Host, McpServer, Permissions, Question, Session, Setup, Tools};
use std::path::Path;

/// Stops every running model request at once: the engine, its tool server
/// and the tools it runs, so nothing more happens after the user cancels.
pub fn cancel_all() {
    lejce::cancel_all();
}

/// True if the error is a cancelled request.
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    matches!(error.downcast_ref::<lejce::Error>(), Some(lejce::Error::Cancelled))
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
    let Some(engine) = Engine::parse(&profile.engine) else {
        bail!("unknown engine {:?} in profile {:?}", profile.engine, profile.name);
    };
    let state = paths::state_dir();
    let setup = Setup {
        engine,
        host: Host::Local { folder: Some(folder.unwrap_or(&state).to_path_buf()) },
        model: profile.model.clone(),
        effort: profile.effort.clone(),
        system: system.into(),
        project: folder.is_some(),
        tools: tools.and_then(|set| tool_list(profile, engine, set)),
        // One message per engine: the program sends the history itself.
        session: Session::None,
        disable_hooks: false,
        files: state,
    };
    let ask = |question: &Question| ask(engine, question);
    let outcome = lejce::run(&setup, user, progress, &ask);
    // Saved even when cancelled: the tokens were used.
    let session = tools.map(|t| t.session.to_string()).unwrap_or_else(crate::sessions::current_name);
    crate::usage::record(profile, &session, purpose, &outcome.activity.calls);
    Ok(outcome.answer?)
}

/// The engine's own tools and this program as the tool server. None if
/// the server cannot be named (no path to this program): then no tools.
fn tool_list(profile: &Profile, engine: Engine, set: ToolSet) -> Option<Tools> {
    let me = std::env::current_exe().ok()?;
    let mut args = vec!["mcp".to_string(), "--session".into(), set.session.into()];
    if profile.local {
        args.push("--local".into());
    }
    if let Some(names) = set.only {
        args.extend(["--only".into(), names.join(",")]);
    }
    let mut server = McpServer::stdio("kuterlapa", &me.to_string_lossy(), args);
    server.description = Some("Kuterlapa tools: the user's devices and programs".into());
    // Only the Kuterlapa tools for a model that only reads. Otherwise web
    // search for Claude, and the engine's shell when the profile allows
    // it: the engine's permission classifier judges each command.
    let builtin: &[&str] = match (engine, set.only.is_some(), profile.shell) {
        (_, true, _) => &[],
        (Engine::Claude, false, true) => &["WebSearch", "WebFetch", "Bash"],
        (Engine::Claude, false, false) => &["WebSearch", "WebFetch"],
        (Engine::Pi, false, true) => &["bash"],
        (Engine::Pi, false, false) => &[],
        // Codex keeps its shell; "shell" lets it write in its folder.
        (Engine::Codex, false, true) => &["shell"],
        (Engine::Codex, false, false) => &[],
    };
    Some(Tools {
        builtin: builtin.iter().map(|s| s.to_string()).collect(),
        servers: vec![server],
        permissions: Permissions::Auto,
    })
}

/// pi-censor's options, as the overlay shows them. The answer is the
/// option itself.
const LABELS: [(&str, &str); 4] = [
    ("Allow once", "Pozwól raz"),
    ("Allow this exact call for this session", "Pozwól na to samo wywołanie do końca rozmowy"),
    ("Deny", "Odmów"),
    ("Show the whole call", "Pokaż całe wywołanie"),
];

/// Asks the user an engine's question in the overlay, with the options in
/// Polish.
fn ask(engine: Engine, question: &Question) -> Answer {
    if question.options.len() > approvals::MAX_OPTIONS {
        eprintln!("{engine} asked with {} options; cancelled", question.options.len());
        return Answer::Unanswered;
    }
    let mut shown = question.clone();
    shown.options = question
        .options
        .iter()
        .map(|o| LABELS.iter().find(|(from, _)| from == o).map(|(_, to)| to.to_string()).unwrap_or_else(|| o.clone()))
        .collect();
    approvals::show(&format!("Pytanie z {engine}"), &shown, || {}).unwrap_or(Answer::Unanswered)
}
