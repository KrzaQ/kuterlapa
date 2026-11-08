//! The core service: turns buttons and dictated text into actions.

use crate::config::Config;
use crate::mapping::{Action, Mapping};
use crate::{paths, shell, socket, speech};
use anyhow::Result;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;

struct State {
    mapping: Mapping,
    /// Text that dictation typed since the last other action. Erasing it
    /// is right only while the cursor stays at its end, so any other action
    /// forgets it.
    dictated: String,
}

pub fn run(config: Config) -> Result<()> {
    exit_when_stdin_closes();
    speech::supervise(config.speech.command);
    let state = Mutex::new(State { mapping: Mapping::new(), dictated: String::new() });
    state.lock().unwrap().mapping.reload_if_changed();
    socket::serve(&paths::core_socket(), move |line| handle(&state, line))
}

/// The platform shell starts the core with a pipe on stdin. When the shell
/// exits, the pipe closes, and the core exits with it instead of lingering.
/// Started from a terminal, stdin is the terminal, and Ctrl+D ends the core.
fn exit_when_stdin_closes() {
    thread::spawn(|| {
        let mut buffer = [0u8; 64];
        let mut stdin = std::io::stdin();
        while matches!(stdin.read(&mut buffer), Ok(n) if n > 0) {}
        eprintln!("stdin closed, exiting");
        std::process::exit(0);
    });
}

fn handle(state: &Mutex<State>, line: &str) -> String {
    let (verb, argument) = line.split_once(' ').unwrap_or((line, ""));
    let mut state = state.lock().unwrap();
    match verb {
        "button" => press(&mut state, argument),
        "dictate" => {
            let answer = shell::type_text(argument);
            if answer == "ok" {
                state.dictated.push_str(argument);
            }
            answer
        }
        "speech" => speech::send(argument),
        "type" => {
            state.dictated.clear();
            shell::type_text(argument)
        }
        "key" => {
            state.dictated.clear();
            shell::key(argument)
        }
        "status" => format!("shell: {}", shell::send("status")),
        _ => format!("error: unknown request {verb}"),
    }
}

fn press(state: &mut State, button: &str) -> String {
    let Some(action) = state.mapping.action(button) else {
        eprintln!("{button}: no mapping");
        return "ok".into();
    };
    eprintln!("{button}: {action:?}");
    match action {
        Action::Key(combo) => {
            state.dictated.clear();
            shell::key(&combo)
        }
        Action::Speech(request) => speech::send(&request),
        Action::Command(command) => {
            state.dictated.clear();
            run_command(&command);
            "ok".into()
        }
        Action::EraseDictation { fallback } => {
            speech::send("cancel");
            let count = state.dictated.chars().count();
            state.dictated.clear();
            if count > 0 {
                shell::erase(count)
            } else if let Some(key) = fallback {
                shell::key(&key)
            } else {
                "ok".into()
            }
        }
    }
}

fn run_command(command: &str) {
    let result = Command::new("/bin/sh").arg("-c").arg(command).stdin(Stdio::null()).spawn();
    if let Err(e) = result {
        eprintln!("cannot run {command:?}: {e}");
    }
}
