//! Requests to the platform shell (the native app on each system), which
//! types text and presses keys.

use crate::paths;
use crate::socket;

pub fn send(request: &str) -> String {
    match socket::request(&paths::shell_socket(), request) {
        Ok(answer) => answer,
        Err(e) => format!("error: shell: {e}"),
    }
}

pub fn type_text(text: &str) -> String {
    send(&format!("type {text}"))
}

pub fn key(combo: &str) -> String {
    send(&format!("key {combo}"))
}

/// Erase `count` characters before the cursor.
pub fn erase(count: usize) -> String {
    send(&format!("erase {count}"))
}
