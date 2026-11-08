//! The core service: turns buttons and dictated text into actions.

use crate::config::Config;
use crate::draft::Draft;
use crate::mapping::{Action, Mapping};
use crate::{paths, shell, socket, speech};
use anyhow::Result;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

struct State {
    mapping: Mutex<Mapping>,
    /// Text that dictation typed since the last other action. Erasing it
    /// is right only while the cursor stays at its end, so any other action
    /// forgets it.
    dictated: Mutex<String>,
    draft: Arc<Draft>,
}

pub fn run(config: Config) -> Result<()> {
    exit_when_stdin_closes();
    extend_path();
    speech::supervise(config.speech.command);
    let state = State {
        mapping: Mutex::new(Mapping::new()),
        dictated: Mutex::new(String::new()),
        draft: Arc::new(Draft::new()),
    };
    state.mapping.lock().unwrap().reload_if_changed();
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

/// An app started from Finder or by `open` gets a short PATH. Engines
/// (claude, pi) and their tools (node) live in the usual user places.
fn extend_path() {
    let home = paths::home();
    let mut dirs = vec![home.join(".local/bin"), "/opt/homebrew/bin".into(), "/usr/local/bin".into()];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Ok(joined) = std::env::join_paths(dirs) {
        std::env::set_var("PATH", joined);
    }
}

// No lock is held while the core waits for the speech provider: the
// provider sends "dictate" requests back while it finishes, and those need
// the dictation buffer.

fn handle(state: &State, line: &str) -> String {
    let (verb, argument) = line.split_once(' ').unwrap_or((line, ""));
    match verb {
        "button" => press(state, argument),
        "dictate" if state.draft.takes_dictation() => {
            state.draft.dictated(argument);
            "ok".into()
        }
        "dictate" => {
            let answer = shell::type_text(argument);
            if answer == "ok" {
                state.dictated.lock().unwrap().push_str(argument);
            }
            answer
        }
        "speech" => speech::send(argument),
        "type" => {
            forget_dictation(state);
            shell::type_text(argument)
        }
        "key" => {
            forget_dictation(state);
            shell::key(argument)
        }
        "status" => format!("shell: {}", shell::send("status")),
        _ => format!("error: unknown request {verb}"),
    }
}

fn forget_dictation(state: &State) {
    state.dictated.lock().unwrap().clear();
}

fn press(state: &State, button: &str) -> String {
    let action = state.mapping.lock().unwrap().action(button);
    let Some(action) = action else {
        eprintln!("{button}: no mapping");
        return "ok".into();
    };
    eprintln!("{button}: {action:?}");
    match action {
        Action::Key(combo) => {
            forget_dictation(state);
            shell::key(&combo)
        }
        Action::Speech(request) => speech::send(&request),
        Action::Command(command) => {
            forget_dictation(state);
            run_command(&command);
            "ok".into()
        }
        Action::Draft => {
            forget_dictation(state);
            state.draft.button();
            "ok".into()
        }
        Action::NextProfile => {
            state.draft.next_profile();
            "ok".into()
        }
        Action::EraseDictation { .. } if state.draft.active() => {
            state.draft.discard();
            "ok".into()
        }
        Action::FinishDictation { .. } if state.draft.active() => state.draft.send(),
        Action::EraseDictation { fallback } => {
            speech::send("cancel");
            let count = std::mem::take(&mut *state.dictated.lock().unwrap()).chars().count();
            if count > 0 {
                shell::erase(count)
            } else if let Some(key) = fallback {
                shell::key(&key)
            } else {
                "ok".into()
            }
        }
        Action::FinishDictation { then } => {
            let finished = speech::send("finish");
            if finished.starts_with("error") {
                eprintln!("{button}: {finished}");
            }
            forget_dictation(state);
            match then {
                Some(key) => shell::key(&key),
                None => "ok".into(),
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
