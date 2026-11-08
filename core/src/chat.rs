//! Talking to the model: a conversation in the current session, or a
//! one-off command.
//!
//!   chat button (Y), first press       record a message to the session's
//!                                      conversation
//!   command button (Y double)          record a one-off command; it sees
//!                                      the session's context but is not
//!                                      saved in its conversation
//!   the same button again, or finish   stop and send to the current model
//!   erase (B)                          drop the recording, or close the
//!                                      answer (which also ends a command
//!                                      thread)
//!   recall (ZR)                        show the last answer again
//!   voice toggle (Y hold)              cycle the voice mode: off, read the
//!                                      answer, talk (short spoken answers)
//!
//! While a command's answer is on screen, the next command continues it
//! as a thread. Closing the answer, or a long pause, starts a new thread.

use crate::hooks::{self, Flag};
use crate::profiles::Current;
use crate::sessions;
use crate::store::{Message, Store};
use crate::{engines, paths, shell, speech, voice};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const THREAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const HISTORY: usize = 30;

/// How the narrator voice takes part.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VoiceMode {
    /// Answers only on screen.
    #[default]
    Off,
    /// The narrator reads every answer as the model wrote it.
    Read,
    /// The narrator reads every answer, and the model knows it: it answers
    /// briefly and in spoken style, for a flowing conversation.
    Talk,
}

impl VoiceMode {
    fn next(self) -> VoiceMode {
        match self {
            VoiceMode::Off => VoiceMode::Read,
            VoiceMode::Read => VoiceMode::Talk,
            VoiceMode::Talk => VoiceMode::Off,
        }
    }

    fn name(self) -> &'static str {
        match self {
            VoiceMode::Off => "off",
            VoiceMode::Read => "read",
            VoiceMode::Talk => "talk",
        }
    }

    fn from_name(name: &str) -> VoiceMode {
        match name.trim() {
            "read" => VoiceMode::Read,
            "talk" => VoiceMode::Talk,
            _ => VoiceMode::Off,
        }
    }

    /// Kept in ~/.local/state/kuterlapa/voice-mode across restarts.
    fn load() -> VoiceMode {
        VoiceMode::from_name(&std::fs::read_to_string(paths::state_dir().join("voice-mode")).unwrap_or_default())
    }

    fn save(self) {
        let _ = std::fs::write(paths::state_dir().join("voice-mode"), self.name());
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Conversation,
    Command,
}

#[derive(Default)]
struct State {
    mode: Mode,
    recording: bool,
    pending: String,
    busy: bool,
    question: String,
    answer: String,
    error: Option<String>,
    visible: bool,
    thread: Option<i64>,
    last_activity: Option<Instant>,
    voice: VoiceMode,
    note: Option<String>,
}

pub struct Chat {
    state: Mutex<State>,
    profiles: Arc<Current>,
    store: Arc<Store>,
}

impl Chat {
    pub fn new(profiles: Arc<Current>, store: Arc<Store>) -> Chat {
        let state = State { voice: VoiceMode::load(), ..State::default() };
        Chat { state: Mutex::new(state), profiles, store }
    }

    pub fn takes_dictation(&self) -> bool {
        self.state.lock().unwrap().recording
    }

    /// True while the chat is recording, thinking or showing an answer.
    pub fn active(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.recording || s.busy || s.visible
    }

    pub fn dictated(&self, text: &str) {
        self.state.lock().unwrap().pending.push_str(text);
        self.show();
    }

    /// The chat or command button: start recording, or stop and send.
    pub fn button(&self, mode: Mode) {
        let start = {
            let mut s = self.state.lock().unwrap();
            if s.busy {
                return;
            }
            if s.recording {
                false
            } else {
                // A command continues the thread only while its answer is
                // still on screen and the thread is fresh.
                let fresh = s.last_activity.is_some_and(|t| t.elapsed() < THREAD_TIMEOUT);
                if mode != Mode::Command || s.mode != Mode::Command || !s.visible || !fresh {
                    s.thread = None;
                }
                s.mode = mode;
                s.recording = true;
                s.pending.clear();
                s.error = None;
                s.visible = true;
                true
            }
        };
        if start {
            voice::send("stop");
            speech::send("toggle");
            self.show();
        } else {
            self.send_recording();
        }
    }

    /// The finish button while recording: stop and send. Returns false if
    /// the chat was not recording.
    pub fn finish(&self) -> bool {
        if !self.takes_dictation() {
            return false;
        }
        self.send_recording();
        true
    }

    fn send_recording(&self) {
        // Waits until the provider has sent the last words.
        speech::send("finish");
        let (mode, question, thread) = {
            let mut s = self.state.lock().unwrap();
            s.recording = false;
            let question = std::mem::take(&mut s.pending).trim().to_string();
            if question.is_empty() {
                drop(s);
                return self.show();
            }
            s.busy = true;
            s.question = question.clone();
            s.answer.clear();
            (s.mode, question, s.thread)
        };
        self.show();
        hooks::set(Flag::Thinking, true);
        let result = self.ask(mode, &question, thread);
        let speak = {
            let mut s = self.state.lock().unwrap();
            s.busy = false;
            s.last_activity = Some(Instant::now());
            match result {
                Ok((answer, thread)) => {
                    s.thread = thread;
                    s.answer = answer;
                    (s.voice != VoiceMode::Off).then(|| s.answer.clone())
                }
                Err(e) => {
                    eprintln!("chat: {e:#}");
                    s.error = Some(format!("{e}"));
                    None
                }
            }
        };
        self.show();
        if let Some(answer) = speak {
            voice::send(&format!("say {}", for_speech(&answer)));
        }
        // After "say": the voice provider reports speaking before this
        // late stop event runs, so hooks see no quiet gap in between.
        hooks::set(Flag::Thinking, false);
    }

    /// Asks the current model. Returns the answer and, for a command, its
    /// thread.
    fn ask(&self, mode: Mode, question: &str, thread: Option<i64>) -> anyhow::Result<(String, Option<i64>)> {
        let session = sessions::current();
        let profile = self.profiles.get();
        let voice = self.state.lock().unwrap().voice;
        let system = system_prompt(mode, &session, voice);
        match mode {
            Mode::Conversation => {
                let conversation = self.store.current_conversation(&session.name)?;
                let history = self.store.messages(conversation, HISTORY)?;
                self.store.add_message(conversation, "user", question, None)?;
                let answer =
                    engines::complete_in(&profile, &system, &with_history(&history, question), session.folder.as_deref())?;
                self.store.add_message(conversation, "assistant", &answer, Some(&profile.name))?;
                Ok((answer, None))
            }
            Mode::Command => {
                let history = match thread {
                    Some(t) => self.store.thread(t)?,
                    None => Vec::new(),
                };
                let thread = self.store.add_command(thread, Some(&session.name), "user", question, None)?;
                let answer =
                    engines::complete_in(&profile, &system, &with_history(&history, question), session.folder.as_deref())?;
                self.store.add_command(Some(thread), Some(&session.name), "assistant", &answer, Some(&profile.name))?;
                Ok((answer, Some(thread)))
            }
        }
    }

    /// The erase button: drop the recording, or close the answer.
    pub fn close(&self) {
        let recording = self.takes_dictation();
        if recording {
            speech::send("cancel");
            let mut s = self.state.lock().unwrap();
            s.recording = false;
            s.pending.clear();
            if s.answer.is_empty() && s.error.is_none() {
                s.visible = false;
            }
        } else {
            voice::send("stop");
            let mut s = self.state.lock().unwrap();
            s.visible = false;
            s.thread = None;
        }
        self.show();
    }

    /// Show the last answer again; from the history if there is none in
    /// memory (for example after a restart).
    pub fn recall(&self) {
        let empty = {
            let s = self.state.lock().unwrap();
            s.answer.is_empty() && s.question.is_empty()
        };
        if empty {
            let session = sessions::current();
            if let Ok(conversation) = self.store.current_conversation(&session.name) {
                if let Ok(messages) = self.store.messages(conversation, 2) {
                    let mut s = self.state.lock().unwrap();
                    s.mode = Mode::Conversation;
                    for m in messages {
                        if m.role == "user" {
                            s.question = m.text;
                        } else {
                            s.answer = m.text;
                        }
                    }
                }
            }
        }
        self.state.lock().unwrap().visible = true;
        self.show();
    }

    /// Start a new conversation in the current session. The next message
    /// (Y) goes there, without the earlier history.
    pub fn new_conversation(self: &Arc<Self>) {
        let session = sessions::current_name();
        if let Err(e) = self.store.new_conversation(&session) {
            eprintln!("new conversation: {e:#}");
            return;
        }
        voice::send("stop");
        {
            let mut s = self.state.lock().unwrap();
            if s.recording || s.busy {
                return;
            }
            s.question.clear();
            s.answer.clear();
            s.error = None;
            s.thread = None;
            s.mode = Mode::Conversation;
        }
        self.flash("Nowa rozmowa");
    }

    pub fn toggle_voice(self: &Arc<Self>) {
        let mode = {
            let mut s = self.state.lock().unwrap();
            s.voice = s.voice.next();
            s.voice
        };
        mode.save();
        if mode == VoiceMode::Off {
            voice::send("stop");
        }
        self.flash(match mode {
            VoiceMode::Off => "Lektor: wyłączony",
            VoiceMode::Read => "Lektor: czyta całe odpowiedzi",
            VoiceMode::Talk => "Lektor: rozmowa (krótkie odpowiedzi)",
        });
    }

    pub fn profile_changed(self: &Arc<Self>, name: &str) {
        self.flash(&format!("Profil: {name}"));
    }

    /// A note for two seconds, above the answer if one is shown.
    fn flash(self: &Arc<Self>, note: &str) {
        let note = note.to_string();
        self.state.lock().unwrap().note = Some(note.clone());
        self.show();
        let me = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            let mut s = me.state.lock().unwrap();
            if s.note.as_deref() == Some(note.as_str()) {
                s.note = None;
                drop(s);
                me.show();
            }
        });
    }

    fn show(&self) {
        let profile = self.profiles.get().name;
        let session = sessions::current_name();
        let s = self.state.lock().unwrap();
        if !s.visible && s.note.is_none() {
            drop(s);
            shell::overlay(&json!({ "visible": false }));
            return;
        }
        let kind = match (s.mode, s.thread.is_some() && s.recording) {
            (Mode::Conversation, _) => "Rozmowa",
            (Mode::Command, true) => "Ciąg dalszy polecenia",
            (Mode::Command, false) => "Polecenie",
        };
        let speaker = match s.voice {
            VoiceMode::Off => "",
            VoiceMode::Read => " · 🔊",
            VoiceMode::Talk => " · 🗣",
        };
        let title = format!("{kind} · {session} · {profile}{speaker}");
        let status = if let Some(note) = &s.note {
            note.clone()
        } else if s.recording {
            "Słucham…".into()
        } else if s.busy {
            format!("{profile} myśli…")
        } else if let Some(e) = &s.error {
            format!("Błąd: {e}")
        } else {
            String::new()
        };
        let mut text = String::new();
        if s.visible {
            if s.recording {
                text = s.pending.trim().to_string();
            } else {
                if !s.question.is_empty() {
                    text = format!("› {}", s.question);
                }
                if !s.answer.is_empty() {
                    text = format!("{text}\n\n{}", s.answer).trim().to_string();
                }
            }
        }
        let hint = if s.recording {
            "Y: wyślij · A: wyślij · B: anuluj"
        } else if s.mode == Mode::Command {
            "Y Y: dopytaj · Y: rozmowa · B: zamknij"
        } else {
            "Y: dalej · Y Y: polecenie · B: zamknij"
        };
        let state = json!({ "visible": true, "title": title, "status": status, "text": text, "hint": hint });
        drop(s);
        shell::overlay(&state);
    }
}

fn system_prompt(mode: Mode, session: &sessions::Session, voice: VoiceMode) -> String {
    let mut p = String::from(
        "Jesteś asystentem w Kuterłapie. Użytkownik pracuje głosem: dyktuje, a rozpoznawanie mowy bywa \
         niedokładne, więc domyślaj się sensu. Odpowiadaj krótko i konkretnie, po polsku, chyba że \
         użytkownik prosi inaczej. Odpowiedź pojawi się w małym oknie w rogu ekranu.",
    );
    // Read: the answer is read as written, so the prompt does not change.
    if voice == VoiceMode::Talk {
        p.push_str(
            " Lektor przeczyta Twoją odpowiedź na głos, a użytkownik prowadzi z Tobą płynną rozmowę: \
             streszczaj się, mów jak w rozmowie, zwykle jednym do trzech zdań, bez list, tabel i \
             bloków kodu.",
        );
    }
    match mode {
        Mode::Conversation => p.push_str(&format!(" To rozmowa w sesji „{}”.", session.name)),
        Mode::Command => p.push_str(" To jednorazowe polecenie: odpowiedz zwięźle."),
    }
    if !session.context.trim().is_empty() {
        p.push_str(&format!("\n\nKontekst sesji „{}”:\n{}", session.name, session.context.trim()));
    }
    p
}

fn with_history(history: &[Message], question: &str) -> String {
    if history.is_empty() {
        return question.to_string();
    }
    let mut text = String::from("Dotychczasowa rozmowa:\n\n");
    for m in history {
        let who = if m.role == "user" { "Użytkownik" } else { "Asystent" };
        text.push_str(&format!("{who}: {}\n\n", m.text));
    }
    text.push_str(&format!("Nowa wiadomość użytkownika:\n{question}"));
    text
}

/// Markdown read aloud sounds bad: drop code blocks and markup.
fn for_speech(text: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if !in_code {
                out.push_str("(kod pominięty) ");
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let line = line.trim_start_matches(|c| c == '#' || c == '>' || c == '-' || c == '*').trim();
        out.push_str(&line.replace(['*', '`', '_'], ""));
        out.push(' ');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
