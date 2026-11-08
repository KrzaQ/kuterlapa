//! Questions to the user before a tool runs (`confirm = true` in the tool's
//! file). The MCP server sends "approve <json>" to the core. The core shows
//! the question in the overlay, reads it aloud when the voice is on, and
//! waits for the finish button (A: yes) or the erase button (B: no).

use crate::{hooks, shell, voice};
use serde_json::{json, Value};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// The core answers "no" after this time, a little before the MCP server
/// stops waiting.
const WAIT: Duration = Duration::from_secs(140);

static PENDING: Mutex<Option<Sender<bool>>> = Mutex::new(None);

/// Asks the user and waits. `restore` shows the overlay as it was before.
/// Returns the answer for the MCP server: "yes" or "no: <why>".
pub fn ask(question: &str, restore: impl Fn()) -> String {
    let question: Value = serde_json::from_str(question).unwrap_or(Value::Null);
    let text = |key: &str| question.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let (tool, description, command) = (text("tool"), text("description"), text("command"));
    let (sender, receiver) = mpsc::channel();
    {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_some() {
            return "no: another question is waiting".into();
        }
        *pending = Some(sender);
    }
    shell::overlay(&json!({
        "visible": true,
        "title": format!("Zgoda · {tool}"),
        "status": "Czekam na Twoją decyzję",
        "text": format!("{description}\n\n{command}").trim().to_string(),
        "buttons": [
            { "key": "A", "label": "Tak", "tone": "yes" },
            { "key": "B", "label": "Nie", "tone": "no" },
        ],
        "attention": true,
    }));
    if hooks::voice_mode() != "off" {
        voice::send(&format!("say Czy pozwalasz? {description}"));
    }
    let answer = receiver.recv_timeout(WAIT);
    PENDING.lock().unwrap().take();
    restore();
    match answer {
        Ok(true) => "yes".into(),
        Ok(false) => "no: the user said no".into(),
        Err(_) => "no: the user did not answer".into(),
    }
}

/// Answers the waiting question (the A and B buttons). Returns false if
/// none waits.
pub fn answer(yes: bool) -> bool {
    match PENDING.lock().unwrap().take() {
        Some(sender) => {
            let _ = sender.send(yes);
            true
        }
        None => false,
    }
}
