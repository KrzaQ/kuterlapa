//! The speech provider: speech recognition (docs/protocol.md). Whisper is
//! one choice among several.

use crate::{paths, provider};

pub fn supervise(command: Vec<String>) {
    provider::supervise("speech", command);
}

/// Sends a request ("toggle", "cancel", "finish", "status").
pub fn send(request: &str) -> String {
    provider::send("speech", &paths::speech_socket(), request)
}
