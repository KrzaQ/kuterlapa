//! Whether to send a typed draft, and with which key, in each application,
//! from ~/.config/kuterlapa/submit.json:
//!   { "default": "", "com.tinyspeck.slackmacgap": "Return" }
//! Keys are bundle identifiers on macOS. An empty value, or no entry,
//! means: only type the text. A send the user did not see coming (a
//! confirmation, a message, a command) costs too much, so sending is
//! something the user turns on per application.

use crate::paths;
use std::collections::HashMap;
use std::fs;

pub fn key_for(app: &str) -> Option<String> {
    let path = paths::config_dir().join("submit.json");
    let table: HashMap<String, String> = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let key = table.get(app).or_else(|| table.get("default")).cloned().unwrap_or_default();
    (!key.is_empty()).then_some(key)
}
