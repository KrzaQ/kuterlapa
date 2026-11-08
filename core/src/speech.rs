//! Runs the speech provider as a child process and starts it again when it
//! stops. The provider is any program that speaks the speech protocol
//! (docs/protocol.md), so Whisper is one choice among several.

use crate::paths;
use crate::socket;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn supervise(command: Vec<String>) {
    let Some((program, args)) = command.split_first() else {
        eprintln!("speech: no provider configured");
        return;
    };
    let program = paths::expand(program);
    if !program.exists() {
        eprintln!("speech: provider not found: {} (run `make install`)", program.display());
        return;
    }
    let args = args.to_vec();
    thread::spawn(move || loop {
        // The provider's stdin is a pipe that nobody writes to. When the core
        // exits, the pipe closes and the provider exits too.
        match Command::new(&program).args(&args).stdin(Stdio::piped()).spawn() {
            Ok(mut child) => {
                eprintln!("speech: provider started (pid {})", child.id());
                // Child::wait closes the child's stdin first, which would
                // tell the provider to exit. Keep our end open until then.
                let _stdin = child.stdin.take();
                let status = child.wait();
                eprintln!("speech: provider exited: {status:?}");
            }
            Err(e) => eprintln!("speech: cannot start {}: {e}", program.display()),
        }
        thread::sleep(Duration::from_secs(3));
    });
}

/// Sends a request ("toggle", "cancel") to the provider.
pub fn send(request: &str) -> String {
    match socket::request(&paths::speech_socket(), request) {
        Ok(answer) => answer,
        Err(e) => format!("error: speech provider: {e}"),
    }
}
