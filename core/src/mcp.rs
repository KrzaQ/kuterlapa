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
        let question = json!({ "tool": name, "description": tool.description, "command": shown });
        let answer = socket::request_waiting(&paths::core_socket(), &format!("approve {question}"), APPROVAL_WAIT)
            .unwrap_or_else(|e| format!("error: {e}"));
        if answer != "yes" {
            return result(format!("The user did not allow this ({answer}). Do not try again unless asked."), false);
        }
    }
    let (text, ok) = tool.run(arguments);
    result(if text.is_empty() { "done".into() } else { text }, ok)
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
