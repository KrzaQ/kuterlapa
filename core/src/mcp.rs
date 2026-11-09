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
                Ok(json!({ "tools": list }))
            }
            "tools/call" => Ok(call(&tools, &params, session.as_deref().unwrap_or("general"))),
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

fn call(tools: &[Tool], params: &Value, session: &str) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let empty = Map::new();
    let arguments = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
    if name.starts_with("memory_") {
        eprintln!("tool {name}: {}", Value::Object(arguments.clone()));
        let (text, ok) = match memory_call(name, arguments, session) {
            Ok(text) => (text, true),
            Err(e) => (format!("error: {e:#}"), false),
        };
        return result(text, ok);
    }
    if name.starts_with("session_") || name == "conversation_export" {
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

fn memory_schemas() -> Vec<Value> {
    let scope = json!({ "type": "string", "enum": ["global", "session"],
        "description": "global: about the computer, apps, websites and the user; session: about this session's topic" });
    vec![
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
            let name = sessions::switch(text("name"), true)?;
            Ok(format!("Session {name} is current now and stays until the user switches."))
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
            let plain = sessions::slug(text("name"))?;
            if plain == sessions::DEFAULT || !sessions::all().iter().any(|s| s.name == plain) {
                // Let delete explain (no such session, similar names).
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
