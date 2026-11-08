//! Hooks: programs that the core runs when its state changes, so that other
//! devices can follow Kuterlapa. Example: pause the TV while Kuterlapa
//! listens, and play again when it is quiet.
//!
//! In config.toml:
//!
//!   [hooks]
//!   commands = ["~/bin/tv-hook"]
//!
//! The core runs each command with one argument, the event:
//!
//!   listening-start, listening-stop   the speech provider records
//!   thinking-start, thinking-stop     a model works on a chat answer
//!   speaking-start, speaking-stop     the voice provider reads aloud
//!   silence                           the user asked for silence
//!
//! The environment holds the whole state when the hook runs:
//!
//!   KUTERLAPA_EVENT        the event again
//!   KUTERLAPA_LISTENING    "1" or "0"
//!   KUTERLAPA_THINKING     "1" or "0"
//!   KUTERLAPA_SPEAKING     "1" or "0"
//!   KUTERLAPA_VOICE_MODE   "off", "read" or "talk"
//!
//! Hooks run on their own thread, one at a time, so a slow hook never
//! delays the core. A stop event runs STOP_DELAY late, with the state of
//! that moment. When the user starts to speak again within that time, the
//! hook sees KUTERLAPA_LISTENING=1 and can leave things as they are.

use crate::paths;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const STOP_DELAY: Duration = Duration::from_millis(1500);
/// A hook that runs longer is killed, so it cannot block the hooks after it.
const HOOK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy)]
pub enum Flag {
    Listening,
    Thinking,
    Speaking,
}

impl Flag {
    fn name(self) -> &'static str {
        match self {
            Flag::Listening => "listening",
            Flag::Thinking => "thinking",
            Flag::Speaking => "speaking",
        }
    }

    pub fn from_name(name: &str) -> Option<Flag> {
        match name {
            "listening" => Some(Flag::Listening),
            "thinking" => Some(Flag::Thinking),
            "speaking" => Some(Flag::Speaking),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Flags {
    listening: bool,
    thinking: bool,
    speaking: bool,
}

impl Flags {
    fn slot(&mut self, flag: Flag) -> &mut bool {
        match flag {
            Flag::Listening => &mut self.listening,
            Flag::Thinking => &mut self.thinking,
            Flag::Speaking => &mut self.speaking,
        }
    }
}

struct Job {
    event: String,
    due: Instant,
}

struct Hooks {
    jobs: Mutex<Sender<Job>>,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();
static FLAGS: Mutex<Flags> = Mutex::new(Flags { listening: false, thinking: false, speaking: false });

/// Starts the hook thread. Without commands, hooks do nothing.
pub fn start(commands: Vec<String>) {
    if commands.is_empty() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    if HOOKS.set(Hooks { jobs: Mutex::new(sender) }).is_err() {
        return;
    }
    thread::spawn(move || {
        let mut pending: Vec<Job> = Vec::new();
        loop {
            let wait = pending
                .iter()
                .map(|j| j.due.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(Duration::from_secs(3600));
            match receiver.recv_timeout(wait) {
                Ok(job) => pending.push(job),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            // Due jobs run in the order they fall due. A start event is due
            // at once, so it runs before a stop event that waits.
            let now = Instant::now();
            let (mut due, rest): (Vec<Job>, Vec<Job>) = pending.drain(..).partition(|j| j.due <= now);
            pending = rest;
            due.sort_by_key(|j| j.due);
            for job in due {
                for command in &commands {
                    run(command, &job.event);
                }
            }
        }
    });
}

/// Records a change of state and runs the hooks for it.
pub fn set(flag: Flag, on: bool) {
    {
        let mut flags = FLAGS.lock().unwrap();
        let slot = flags.slot(flag);
        if *slot == on {
            return;
        }
        *slot = on;
    }
    let event = format!("{}-{}", flag.name(), if on { "start" } else { "stop" });
    let delay = if on { Duration::ZERO } else { STOP_DELAY };
    queue(event, delay);
}

/// Runs the hooks for an event that is not a change of state.
pub fn event(name: &str) {
    queue(name.to_string(), Duration::ZERO);
}

fn queue(event: String, delay: Duration) {
    eprintln!("hook event: {event}");
    if let Some(hooks) = HOOKS.get() {
        let _ = hooks.jobs.lock().unwrap().send(Job { event, due: Instant::now() + delay });
    }
}

fn voice_mode() -> String {
    let mode = std::fs::read_to_string(paths::state_dir().join("voice-mode")).unwrap_or_default();
    match mode.trim() {
        "read" | "talk" => mode.trim().to_string(),
        _ => "off".into(),
    }
}

fn run(command: &str, event: &str) {
    let flags = *FLAGS.lock().unwrap();
    let bit = |on: bool| if on { "1" } else { "0" };
    let program = paths::expand(command);
    let started = Instant::now();
    // The command is a shell command line, so it may carry arguments.
    let child = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", program.display()))
        .arg("sh")
        .arg(event)
        .env("KUTERLAPA_EVENT", event)
        .env("KUTERLAPA_LISTENING", bit(flags.listening))
        .env("KUTERLAPA_THINKING", bit(flags.thinking))
        .env("KUTERLAPA_SPEAKING", bit(flags.speaking))
        .env("KUTERLAPA_VOICE_MODE", voice_mode())
        .stdin(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => {
            eprintln!("hook {command}: cannot start: {e}");
            return;
        }
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    eprintln!("hook {command} {event}: {status}");
                }
                return;
            }
            Ok(None) if started.elapsed() > HOOK_TIMEOUT => {
                eprintln!("hook {command} {event}: no answer in {HOOK_TIMEOUT:?}, killing it");
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                eprintln!("hook {command} {event}: {e}");
                return;
            }
        }
    }
}
