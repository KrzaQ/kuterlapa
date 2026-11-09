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
use crate::memory;
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
    /// While the user pauses: when the message sends itself (Unix time in
    /// seconds) and how long the countdown is.
    countdown: Option<(f64, f64)>,
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
            // The recording ends by itself after a pause (the speech
            // provider counts it down), or with the button.
            speech::send("start-auto");
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
        let question = {
            let mut s = self.state.lock().unwrap();
            s.recording = false;
            s.countdown = None;
            std::mem::take(&mut s.pending)
        };
        self.send(&question);
    }

    /// Sends a typed message to the conversation, without speech (`kuterlapa
    /// ask <text>`, for tests). Ignored while the chat records or thinks.
    pub fn ask_text(&self, text: &str) {
        {
            let mut s = self.state.lock().unwrap();
            if s.recording || s.busy {
                return;
            }
            s.mode = Mode::Conversation;
            s.thread = None;
            s.visible = true;
            s.error = None;
        }
        self.send(text);
    }

    fn send(&self, question: &str) {
        let (mode, question, thread) = {
            let mut s = self.state.lock().unwrap();
            let question = question.trim().to_string();
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
        crate::view::changed();
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
                // Cancelled with B: say nothing, the overlay is closed.
                Err(e) if engines::is_cancelled(&e) => None,
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
        sessions::touch(&session.name);
        let profile = self.profiles.get();
        let voice = self.state.lock().unwrap().voice;
        let system = system_prompt(mode, &session, voice);
        match mode {
            Mode::Conversation => {
                let conversation = self.store.current_conversation(&session.name)?;
                let history = self.store.messages(conversation, HISTORY)?;
                self.store.add_message(conversation, "user", question, None)?;
                crate::view::changed();
                let answer =
                    engines::complete_in(&profile, &system, &with_history(&history, question), session.folder.as_deref(), Some(&session.name))?;
                self.store.add_message(conversation, "assistant", &answer, Some(&profile.name))?;
                Ok((answer, None))
            }
            Mode::Command => {
                let history = match thread {
                    Some(t) => self.store.thread(t)?,
                    None => Vec::new(),
                };
                let thread = self.store.add_command(thread, Some(&session.name), "user", question, None)?;
                crate::view::changed();
                let answer =
                    engines::complete_in(&profile, &system, &with_history(&history, question), session.folder.as_deref(), Some(&session.name))?;
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
            s.countdown = None;
            s.pending.clear();
            if s.answer.is_empty() && s.error.is_none() {
                s.visible = false;
            }
        } else {
            voice::send("stop");
            let mut s = self.state.lock().unwrap();
            if s.busy {
                // Stop the model and its tools, not only the view.
                engines::cancel_all();
            }
            s.visible = false;
            s.thread = None;
        }
        self.show();
    }

    /// The last two messages of the session's conversation, shortened, as
    /// hints for speech recognition.
    pub fn recent_text(&self) -> String {
        let session = sessions::current_name();
        let Ok(conversation) = self.store.current_conversation(&session) else { return String::new() };
        self.store
            .messages(conversation, 2)
            .unwrap_or_default()
            .iter()
            .map(|m| m.text.chars().take(300).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Hides the conversation without ending anything: the user dictates
    /// into an application now. A model still at work finishes, and recall
    /// shows its answer.
    pub fn dismiss(&self) {
        voice::send("stop");
        let visible = {
            let mut s = self.state.lock().unwrap();
            std::mem::replace(&mut s.visible, false)
        };
        if visible {
            self.show();
        }
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
    /// The new-conversation button. While a command's answer shows, it
    /// ends only that command's thread, so the next command starts fresh
    /// and the session's conversation stays. Otherwise it starts a new
    /// conversation in the session.
    pub fn new_conversation(self: &Arc<Self>) {
        {
            let mut s = self.state.lock().unwrap();
            if s.recording || s.busy {
                return;
            }
            if s.visible && s.mode == Mode::Command {
                s.thread = None;
                s.visible = false;
                s.question.clear();
                s.answer.clear();
                s.error = None;
                drop(s);
                voice::send("stop");
                self.flash("Nowe polecenie");
                return;
            }
        }
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

    /// The window in front picked another session. The note stays longer
    /// than others: the user must see that the context changed.
    pub fn session_changed(self: &Arc<Self>, old: &str, new: &str) {
        self.flash_for(
            &format!("Sesja: {old} → {new} (z okna) · cofnięcie: „wróć do poprzedniej sesji”"),
            Duration::from_secs(5),
        );
    }

    pub fn profile_changed(self: &Arc<Self>, name: &str) {
        self.flash(&format!("Profil: {name}"));
    }

    /// A note for two seconds, above the answer if one is shown.
    fn flash(self: &Arc<Self>, note: &str) {
        self.flash_for(note, Duration::from_secs(2));
    }

    fn flash_for(self: &Arc<Self>, note: &str, time: Duration) {
        let note = note.to_string();
        self.state.lock().unwrap().note = Some(note.clone());
        self.show();
        let me = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(time);
            let mut s = me.state.lock().unwrap();
            if s.note.as_deref() == Some(note.as_str()) {
                s.note = None;
                drop(s);
                me.show();
            }
        });
    }

    /// The speech provider heard a pause: the message sends itself in
    /// `seconds`, unless the user speaks again (None).
    pub fn countdown(&self, seconds: Option<f64>) {
        {
            let mut s = self.state.lock().unwrap();
            if !s.recording {
                return;
            }
            s.countdown = seconds.map(|total| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or_default();
                (now + total, total)
            });
        }
        self.show();
    }

    /// Shows the overlay as the chat state says, for example after a
    /// question about a tool covered it.
    pub fn refresh(&self) {
        self.show();
    }

    fn show(&self) {
        // A panel covers the overlay; this view shows when it closes.
        if crate::help::PANEL.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let profile = self.profiles.get().name;
        let session = sessions::current_name();
        let s = self.state.lock().unwrap();
        // The chat view shows the conversation itself; the overlay only
        // shows a recording (what the user says, the pause countdown).
        if crate::view::is_open() && !s.recording && s.note.is_none() {
            drop(s);
            shell::overlay(&json!({ "visible": false }));
            return;
        }
        if !s.visible && s.note.is_none() {
            drop(s);
            shell::overlay(&json!({ "visible": false }));
            return;
        }
        // A badge says what kind of exchange this is; a one-off command is
        // temporary, so the overlay draws a dashed frame around it.
        let (badge, tone) = match (s.mode, s.thread.is_some()) {
            (Mode::Conversation, _) => ("ROZMOWA", "conversation"),
            (Mode::Command, true) => ("POLECENIE · CIĄG DALSZY", "command"),
            (Mode::Command, false) => ("POLECENIE · JEDNORAZOWE", "command"),
        };
        let speaker = match s.voice {
            VoiceMode::Off => "",
            VoiceMode::Read => " · 🔊",
            VoiceMode::Talk => " · 🗣",
        };
        let picked = if sessions::automatic() { " (auto)" } else { "" };
        let title = format!("{session}{picked} · {profile}{speaker}");
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
        } else if s.mode == Mode::Command && s.thread.is_some() {
            "Y Y: dopytaj · +: nowe polecenie · Y: rozmowa · B: zamknij"
        } else if s.mode == Mode::Command {
            "Y Y: nowe polecenie · Y: rozmowa · B: zamknij"
        } else {
            "Y: dalej · Y Y: polecenie · B: zamknij"
        };
        let mut state = json!({
            "visible": true, "title": title, "status": status, "text": text, "hint": hint,
            "badge": { "text": badge, "tone": tone },
        });
        if s.mode == Mode::Command {
            state["frame"] = json!("dashed");
        }
        if let Some((ends, total)) = s.countdown {
            state["countdown"] = json!({ "ends": ends, "total": total });
        }
        drop(s);
        shell::overlay(&state);
    }
}

fn system_prompt(mode: Mode, session: &sessions::Session, voice: VoiceMode) -> String {
    let mut p = String::from(
        "Jesteś asystentem w Kuterłapie. Użytkownik pracuje głosem: dyktuje, a rozpoznawanie mowy bywa \
         niedokładne, więc domyślaj się sensu. Odpowiadaj krótko i konkretnie, po polsku, chyba że \
         użytkownik prosi inaczej. Odpowiedź pojawi się w małym oknie w rogu ekranu. Możesz mieć \
         narzędzia do sterowania urządzeniami i programami użytkownika. Działaj nimi, gdy użytkownik o \
         to prosi. Każdą czynność wykonuj raz; gdy się udała, nie powtarzaj jej i zakończ odpowiedzią. Nigdy nie twierdź, że coś zrobiłeś, jeśli nie zrobiło tego narzędzie. Gdy nie masz \
         narzędzia do prośby, powiedz to wprost.",
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
    p.push_str(
        "\n\nSesje: zmieniaj sesję (session_switch) tylko, gdy użytkownik wprost o to prosi, np. „przejdź do \
         sesji X” albo „wejdź do iTerma, do sesji X”. Samo przejście do okna, karty czy aplikacji sesji nie \
         zmienia: w trybie automatycznym zrobi to okno, gdy użytkownik zacznie mówić. Gdy użytkownik mówi, \
         że okno na wierzchu należy do jakiejś sesji, użyj session_bind ze stałym fragmentem tytułu (nazwa \
         arkusza, projektu, kanału; nie liczby powiadomień). „Wróć do poprzedniej sesji”: session_back. Nową sesję twórz \
         (session_create) tylko na wyraźną prośbę użytkownika; gdy sesji, do której chce przejść, nie ma, \
         zaproponuj utworzenie i zaczekaj na zgodę.",
    );
    p.push_str(&memory::prompt(&session.name));
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
