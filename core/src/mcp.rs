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
                Ok(json!({ "tools": list }))
            }
            "tools/call" => Ok(call(&tools, &params)),
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

fn call(tools: &[Tool], params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let empty = Map::new();
    let arguments = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
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
