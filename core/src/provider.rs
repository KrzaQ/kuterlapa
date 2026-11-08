//! Providers: helper programs that the core runs as children (speech
//! recognition, the voice). The core starts a provider again when it stops.

use crate::hooks::{self, Flag};
use crate::paths;
use crate::socket;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn supervise(name: &'static str, command: Vec<String>) {
    let Some((program, args)) = command.split_first() else {
        eprintln!("{name}: no provider configured");
        return;
    };
    let program = paths::expand(program);
    if !program.exists() {
        eprintln!("{name}: provider not found: {} (run `make install`)", program.display());
        return;
    }
    let args: Vec<String> = args.iter().map(|a| paths::expand(a).to_string_lossy().into_owned()).collect();
    thread::spawn(move || loop {
        // The provider's stdin is a pipe that nobody writes to. When the core
        // exits, the pipe closes and the provider exits too.
        match Command::new(&program).args(&args).stdin(Stdio::piped()).spawn() {
            Ok(mut child) => {
                eprintln!("{name}: provider started (pid {})", child.id());
                // Child::wait closes the child's stdin first, which would
                // tell the provider to exit. Keep our end open until then.
                let _stdin = child.stdin.take();
                let status = child.wait();
                eprintln!("{name}: provider exited: {status:?}");
                // A provider that died cannot report that it stopped.
                match name {
                    "speech" => hooks::set(Flag::Listening, false),
                    "voice" => hooks::set(Flag::Speaking, false),
                    _ => {}
                }
            }
            Err(e) => eprintln!("{name}: cannot start {}: {e}", program.display()),
        }
        thread::sleep(Duration::from_secs(3));
    });
}

/// Sends one request to a provider's socket.
pub fn send(name: &str, socket_path: &Path, request: &str) -> String {
    match socket::request(socket_path, request) {
        Ok(answer) => answer,
        Err(e) => format!("error: {name} provider: {e}"),
    }
}
