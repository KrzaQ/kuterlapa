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
}

pub struct Draft {
    state: Mutex<State>,
    profiles: Mutex<(Vec<Profile>, usize)>,
}

impl Draft {
    pub fn new() -> Draft {
        let list = profiles::load();
        let current = profiles::current(&list);
        Draft { state: Mutex::new(State::default()), profiles: Mutex::new((list, current)) }
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
            *self.state.lock().unwrap() = State::default();
        }
        self.show();
    }

    pub fn next_profile(self: &Arc<Self>) {
        let name = {
            let mut p = self.profiles.lock().unwrap();
            p.0 = profiles::load();
            p.1 = (p.1 + 1) % p.0.len();
            profiles::save(&p.0[p.1]);
            p.0[p.1].name.clone()
        };
        eprintln!("profile: {name}");
        shell::overlay(&json!({
            "visible": true, "title": format!("Profil: {name}"), "status": "", "text": "", "hint": "L: następny profil",
        }));
        // Go back to the draft view, or hide, after a moment.
        let me = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            me.show();
        });
    }

    fn profile(&self) -> Profile {
        let p = self.profiles.lock().unwrap();
        p.0[p.1].clone()
    }

    fn hide(&self) {
        shell::overlay(&json!({ "visible": false }));
    }

    /// Show the overlay for the current state, or hide it when there is
    /// nothing to show.
    fn show(&self) {
        let profile = self.profile().name;
        let s = self.state.lock().unwrap();
        if !s.recording && !s.busy && s.text.is_empty() && s.error.is_none() {
            drop(s);
            return self.hide();
        }
        let status = if s.recording {
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
