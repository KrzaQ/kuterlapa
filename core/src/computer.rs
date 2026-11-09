//! Built-in tools that let the model see and use the computer. The MCP
//! server (mcp.rs) offers them next to the tools from files; the platform
//! shell does the work (requests "screen", "ui", "press", ... on
//! shell.sock).
//!
//! Each model turn costs seconds, so the tools save turns: every action
//! answers with the elements of the window in front as they are after it,
//! `computer_do` runs several actions in one call, and `computer_find`
//! lists only the elements that match a text.

use crate::{paths, sessions, shell};
use serde_json::{json, Map, Value};
use std::fs;
use std::thread;
use std::time::Duration;

/// Time for the window to change after an action, before the new list.
const SETTLE: Duration = Duration::from_millis(300);

const STEP: &str = r#"{"type": "object", "properties": {
    "action": {"type": "string", "enum": ["press", "click", "type", "key", "scroll", "activate", "focus_window", "desktop", "wait"]},
    "element": {"type": "integer"}, "x": {"type": "number"}, "y": {"type": "number"},
    "how": {"type": "string"}, "text": {"type": "string"}, "combo": {"type": "string"},
    "direction": {"type": "string"}, "amount": {"type": "number"}, "name": {"type": "string"},
    "window": {"type": "integer"}, "target": {"type": "string"},
    "ms": {"type": "number"}}, "required": ["action"]}"#;

const TOOLS: &[(&str, &str, &str)] = &[
    (
        "computer_ui",
        "List the usable elements of the window in front (buttons, fields, links, text) with numbers, \
         positions and sizes. Prefer it to a screenshot: it is faster and exact. Every action tool \
         answers with this list as it is after the action, so call it again only when needed.",
        r#"{}"#,
    ),
    (
        "computer_find",
        "Like computer_ui, but only the elements whose text contains the given words (any case). \
         Use it in big windows such as a browser.",
        r#"{"text": {"type": "string"}}"#,
    ),
    (
        "computer_screen",
        "Take a screenshot of the main screen, for what computer_ui cannot show (pictures, canvas). \
         Its coordinates are the same as in computer_ui and computer_click.",
        r#"{}"#,
    ),
    (
        "computer_do",
        "Run several actions in a row, in one call, for example pressing several buttons or typing \
         and pressing Return. Each step is an object with \"action\" (press, click, type, key, \
         scroll, activate, wait) and the arguments of the tool of that name (element; x, y, how; \
         text; combo; x, y, direction, amount; name; ms). Element numbers come from the last list. \
         Answers with the elements after the last step.",
        r#"{"steps": {"type": "array", "items": STEP}}"#,
    ),
    (
        "computer_press",
        "Press an element by its number from the last list.",
        r#"{"element": {"type": "integer", "description": "The number from the list"}}"#,
    ),
    (
        "computer_click",
        "Click at a point (coordinates from computer_ui or computer_screen).",
        r#"{"x": {"type": "number"}, "y": {"type": "number"},
            "how": {"type": "string", "enum": ["click", "double-click", "right-click"], "description": "Default: click"}}"#,
    ),
    (
        "computer_type",
        "Type text into the focused field of the application in front. Never type passwords, card \
         numbers or other secrets. For a terminal, use iterm_run instead: it writes to the tab itself.",
        r#"{"text": {"type": "string"}}"#,
    ),
    (
        "computer_key",
        "Press a key combination, for example Return, Escape, Tab, cmd+t, cmd+l, ctrl+Left, PageDown.",
        r#"{"combo": {"type": "string"}}"#,
    ),
    (
        "computer_scroll",
        "Scroll at a point.",
        r#"{"x": {"type": "number"}, "y": {"type": "number"},
            "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
            "amount": {"type": "number", "description": "Pixels, default 400"}}"#,
    ),
    ("computer_apps", "List the running applications; the one in front is marked with *.", r#"{}"#),
    (
        "computer_zoom",
        "Make text bigger or smaller, for a user who sits far from the screen. target \"app\": the \
         application in front (cmd+= / cmd+-; reset: cmd+0), which works in most Mac apps such as \
         Chrome, iTerm or Claude. target \"overlay\": Kuterlapa's own panel in the corner; its size is \
         remembered.",
        r#"{"target": {"type": "string", "enum": ["app", "overlay"]},
            "direction": {"type": "string", "enum": ["in", "out", "reset"]},
            "steps": {"type": "integer", "description": "How many steps, default 1"}}"#,
    ),
    (
        "computer_windows",
        "List the windows of all applications, numbered, with the virtual desktop each is on, and \
         the number of the current desktop. Use it to find where a window is.",
        r#"{}"#,
    ),
    (
        "computer_focus_window",
        "Bring a window to the front by its number from computer_windows: go to its desktop and \
         raise it.",
        r#"{"window": {"type": "integer"}}"#,
    ),
    (
        "computer_desktop",
        "Go to a virtual desktop: its number, \"left\" or \"right\".",
        r#"{"target": {"type": "string", "description": "A number such as 2, or left, or right"}}"#,
    ),
    (
        "computer_activate",
        "Bring an application to the front by its name (for example Safari, Slack), or open it.",
        r#"{"name": {"type": "string"}}"#,
    ),
];

/// Keys that a tool does not require.
const OPTIONAL: &[&str] = &["how", "amount", "steps"];

/// Tools that only look; the others act on the computer.
const READ_ONLY: &[&str] = &["computer_ui", "computer_find", "computer_screen", "computer_apps", "computer_windows"];

/// The tool descriptions for MCP tools/list.
pub fn schemas() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|(name, description, properties)| {
            let properties: Value = serde_json::from_str(&properties.replace("STEP", STEP)).expect("valid schema");
            let required: Vec<&String> = properties
                .as_object()
                .map(|p| p.keys().filter(|k| !OPTIONAL.contains(&k.as_str())).collect())
                .unwrap_or_default();
            json!({
                "name": name,
                "description": description,
                "inputSchema": { "type": "object", "properties": properties, "required": required },
                "annotations": { "readOnlyHint": READ_ONLY.contains(name) },
            })
        })
        .collect()
}

pub fn has(name: &str) -> bool {
    TOOLS.iter().any(|(n, _, _)| *n == name)
}

/// Runs a computer tool. Returns MCP content and whether it worked.
pub fn call(name: &str, args: &Map<String, Value>) -> (Vec<Value>, bool) {
    let answer = match name {
        "computer_screen" => return screen(),
        "computer_ui" => ui(None),
        "computer_find" => ui(Some(&text(args, "text"))),
        "computer_apps" => Ok(shell::send("apps").replace(" | ", "\n")),
        "computer_windows" => checked(shell::send("windows")).map(|l| l.replace(" | ", "\n")),
        "computer_do" => run_steps(args),
        "computer_zoom" => zoom(args),
        other => match other.strip_prefix("computer_") {
            Some(action) => {
                // Keys and text go to the application in front, which may
                // not be the one the model thinks: say where they went.
                let target = matches!(action, "type" | "key")
                    .then(sessions::front_window)
                    .flatten()
                    .map(|(_, app, title)| format!("Went to: {app} — {title}\n"))
                    .unwrap_or_default();
                act(action, args).and_then(|_| after()).map(|list| format!("{target}{list}"))
            }
            None => Err(format!("no tool {other}")),
        },
    };
    match answer {
        Ok(text) => (vec![text_content(&text)], true),
        Err(e) => (vec![text_content(&format!("error: {e}"))], false),
    }
}

fn run_steps(args: &Map<String, Value>) -> Result<String, String> {
    let steps = args.get("steps").and_then(Value::as_array).ok_or("steps is missing")?;
    for (i, step) in steps.iter().enumerate() {
        let step = step.as_object().ok_or(format!("step {} is not an object", i + 1))?;
        let action = step.get("action").and_then(Value::as_str).unwrap_or_default();
        act(action, step).map_err(|e| format!("step {} ({action}): {e}", i + 1))?;
        thread::sleep(Duration::from_millis(80));
    }
    after()
}

/// One action, without the list after it.
fn act(action: &str, args: &Map<String, Value>) -> Result<(), String> {
    let number = |key: &str| args.get(key).and_then(Value::as_f64);
    let point = || match (number("x"), number("y")) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err("x and y are missing".to_string()),
    };
    let answer = match action {
        "press" => shell::send(&format!("press {}", number("element").ok_or("element is missing")? as i64)),
        "click" => {
            let (x, y) = point()?;
            let how = args.get("how").and_then(Value::as_str).unwrap_or("click");
            shell::send(&format!("click {x} {y} {how}"))
        }
        "type" => shell::type_text(&text(args, "text")),
        "key" => shell::key(&text(args, "combo")),
        "scroll" => {
            let (x, y) = point()?;
            let amount = number("amount").unwrap_or(400.0) as i64;
            let (dx, dy) = match text(args, "direction").as_str() {
                "up" => (0, amount),
                "down" => (0, -amount),
                "left" => (amount, 0),
                "right" => (-amount, 0),
                other => return Err(format!("bad direction {other:?}")),
            };
            shell::send(&format!("scroll {x} {y} {dx} {dy}"))
        }
        "activate" => shell::send(&format!("activate {}", text(args, "name"))),
        "focus_window" => shell::send(&format!("focus {}", number("window").ok_or("window is missing")? as i64)),
        "desktop" => {
            let target = match args.get("target") {
                Some(Value::Number(n)) => n.to_string(),
                _ => text(args, "target"),
            };
            shell::send(&format!("desktop {target}"))
        }
        "wait" => {
            thread::sleep(Duration::from_millis(number("ms").unwrap_or(500.0).clamp(0.0, 5000.0) as u64));
            "ok".into()
        }
        other => return Err(format!("unknown action {other:?}")),
    };
    match answer.strip_prefix("error: ") {
        Some(e) => Err(e.to_string()),
        None => Ok(()),
    }
}

/// The elements after an action, once the window had time to change.
fn after() -> Result<String, String> {
    thread::sleep(SETTLE);
    ui(None).map(|list| format!("Done. Now:\n{list}"))
}

/// The element list; with `filter`, only the header and matching lines.
fn ui(filter: Option<&str>) -> Result<String, String> {
    let answer = shell::send("ui");
    if let Some(e) = answer.strip_prefix("error: ") {
        return Err(e.to_string());
    }
    let list = fs::read_to_string(&answer).map_err(|e| e.to_string())?;
    let Some(filter) = filter.map(str::to_lowercase).filter(|f| !f.is_empty()) else {
        return Ok(list);
    };
    let lines: Vec<&str> = list
        .lines()
        .filter(|l| !l.starts_with('[') || l.to_lowercase().contains(&filter))
        .collect();
    if lines.iter().all(|l| !l.starts_with('[')) {
        return Ok(format!("{}\n(no element contains {filter:?})", lines.join("\n")));
    }
    Ok(lines.join("\n"))
}

/// One step of the overlay's size.
const OVERLAY_STEP: f64 = 1.15;

fn zoom(args: &Map<String, Value>) -> Result<String, String> {
    let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(1).clamp(1, 10);
    let direction = text(args, "direction");
    match text(args, "target").as_str() {
        "app" => {
            let combo = match direction.as_str() {
                "in" => "cmd+=",
                "out" => "cmd+-",
                "reset" => "cmd+0",
                other => return Err(format!("bad direction {other:?}")),
            };
            let times = if direction == "reset" { 1 } else { steps };
            for _ in 0..times {
                checked(shell::key(combo))?;
                thread::sleep(Duration::from_millis(60));
            }
            Ok(format!("pressed {combo} {times}×"))
        }
        "overlay" => {
            let now = overlay_scale();
            let scale = match direction.as_str() {
                "in" => now * OVERLAY_STEP.powi(steps as i32),
                "out" => now / OVERLAY_STEP.powi(steps as i32),
                "reset" => 1.0,
                other => return Err(format!("bad direction {other:?}")),
            }
            .clamp(0.6, 3.0);
            fs::write(paths::state_dir().join("overlay-scale"), format!("{scale:.3}\n")).map_err(|e| e.to_string())?;
            checked(shell::send(&format!("overlay-scale {scale:.3}")))?;
            Ok(format!("overlay size {:.0}%", scale * 100.0))
        }
        other => Err(format!("bad target {other:?}")),
    }
}

/// The overlay's size from the state folder; 1 when not set.
pub fn overlay_scale() -> f64 {
    fs::read_to_string(paths::state_dir().join("overlay-scale"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1.0)
}

fn checked(answer: String) -> Result<String, String> {
    match answer.strip_prefix("error: ") {
        Some(e) => Err(e.to_string()),
        None => Ok(answer),
    }
}

fn text(args: &Map<String, Value>, key: &str) -> String {
    args.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn screen() -> (Vec<Value>, bool) {
    let answer = shell::send("screen");
    let mut words = answer.split(' ');
    let (Some(path), Some(width), Some(height)) = (words.next(), words.next(), words.next()) else {
        return (vec![text_content(&answer)], false);
    };
    if answer.starts_with("error") {
        return (vec![text_content(&answer)], false);
    }
    match fs::read(path) {
        Ok(bytes) => (
            vec![
                json!({ "type": "image", "data": base64(&bytes), "mimeType": "image/jpeg" }),
                text_content(&format!("Screen {width}x{height}; the top left corner is 0,0.")),
            ],
            true,
        ),
        Err(e) => (vec![text_content(&format!("error: reading {path}: {e}"))], false),
    }
}

fn text_content(text: &str) -> Value {
    json!({ "type": "text", "text": if text.is_empty() { "ok" } else { text } })
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn schemas_parse() {
        let schemas = super::schemas();
        assert_eq!(schemas.len(), super::TOOLS.len());
        let todo = schemas.iter().find(|s| s["name"] == "computer_do").unwrap();
        assert_eq!(todo["inputSchema"]["properties"]["steps"]["items"]["required"][0], "action");
    }
}
