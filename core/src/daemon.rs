//! The core service: turns buttons and dictated text into actions.

use crate::approvals;
use crate::chat::{Chat, Mode};
use crate::help;
use crate::config::Config;
use crate::draft::Draft;
use crate::gestures::Recognizer;
use crate::hooks::{self, Flag};
use crate::mapping::{Action, Mapping};
use crate::profiles::Current;
use crate::store::Store;
use crate::{paths, shell, socket, speech, voice};
use anyhow::Result;
use serde_json::json;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

struct State {
    mapping: Mutex<Mapping>,
    /// Text that dictation typed since the last other action. Erasing it
    /// is right only while the cursor stays at its end, so any other action
    /// forgets it.
    dictated: Mutex<String>,
    draft: Arc<Draft>,
    chat: Arc<Chat>,
    profiles: Arc<Current>,
    gestures: OnceLock<Arc<Recognizer>>,
    /// While the help shows: its number, so an old timer does not hide a
    /// newer help.
    help: Mutex<Option<u64>>,
}

pub fn run(config: Config) -> Result<()> {
    exit_when_stdin_closes();
    extend_path();
    hooks::start(config.hooks.commands);
    speech::supervise(config.speech.command);
    voice::supervise(config.voice.command);
    let store = Arc::new(Store::open()?);
    let profiles = Arc::new(Current::new());
    let state = Arc::new(State {
        mapping: Mutex::new(Mapping::new()),
        dictated: Mutex::new(String::new()),
        draft: Arc::new(Draft::new(Arc::clone(&profiles))),
        chat: Arc::new(Chat::new(Arc::clone(&profiles), store)),
        profiles,
        gestures: OnceLock::new(),
        help: Mutex::new(None),
    });
    let for_gestures = Arc::clone(&state);
    let _ = state.gestures.set(Recognizer::new(move |button, gesture| {
        let key = gesture.key(button);
        let answer = act(&for_gestures, &key);
        if answer != "ok" {
            eprintln!("{key}: {answer}");
        }
    }));
    state.mapping.lock().unwrap().reload_if_changed();
    send_sticks(&state);
    // The overlay keeps the size the user asked for (computer_zoom).
    shell::send(&format!("overlay-scale {}", crate::computer::overlay_scale()));
    socket::serve(&paths::core_socket(), move |line| handle(&state, line))
}

/// Tell the platform shell what the analog sticks do. The shell moves the
/// pointer and scrolls by itself: it has to update many times a second.
fn send_sticks(state: &State) {
    let sticks = state.mapping.lock().unwrap().sticks().clone();
    let answer = shell::send(&format!("sticks {}", json!(sticks)));
    if answer != "ok" {
        eprintln!("sticks: {answer}");
    }
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

fn handle(state: &Arc<State>, line: &str) -> String {
    let (verb, argument) = line.split_once(' ').unwrap_or((line, ""));
    match verb {
        // A whole press, without gestures (the CLI uses it: `kuterlapa
        // press A`, or "kuterlapa press X double").
        "button" => act(state, argument),
        "press" | "release" => {
            let (has, changed) = {
                let mut mapping = state.mapping.lock().unwrap();
                let changed = mapping.reload_if_changed();
                (mapping.has(argument), changed)
            };
            if changed {
                send_sticks(state);
            }
            let gestures = state.gestures.get().expect("set at start");
            if verb == "press" {
                gestures.press(argument, has);
            } else {
                gestures.release(argument, has);
            }
            "ok".into()
        }
        "dictate" if state.draft.takes_dictation() => {
            state.draft.dictated(argument);
            "ok".into()
        }
        "dictate" if state.chat.takes_dictation() => {
            state.chat.dictated(argument);
            "ok".into()
        }
        "dictate" => {
            let answer = shell::type_text(argument);
            if answer == "ok" {
                state.dictated.lock().unwrap().push_str(argument);
            }
            answer
        }
        // A provider reports its state: "state listening on",
        // "state speaking off".
        "state" => {
            let (name, value) = argument.split_once(' ').unwrap_or((argument, ""));
            match (Flag::from_name(name), value) {
                (Some(flag), "on") => hooks::set(flag, true),
                (Some(flag), "off") => hooks::set(flag, false),
                _ => return format!("error: bad state {argument:?}"),
            }
            "ok".into()
        }
        // The MCP server asks before a tool with confirm = true. The answer
        // waits for the A or B button.
        "approve" => approvals::ask(argument, || state.chat.refresh()),
        // The speech provider counts down a pause in a chat message, and
        // finishes the message when the pause is long enough.
        "countdown" => {
            state.chat.countdown(argument.parse::<f64>().ok());
            "ok".into()
        }
        "auto-finish" => {
            // Sending waits for the model, and the provider must not wait.
            let chat = Arc::clone(&state.chat);
            thread::spawn(move || {
                chat.finish();
            });
            "ok".into()
        }
        // A typed chat message, for tests: it answers once the model has.
        "ask" => {
            state.chat.ask_text(argument);
            "ok".into()
        }
        "speech" => speech::send(argument),
        "say" => voice::send(&format!("say {argument}")),
        "voice" => voice::send(argument),
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

/// Runs the action for a key such as "X" or "X double".
fn act(state: &Arc<State>, button: &str) -> String {
    let action = state.mapping.lock().unwrap().action(button);
    let Some(action) = action else {
        eprintln!("{button}: no mapping");
        return "ok".into();
    };
    eprintln!("{button}: {action:?}");
    match action {
        // A question about a tool waits: A says yes, B says no.
        Action::FinishDictation { .. } if approvals::answer(true) => "ok".into(),
        Action::EraseDictation { .. } if approvals::answer(false) => "ok".into(),
        Action::Help => {
            if !hide_help(state) {
                show_help(state);
            }
            "ok".into()
        }
        Action::EraseDictation { .. } if hide_help(state) => "ok".into(),
        Action::Dictate if state.draft.active() => {
            state.draft.button();
            "ok".into()
        }
        Action::Dictate => speech::send("toggle"),
        Action::Mouse(what) => {
            forget_dictation(state);
            shell::send(&format!("mouse {what}"))
        }
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
        Action::NextProfile | Action::PreviousProfile => {
            let step = if action == Action::NextProfile { 1 } else { -1 };
            let name = state.profiles.step(step).name;
            eprintln!("profile: {name}");
            if state.draft.active() {
                state.draft.profile_changed(&name);
            } else {
                state.chat.profile_changed(&name);
            }
            "ok".into()
        }
        // A dictation for the draft or the chat already runs: the chat
        // buttons do not start a second one.
        Action::Chat | Action::Command1 if state.draft.takes_dictation() => "ok".into(),
        Action::Chat => {
            forget_dictation(state);
            state.chat.button(Mode::Conversation);
            "ok".into()
        }
        Action::Command1 => {
            forget_dictation(state);
            state.chat.button(Mode::Command);
            "ok".into()
        }
        Action::VoiceToggle => {
            state.chat.toggle_voice();
            "ok".into()
        }
        Action::Recall => {
            state.chat.recall();
            "ok".into()
        }
        Action::NewConversation => {
            state.chat.new_conversation();
            "ok".into()
        }
        Action::VoiceStop => {
            hooks::event("silence");
            voice::send("stop")
        }
        Action::EraseDictation { .. } if state.draft.active() => {
            state.draft.discard();
            "ok".into()
        }
        Action::FinishDictation { .. } if state.draft.active() => state.draft.send(),
        Action::EraseDictation { .. } if state.chat.active() => {
            state.chat.close();
            "ok".into()
        }
        Action::FinishDictation { .. } if state.chat.takes_dictation() => {
            state.chat.finish();
            "ok".into()
        }
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

/// How long the help stays without a button.
const HELP_TIME: std::time::Duration = std::time::Duration::from_secs(30);

fn show_help(state: &Arc<State>) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    *state.help.lock().unwrap() = Some(number);
    let overlay = help::overlay(&mut state.mapping.lock().unwrap());
    shell::overlay(&overlay);
    let state = Arc::clone(state);
    thread::spawn(move || {
        thread::sleep(HELP_TIME);
        if *state.help.lock().unwrap() == Some(number) {
            hide_help(&state);
        }
    });
}

/// Hides the help and shows what was there before. Returns false if the
/// help was not showing.
fn hide_help(state: &State) -> bool {
    if state.help.lock().unwrap().take().is_none() {
        return false;
    }
    if state.draft.active() {
        state.draft.refresh();
    } else {
        state.chat.refresh();
    }
    true
}

fn run_command(command: &str) {
    let result = Command::new("/bin/sh").arg("-c").arg(command).stdin(Stdio::null()).spawn();
    if let Err(e) = result {
        eprintln!("cannot run {command:?}: {e}");
    }
}
