//! `kuterlapa mcp`: an MCP server on stdin and stdout that gives the model
//! the tools from tools.rs and the computer tools from computer.rs. The engine (for example `claude -p`) starts it
//! for each request.
//!
//!   kuterlapa mcp [--session <name>]
//!
//! Messages are JSON-RPC 2.0, one per line. A tool with `confirm = true`
//! runs only after the user agrees: the server asks the core
//! ("approve <json>" on core.sock), and the core shows the question in the
//! overlay and waits for A (yes) or B (no).

use crate::computer;
use crate::memory;
use crate::sessions;
use crate::help;
use crate::mapping::Mapping;
use crate::tools::{self, Tool};
use crate::{paths, socket};
use anyhow::Result;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::time::Duration;

const PROTOCOL: &str = "2025-06-18";
/// How long the server waits for the user's answer. The core gives up a
/// little earlier and answers "no".
const APPROVAL_WAIT: Duration = Duration::from_secs(150);

pub fn run(args: &[String]) -> Result<()> {
    let session = args.iter().position(|a| a == "--session").and_then(|i| args.get(i + 1)).cloned();
    let tools = tools::load(session.as_deref());
    // Actions already done in this request (one engine run), by tool and
    // arguments: see `call`.
    let mut done = Repeats::default();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(e) => {
                reply(&mut stdout, Value::Null, Err((-32700, format!("parse error: {e}"))))?;
                continue;
            }
        };
        // A notification has no id and gets no answer.
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "kuterlapa", "version": env!("CARGO_PKG_VERSION") },
            })),
            "ping" => Ok(json!({})),
            "tools/list" => {
                let mut list: Vec<Value> = tools.iter().map(Tool::schema).collect();
                list.extend(computer::schemas());
                list.push(json!({
                    "name": "ask_user",
                    "description": "Ask the user a yes-or-no question and wait for the answer: it shows in the \
                                    overlay with large Yes (A) and No (B) buttons. Use it whenever you need the \
                                    user to confirm or choose between two options; never write \"A — …, B — …\" \
                                    in an answer, as the buttons do not answer text. To act only after a yes, put \
                                    the action in on_yes (a tool name and its arguments): it runs right after the \
                                    user presses yes, as the user's answer is their permission. Answers yes or no, \
                                    and the action's result.",
                    "inputSchema": { "type": "object", "properties": {
                        "question": { "type": "string", "description": "Short, ending with a question mark" },
                        "on_yes": { "type": "object", "description": "Optional: {\"name\": tool, \"arguments\": {...}}",
                            "properties": { "name": { "type": "string" }, "arguments": { "type": "object" } } },
                    }, "required": ["question"] },
                }));
                list.extend(memory_schemas());
                list.extend(session_schemas());
                list.push(json!({
                    "name": "kuterlapa_help",
                    "description": "How to use Kuterlapa, the app the user talks to you through: what each \
                                    controller button does (from the user's current settings) and how voice, \
                                    the narrator, consent and cancelling work. Use it to answer questions \
                                    about Kuterlapa or the controller.",
                    "inputSchema": { "type": "object", "properties": {} },
                }));
                annotate(&mut list);
                Ok(json!({ "tools": list }))
            }
            "tools/call" => Ok(call(&tools, &params, session.as_deref().unwrap_or("general"), &mut done)),
            other => Err((-32601, format!("unknown method {other}"))),
        };
        reply(&mut stdout, id, result)?;
    }
    Ok(())
}

fn reply(out: &mut impl Write, id: Value, result: Result<Value, (i64, String)>) -> Result<()> {
    let message = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, text)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": text } }),
    };
    writeln!(out, "{message}")?;
    out.flush()?;
    Ok(())
}

/// Built-in tools that change something and make no sense twice with the
/// same arguments in one request.
const ONCE: &[&str] = &[
    "memory_write", "memory_forget", "memory_move", "session_switch", "session_back", "session_bind",
    "session_auto", "session_create", "session_delete", "session_show", "conversation_export", "conversation_show",
];

/// What one request did, to catch an action repeated by mistake.
#[derive(Default)]
struct Repeats {
    /// How often each action (tool and arguments) succeeded.
    times: std::collections::HashMap<String, u32>,
    /// Repeats the user refused: refused again without asking.
    refused: std::collections::HashSet<String>,
}

/// The most times one action may run in one request, even with the user's
/// agreement.
const MAX_REPEATS: u32 = 5;

/// Runs a tool call. An action that already succeeded in this request with
/// the same arguments runs again only if the model says it means it
/// ("repeat": true) and the user agrees with A: a model once read the note
/// it had just written, took it as a new task, and opened the same page
/// four times, and a permission classifier lets such repeats through.
/// Keys, clicks and the other computer tools may repeat freely.
fn call(tools: &[Tool], params: &Value, session: &str, done: &mut Repeats) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut params = params.clone();
    let repeat = params
        .get_mut("arguments")
        .and_then(Value::as_object_mut)
        .and_then(|a| a.remove("repeat"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let once = ONCE.contains(&name.as_str()) || tools.iter().any(|t| t.name == name && !t.read_only);
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let key = format!("{name} {arguments}");
    let times = done.times.get(&key).copied().unwrap_or(0);
    if once && times > 0 {
        if done.refused.contains(&key) {
            return result("The user refused repeating this. Do not try again.".into(), false);
        }
        if !repeat {
            eprintln!("tool {name}: refused, done already in this request");
            return result(
                "Already done in this request with the same arguments. Do not repeat it; the task is done. \
                 Only if the user really asked for it more than once, call it again with \"repeat\": true; \
                 the user then confirms."
                    .into(),
                false,
            );
        }
        if times >= MAX_REPEATS {
            return result(format!("Done {times} times already in this request; that is the limit."), false);
        }
        let description = tools.iter().find(|t| t.name == name).map(|t| t.description.clone()).unwrap_or_default();
        let question = format!("Powtórzenie ({}. raz w tej prośbie). {description}", times + 1);
        if let Err(refusal) = ask_user(&name, &question, &arguments.to_string()) {
            done.refused.insert(key);
            return result(refusal, false);
        }
    }
    let answer = call_tool(tools, &params, session);
    if once && answer["isError"] == json!(false) {
        *done.times.entry(key).or_default() += 1;
    }
    answer
}

fn call_tool(tools: &[Tool], params: &Value, session: &str) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let empty = Map::new();
    let arguments = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
    if name == "ask_user" {
        let question = arguments.get("question").and_then(Value::as_str).unwrap_or_default();
        eprintln!("tool ask_user: {question}");
        let request = json!({ "kind": "question", "tool": "ask_user", "description": question, "command": "" });
        let answer = socket::request_waiting(&paths::core_socket(), &format!("approve {request}"), APPROVAL_WAIT)
            .unwrap_or_else(|e| format!("error: {e}"));
        let text = match answer.as_str() {
            "yes" => "The user answered: yes.".to_string(),
            a if a.contains("did not answer") => "The user did not answer.".to_string(),
            a if a.starts_with("no") => "The user answered: no.".to_string(),
            other => format!("Could not ask: {other}"),
        };
        // The action the question was about runs on yes, with the user's
        // answer as its permission (a classifier only sees what the user
        // said, not the button).
        if let (true, Some(action)) = (answer == "yes", arguments.get("on_yes").filter(|a| a.is_object())) {
            let action_name = action.get("name").and_then(Value::as_str).unwrap_or_default();
            if action_name == "ask_user" || action_name.is_empty() {
                return result(format!("{text} on_yes needs another tool."), false);
            }
            let call = json!({ "name": action_name, "arguments": action.get("arguments").cloned().unwrap_or(json!({})) });
            let done = call_tool(tools, &call, session);
            let outcome = done["content"][0]["text"].as_str().unwrap_or_default().to_string();
            let ok = done["isError"] == json!(false);
            return result(format!("{text} Then {action_name}: {outcome}"), ok);
        }
        return result(text, true);
    }
    if name.starts_with("memory_") {
        eprintln!("tool {name}: {}", Value::Object(arguments.clone()));
        let (text, ok) = match memory_call(name, arguments, session) {
            Ok(text) => (text, true),
            Err(e) => (format!("error: {e:#}"), false),
        };
        return result(text, ok);
    }
    if name.starts_with("session_") || name.starts_with("conversation_") {
        eprintln!("tool {name}: {}", Value::Object(arguments.clone()));
        let (text, ok) = match session_call(name, arguments) {
            Ok(text) => (text, true),
            Err(e) => (format!("error: {e:#}"), false),
        };
        return result(text, ok);
    }
    if name == "kuterlapa_help" {
        let text = help::text(&mut Mapping::new());
        return json!({ "content": [{ "type": "text", "text": text }], "isError": false });
    }
    if computer::has(name) && !tools.iter().any(|t| t.name == name) {
        eprintln!("tool {name}: {}", Value::Object(arguments.clone()));
        let (content, ok) = computer::call(name, arguments);
        return json!({ "content": content, "isError": !ok });
    }
    let Some(tool) = tools.iter().find(|t| t.name == name) else {
        return result(format!("no tool named {name}"), false);
    };
    eprintln!("tool {name}: {}", Value::Object(arguments.clone()));
    if tool.confirm {
        // Shorter to read: the home folder as "~".
        let home = paths::home().to_string_lossy().into_owned();
        let shown = tool.argv(arguments).map(|a| a.join(" ").replace(&home, "~")).unwrap_or_default();
        if let Err(refusal) = ask_user(name, &tool.description, &shown) {
            return result(refusal, false);
        }
    }
    let (text, ok) = tool.run(arguments);
    result(if text.is_empty() { "done".into() } else { text }, ok)
}

/// Asks the user through the core (the overlay, A or B). Err holds the
/// text for the model when the user does not allow it.
fn ask_user(tool: &str, description: &str, command: &str) -> Result<(), String> {
    let question = json!({ "tool": tool, "description": description, "command": command });
    let answer = socket::request_waiting(&paths::core_socket(), &format!("approve {question}"), APPROVAL_WAIT)
        .unwrap_or_else(|e| format!("error: {e}"));
    if answer == "yes" {
        Ok(())
    } else {
        Err(format!("The user did not allow this ({answer}). Do not try again unless asked."))
    }
}

fn result(text: String, ok: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": !ok })
}

/// MCP hints for the built-in memory, session and help tools that have
/// none yet: which only read, and which delete. Tools from files and the
/// computer tools set their own.
fn annotate(list: &mut [Value]) {
    const READ_ONLY: &[&str] = &["memory_read", "memory_search", "session_list", "kuterlapa_help"];
    const DESTRUCTIVE: &[&str] = &["memory_forget", "session_delete"];
    for tool in list.iter_mut() {
        if tool.get("annotations").is_some() {
            continue;
        }
        let name = tool["name"].as_str().unwrap_or_default();
        let mut hints = json!({ "readOnlyHint": READ_ONLY.contains(&name) });
        if DESTRUCTIVE.contains(&name) {
            hints["destructiveHint"] = json!(true);
        }
        tool["annotations"] = hints;
    }
}

fn memory_schemas() -> Vec<Value> {
    let scope = json!({ "type": "string", "enum": ["global", "session"],
        "description": "global: about the computer, apps, websites and the user; session: about this session's topic" });
    vec![
        json!({
            "name": "memory_search",
            "description": "Search by meaning in your notes (all sessions and global) and in all past \
                            conversations with the user, for example \"what did we decide about the backups\". \
                            Answers with the closest texts, where they come from and a similarity score.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "How many results, default 6" },
            }, "required": ["query"] },
        }),
        json!({
            "name": "memory_read",
            "description": "Read a whole note from your memory by its name (the system prompt lists the notes).",
            "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
        }),
        json!({
            "name": "memory_write",
            "description": "Save a note for later, or replace the note with the same name: for example how to do a \
                            task on the computer that worked. Never store passwords, PINs, codes or card numbers.",
            "inputSchema": { "type": "object", "properties": {
                "scope": scope,
                "name": { "type": "string", "description": "Short, for example whatsapp or chrome-gmail" },
                "description": { "type": "string", "description": "One line: what the note is about" },
                "text": { "type": "string", "description": "The note: exact steps, tools, addresses, profiles" },
            }, "required": ["scope", "name", "description", "text"] },
        }),
        json!({
            "name": "memory_forget",
            "description": "Delete a note, when the user asks or it turned out wrong.",
            "inputSchema": { "type": "object", "properties": {
                "name": { "type": "string" }, "scope": scope,
            }, "required": ["name"] },
        }),
        json!({
            "name": "memory_move",
            "description": "Move a note between global memory and this session's memory.",
            "inputSchema": { "type": "object", "properties": {
                "name": { "type": "string" }, "to": scope,
            }, "required": ["name", "to"] },
        }),
    ]
}

fn memory_call(name: &str, args: &Map<String, Value>, session: &str) -> anyhow::Result<String> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    let scope = |key: &str| memory::Scope::parse(text(key));
    let place = |s: memory::Scope| match s {
        memory::Scope::Global => "global memory".to_string(),
        memory::Scope::Session => format!("the memory of session {session}"),
    };
    match name {
        "memory_search" => {
            let store = crate::store::Store::open()?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(6).clamp(1, 20) as usize;
            crate::search::search(&store, text("query"), limit)
        }
        "memory_read" => {
            let note = memory::read(session, text("name"))?;
            Ok(format!("{} ({}): {}\n\n{}", note.name, place(note.scope), note.description, note.text))
        }
        "memory_write" => {
            let s = scope("scope").ok_or_else(|| anyhow::anyhow!("scope must be global or session"))?;
            let saved = memory::write(session, s, text("name"), text("description"), text("text"))?;
            Ok(format!("Saved note {saved} in {}.", place(s)))
        }
        "memory_forget" => {
            let s = memory::forget(session, text("name"), scope("scope"))?;
            Ok(format!("Deleted note {} from {}.", text("name"), place(s)))
        }
        "memory_move" => {
            let to = scope("to").ok_or_else(|| anyhow::anyhow!("to must be global or session"))?;
            memory::move_to(session, text("name"), to)?;
            Ok(format!("Moved note {} to {}.", text("name"), place(to)))
        }
        other => anyhow::bail!("no tool {other}"),
    }
}

fn session_schemas() -> Vec<Value> {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    vec![
        json!({
            "name": "session_list",
            "description": "List the user's sessions (named contexts for work, each with its own conversation, \
                            notes and words), which one is current, and whether the window in front picks it.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "session_switch",
            "description": "Make a session current, when the user asks to go to it. It stays until the user \
                            switches again (sticky). The next message goes to that session's conversation. If \
                            it does not exist, never create it: tell the user, name similar sessions, and \
                            offer to create it.",
            "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
        }),
        json!({
            "name": "conversation_show",
            "description": "Open the large chat view at the past message closest in meaning to the query, \
                            scrolled to it and marked, when the user wants to see where something was said \
                            (\"show me where we talked about …\"). Answers with the message it found.",
            "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] },
            "annotations": { "readOnlyHint": true },
        }),
        json!({
            "name": "conversation_export",
            "description": "Save conversations as Markdown files in ~/Documents/Kuterlapa/<session>/, when \
                            the user asks: the newest conversation of a session, or all of them. Answers \
                            with the files.",
            "inputSchema": { "type": "object", "properties": {
                "session": { "type": "string", "description": "Default: the current session" },
                "all": { "type": "boolean", "description": "All conversations of the session; default: the newest" },
            } },
        }),
        json!({
            "name": "session_delete",
            "description": "Delete a session, only when the user explicitly asks: its folder (context, notes, \
                            words, window rules) goes to the Trash, its conversations stay in the history. The \
                            user confirms with A or B first.",
            "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"] },
        }),
        json!({
            "name": "session_show",
            "description": "Show the user the recently used sessions in the overlay (a readable panel), \
                            when they ask to see their sessions.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "session_back",
            "description": "Go back to the session before the current one, when the user asks.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "session_bind",
            "description": "When the user says the window in front belongs to a session: add that window to the \
                            session's rules, so it picks the session (automatic mode). title_part is a part of \
                            the window title that stays the same (a sheet, project or channel name; not counts \
                            of unread items). With by_app true and no title_part, the whole application picks \
                            the session. Answers with the window it used.",
            "inputSchema": { "type": "object", "properties": {
                "session": { "type": "string" },
                "title_part": { "type": "string" },
                "by_app": { "type": "boolean", "description": "Also require (or, alone, use) the application; default true" },
            }, "required": ["session"] },
        }),
        json!({
            "name": "session_auto",
            "description": "From now on, pick the session from the window in front each time the user starts to \
                            speak (by each session's apps and title rules); \"general\" when none fits.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "session_create",
            "description": "Create a new session, only when the user explicitly asks to create one: a short \
                            name (Polish letters are fine), what it is about, and optionally a project folder \
                            and the windows that pick it (application names or bundle ids, parts of window \
                            titles). It refuses when a session with a similar name exists: then ask the user \
                            whether they mean that one, and pass allow_similar only if they confirm a new one.",
            "inputSchema": { "type": "object", "properties": {
                "name": { "type": "string" },
                "context": { "type": "string", "description": "What the session is about, for the model" },
                "folder": { "type": "string" },
                "apps": strings,
                "titles": strings,
                "allow_similar": { "type": "boolean", "description": "Only after the user confirmed" },
            }, "required": ["name", "context"] },
        }),
    ]
}

fn session_call(name: &str, args: &Map<String, Value>) -> anyhow::Result<String> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
    let list = |key: &str| -> Vec<String> {
        args.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    match name {
        "session_list" => {
            let current = sessions::current_name();
            let mode = if sessions::automatic() { "picked by the window in front" } else { "sticky" };
            let mut out = format!("Current session: {current} ({mode}).\n");
            for s in sessions::all() {
                let about = s.context.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
                let rules = s.rules();
                let label = if s.label != s.name { format!(" ({})", s.label) } else { String::new() };
                out.push_str(&format!("- {}{label}{}{}\n", s.name,
                    if about.is_empty() { String::new() } else { format!(": {about}") },
                    if rules.is_empty() { String::new() } else { format!(" [{rules}]") }));
            }
            Ok(out)
        }
        "session_switch" => {
            let asked = sessions::slug(text("name")).unwrap_or_default();
            let name = sessions::switch(text("name"), true)?;
            let note = if name != asked { format!(" (the closest name to \"{}\")", text("name")) } else { String::new() };
            Ok(format!("Session {name}{note} is current now and stays until the user switches."))
        }
        "conversation_show" => {
            let store = crate::store::Store::open()?;
            let Some((id, label, found)) = crate::search::best_message(&store, text("query"))? else {
                anyhow::bail!("no messages to search yet");
            };
            let answer = socket::request(&paths::core_socket(), &format!("view-open {id}"))?;
            if answer != "ok" {
                anyhow::bail!("{answer}");
            }
            Ok(format!("The chat view shows it ({label}): {found}\nDo not repeat it at length."))
        }
        "conversation_export" => {
            let store = crate::store::Store::open()?;
            let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
            let session = Some(text("session")).filter(|s| !s.is_empty());
            let files = crate::export::export(&store, session, all)?;
            let home = paths::home().to_string_lossy().into_owned();
            let list: Vec<String> = files.iter().map(|p| p.display().to_string().replace(&home, "~")).collect();
            Ok(format!("Saved {} file(s):\n{}", list.len(), list.join("\n")))
        }
        "session_delete" => {
            let plain = sessions::resolve(text("name"))?;
            if plain == sessions::DEFAULT {
                sessions::delete(&plain)?;
            }
            ask_user(
                "session_delete",
                &format!("Usuń sesję „{plain}” (do Kosza; rozmowy zostają w historii)."),
                &format!("~/.config/kuterlapa/sessions/{plain} → Kosz"),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            let name = sessions::delete(&plain)?;
            Ok(format!("Session {name} is in the Trash now."))
        }
        "session_show" => {
            let answer = socket::request(&paths::core_socket(), "panel sessions")?;
            if answer != "ok" {
                anyhow::bail!("{answer}");
            }
            Ok("The panel with recent sessions is on screen; B closes it. Do not repeat the list.".into())
        }
        "session_back" => {
            let name = sessions::back()?;
            Ok(format!("Back in session {name}; it stays until the user switches."))
        }
        "session_bind" => {
            let by_app = args.get("by_app").and_then(Value::as_bool).unwrap_or(true);
            let window = sessions::bind(text("session"), text("title_part"), by_app)?;
            Ok(format!("The window {window:?} now picks session {} (when sessions are automatic).", text("session")))
        }
        "session_auto" => {
            sessions::set_automatic()?;
            Ok("Sessions now follow the window in front.".into())
        }
        "session_create" => {
            let folder = Some(text("folder")).filter(|f| !f.is_empty());
            let allow = args.get("allow_similar").and_then(Value::as_bool).unwrap_or(false);
            let name = sessions::create(text("name"), text("context"), folder, &list("apps"), &list("titles"), allow)?;
            Ok(format!("Created session {name}. It is not current; switch to it if the user wants."))
        }
        other => anyhow::bail!("no tool {other}"),
    }
}
