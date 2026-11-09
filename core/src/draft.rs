//! The draft: text that the user dictates into the overlay instead of into
//! an application, edits by voice with the current model, and then sends.
//!
//!   draft button (X), first press     start recording into a new draft
//!   draft button, while recording     stop; the first speech becomes the
//!                                     draft, later speech goes to the model
//!                                     as an edit or as more text
//!   finish (A)                        type the draft into the application
//!                                     and send it, as that application
//!                                     wants (see submit.rs)
//!   erase (B)                         drop the recording, or the draft

use crate::profiles::{self, Profile};
// `profiles::Current` is shared with the chat.
use crate::{engines, shell, speech, submit};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const EDIT_PROMPT: &str = "Edytujesz szkic tekstu. Użytkownik dyktuje go głosem i wyśle w innej \
aplikacji. Dostajesz obecny szkic i nową wypowiedź użytkownika. Wypowiedź jest albo poleceniem \
edycji (na przykład „zamień X na Y”, „krócej”, „zrób z tego listę”, „przetłumacz na angielski”), \
albo dalszym tekstem do dopisania na końcu. Rozpoznanie mowy bywa niedokładne: popraw oczywiste \
pomyłki. Zwróć wyłącznie nowy, pełny szkic: bez komentarzy, bez cudzysłowów, bez bloków kodu.";

#[derive(Default)]
struct State {
    text: String,
    /// Speech of the current recording, not applied yet.
    pending: String,
    recording: bool,
    /// The model is editing the draft.
    busy: bool,
    error: Option<String>,
    /// A short message under the title, for example after a profile
    /// change. It does not replace the draft on screen.
    note: Option<String>,
}

pub struct Draft {
    state: Mutex<State>,
    profiles: Arc<profiles::Current>,
}

impl Draft {
    pub fn new(profiles: Arc<profiles::Current>) -> Draft {
        Draft { state: Mutex::new(State::default()), profiles }
    }

    /// True while dictated text belongs to the draft, not to the
    /// application in front.
    pub fn takes_dictation(&self) -> bool {
        self.state.lock().unwrap().recording
    }

    /// True while there is a draft or a recording for one.
    pub fn active(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.recording || !s.text.is_empty() || s.busy
    }

    pub fn dictated(&self, text: &str) {
        self.state.lock().unwrap().pending.push_str(text);
        self.show();
    }

    /// The draft button: start a recording for the draft, or stop it and
    /// apply what was said.
    pub fn button(&self) {
        let recording = {
            let mut s = self.state.lock().unwrap();
            if s.busy {
                return;
            }
            if !s.recording {
                s.recording = true;
                s.pending.clear();
                s.error = None;
            }
            // Before the change above: was it recording already?
            !s.pending.is_empty() || self.provider_recording()
        };
        if recording {
            // Waits until the provider has sent the last words.
            speech::send("finish");
            self.apply_recording();
        } else {
            speech::send("toggle");
            self.show();
        }
    }

    /// Whether the provider records right now. "finish" would stop it, so
    /// ask with "status".
    fn provider_recording(&self) -> bool {
        speech::send("status") == "recording"
    }

    /// Stop a running recording and apply its speech to the draft.
    fn apply_recording(&self) {
        let (draft, pending) = {
            let mut s = self.state.lock().unwrap();
            s.recording = false;
            let pending = std::mem::take(&mut s.pending).trim().to_string();
            if pending.is_empty() {
                drop(s);
                self.show();
                return;
            }
            if s.text.is_empty() {
                s.text = pending;
                drop(s);
                self.show();
                return;
            }
            s.busy = true;
            (s.text.clone(), pending)
        };
        self.show();
        let profile = self.profile();
        let user = format!("Szkic:\n<<<\n{draft}\n>>>\n\nWypowiedź użytkownika:\n<<<\n{pending}\n>>>");
        let result = engines::complete(&profile, EDIT_PROMPT, &user);
        {
            let mut s = self.state.lock().unwrap();
            s.busy = false;
            match result {
                Ok(text) if !text.is_empty() => s.text = text,
                Ok(_) => s.error = Some("model zwrócił pusty tekst".into()),
                // Discarded with B while the model worked: nothing to show.
                Err(e) if engines::is_cancelled(&e) => {}
                Err(e) => {
                    eprintln!("draft: {e:#}");
                    s.error = Some(format!("{e}"));
                }
            }
        }
        self.show();
    }

    /// The finish button with a draft: apply speech still recording, then
    /// type the draft and send it as the application in front wants.
    pub fn send(&self) -> String {
        if self.takes_dictation() {
            speech::send("finish");
            self.apply_recording();
        }
        let text = {
            let mut s = self.state.lock().unwrap();
            if s.busy {
                return "ok".into();
            }
            std::mem::take(&mut s.text)
        };
        self.hide();
        if text.is_empty() {
            return "ok".into();
        }
        let answer = shell::type_text(&text);
        if answer != "ok" {
            return answer;
        }
        match submit::key_for(&shell::frontmost()) {
            Some(key) => shell::key(&key),
            None => "ok".into(),
        }
    }

    /// The erase button with a draft: drop the recording, or else the draft.
    pub fn discard(&self) {
        let recording = self.takes_dictation();
        if recording {
            speech::send("cancel");
            let mut s = self.state.lock().unwrap();
            s.recording = false;
            s.pending.clear();
        } else {
            let mut s = self.state.lock().unwrap();
            if s.busy {
                engines::cancel_all();
            }
            *s = State::default();
        }
        self.show();
    }

    /// Shows a profile change. The daemon switched the profile already.
    pub fn profile_changed(self: &Arc<Self>, name: &str) {
        // A note above the draft, not a separate view: replacing the view
        // made the draft vanish for a moment, which looked like the model
        // was writing it again.
        let note = format!("Profil: {name}");
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

    fn profile(&self) -> Profile {
        self.profiles.get()
    }

    fn hide(&self) {
        shell::overlay(&json!({ "visible": false }));
    }

    /// Show the overlay for the current state, or hide it when there is
    /// nothing to show.
    /// Shows the draft again, for example after the help covered it.
    pub fn refresh(&self) {
        self.show();
    }

    fn show(&self) {
        // A panel covers the overlay; this view shows when it closes.
        if crate::help::PANEL.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let profile = self.profile().name;
        let s = self.state.lock().unwrap();
        let draft = s.recording || s.busy || !s.text.is_empty() || s.error.is_some();
        if !draft && s.note.is_none() {
            drop(s);
            return self.hide();
        }
        if !draft {
            // Only a note, no draft: a small view with the note alone.
            let state = json!({
                "visible": true, "title": s.note.clone().unwrap_or_default(), "status": "", "text": "",
                "hint": "L: następny profil",
            });
            drop(s);
            shell::overlay(&state);
            return;
        }
        let status = if let Some(note) = &s.note {
            note.clone()
        } else if s.recording {
            "Słucham…".to_string()
        } else if s.busy {
            format!("{profile} poprawia szkic…")
        } else if let Some(e) = &s.error {
            format!("Błąd: {e}")
        } else {
            String::new()
        };
        let mut text = s.text.clone();
        if !s.pending.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n→ ");
            }
            text.push_str(s.pending.trim());
        }
        let hint = if s.text.is_empty() && s.recording {
            "X: koniec · A: wyślij · B: anuluj"
        } else {
            "X: popraw głosem · A: wyślij · B: odrzuć"
        };
        let state = json!({
            "visible": true, "title": format!("Szkic · {profile}"), "status": status, "text": text, "hint": hint,
        });
        drop(s);
        shell::overlay(&state);
    }
}
