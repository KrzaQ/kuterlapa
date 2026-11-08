//! The voice provider: reads text aloud (docs/protocol.md).

use crate::{paths, provider};

pub fn supervise(command: Vec<String>) {
    provider::supervise("voice", command);
}

/// Sends a request ("say <text>", "stop", "status").
pub fn send(request: &str) -> String {
    provider::send("voice", &paths::voice_socket(), request)
}
