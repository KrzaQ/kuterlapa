//! Built-in tools that let the model see and use the computer. The MCP
//! server (mcp.rs) offers them next to the tools from files; the platform
//! shell does the work (requests "screen", "ui", "press", ... on
//! shell.sock).
//!
//! The usual way: `computer_ui` lists the elements of the window in front
//! with numbers, and `computer_press` presses one. When an element is not
//! in the list (a picture, a web page drawn on a canvas), `computer_screen`
//! shows the screen and `computer_click` clicks a point on it.

use crate::shell;
use serde_json::{json, Map, Value};
use std::fs;

const TOOLS: &[(&str, &str, &str)] = &[
    (
        "computer_screen",
        "Take a screenshot of the main screen. A pixel in the picture is a point on the screen, so \
         use its coordinates with computer_click and computer_scroll.",
        r#"{}"#,
    ),
    (
        "computer_ui",
        "List the usable elements of the window in front (buttons, fields, links, text) with numbers, \
         positions and sizes. Prefer it to a screenshot: it is faster and exact.",
        r#"{}"#,
    ),
    (
        "computer_press",
        "Press an element by its number from the last computer_ui list.",
        r#"{"element": {"type": "integer", "description": "The number from computer_ui"}}"#,
    ),
    (
        "computer_click",
        "Click at a point on the screen (coordinates from computer_screen or computer_ui).",
        r#"{"x": {"type": "number"}, "y": {"type": "number"},
            "how": {"type": "string", "enum": ["click", "double-click", "right-click"], "description": "Default: click"}}"#,
    ),
    (
        "computer_type",
        "Type text into the focused field. Never type passwords, card numbers or other secrets.",
        r#"{"text": {"type": "string"}}"#,
    ),
    (
        "computer_key",
        "Press a key combination, for example Return, Escape, Tab, cmd+t, cmd+l, ctrl+Left, PageDown.",
        r#"{"combo": {"type": "string"}}"#,
    ),
    (
        "computer_scroll",
        "Scroll at a point on the screen.",
        r#"{"x": {"type": "number"}, "y": {"type": "number"},
            "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
            "amount": {"type": "number", "description": "Pixels, default 400"}}"#,
    ),
    ("computer_apps", "List the running applications; the one in front is marked with *.", r#"{}"#),
    (
        "computer_activate",
        "Bring an application to the front by its name (for example Safari, Slack), or open it.",
        r#"{"name": {"type": "string"}}"#,
    ),
];

/// The tool descriptions for MCP tools/list.
pub fn schemas() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|(name, description, properties)| {
            let properties: Value = serde_json::from_str(properties).expect("valid schema");
            let required: Vec<&String> = properties
                .as_object()
                .map(|p| p.keys().filter(|k| !matches!(k.as_str(), "how" | "amount")).collect())
                .unwrap_or_default();
            json!({
                "name": name,
                "description": description,
                "inputSchema": { "type": "object", "properties": properties, "required": required },
            })
        })
        .collect()
}

pub fn has(name: &str) -> bool {
    TOOLS.iter().any(|(n, _, _)| *n == name)
}

/// Runs a computer tool. Returns MCP content and whether it worked.
pub fn call(name: &str, args: &Map<String, Value>) -> (Vec<Value>, bool) {
    let number = |key: &str| args.get(key).and_then(Value::as_f64);
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let answer = match name {
        "computer_screen" => return screen(),
        "computer_ui" => {
            let answer = shell::send("ui");
            if answer.starts_with("error") {
                answer
            } else {
                fs::read_to_string(&answer).unwrap_or_else(|e| format!("error: {e}"))
            }
        }
        "computer_press" => match number("element") {
            Some(n) => shell::send(&format!("press {}", n as i64)),
            None => "error: element is missing".into(),
        },
        "computer_click" => match (number("x"), number("y")) {
            (Some(x), Some(y)) => {
                let how = args.get("how").and_then(Value::as_str).unwrap_or("click");
                shell::send(&format!("click {x} {y} {how}"))
            }
            _ => "error: x and y are missing".into(),
        },
        "computer_type" => shell::type_text(&text("text")),
        "computer_key" => shell::key(&text("combo")),
        "computer_scroll" => match (number("x"), number("y")) {
            (Some(x), Some(y)) => {
                let amount = number("amount").unwrap_or(400.0) as i64;
                let (dx, dy) = match text("direction").as_str() {
                    "up" => (0, amount),
                    "down" => (0, -amount),
                    "left" => (amount, 0),
                    "right" => (-amount, 0),
                    other => return (vec![text_content(&format!("error: bad direction {other:?}"))], false),
                };
                shell::send(&format!("scroll {x} {y} {dx} {dy}"))
            }
            _ => "error: x and y are missing".into(),
        },
        "computer_apps" => shell::send("apps").replace(" | ", "\n"),
        "computer_activate" => shell::send(&format!("activate {}", text("name"))),
        other => format!("error: no tool {other}"),
    };
    let ok = !answer.starts_with("error");
    (vec![text_content(&answer)], ok)
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
                text_content(&format!("Screen {width}x{height} points; the top left corner is 0,0.")),
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
        assert_eq!(super::schemas().len(), super::TOOLS.len());
    }
}
