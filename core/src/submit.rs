//! How to send a typed draft in each application, from
//! ~/.config/kuterlapa/submit.json:
//!   { "default": "Return", "com.apple.mail": "" }
//! Keys are bundle identifiers on macOS. An empty value means: only type
//! the text, never send it (mail, where a wrong send costs too much).

use crate::paths;
use std::collections::HashMap;
use std::fs;

pub fn key_for(app: &str) -> Option<String> {
    let path = paths::config_dir().join("submit.json");
    let table: HashMap<String, String> = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let key = table
        .get(app)
        .or_else(|| table.get("default"))
        .cloned()
        .unwrap_or_else(|| "Return".into());
    (!key.is_empty()).then_some(key)
}
