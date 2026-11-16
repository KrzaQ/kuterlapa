//! Which microphone the speech provider records from: the system's default
//! input, or one the user chose (the "microphone" action). The speech
//! provider keeps the choice in ~/.local/state/kuterlapa/microphone; when
//! the chosen one is not there, it records from the system's and says so.

use crate::{approvals, paths, speech};
use lejce::{Answer, Question};
use serde::Deserialize;

#[derive(Deserialize)]
struct Devices {
    system: String,
    chosen: String,
    inputs: Vec<String>,
}

/// The chosen microphone, or None when it follows the system.
pub fn chosen() -> Option<String> {
    let name = std::fs::read_to_string(paths::state_dir().join("microphone")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Asks which microphone to use, with the d-pad, and tells the speech
/// provider. Returns a note for the overlay. `restore` shows the overlay as
/// it was before the question.
pub fn choose(restore: impl Fn()) -> String {
    let answer = speech::send("devices");
    let devices: Devices = match serde_json::from_str(&answer) {
        Ok(d) => d,
        Err(_) => return format!("Nie mam listy mikrofonów: {answer}"),
    };
    let mark = |on: bool| if on { " ✓" } else { "" };
    let mut options = vec![format!("Systemowy (teraz: {}){}", devices.system, mark(devices.chosen.is_empty()))];
    // The first row is the system's; the rest fit in what the overlay shows.
    let inputs: Vec<&String> = devices.inputs.iter().take(approvals::MAX_OPTIONS - 1).collect();
    options.extend(inputs.iter().map(|name| format!("{name}{}", mark(**name == devices.chosen))));
    let question = Question {
        tool: None,
        text: "Z którego mikrofonu nagrywać?".into(),
        detail: String::new(),
        options,
        multiple: false,
    };
    match approvals::show("Mikrofon", &question, restore) {
        Some(Answer::Chose(picked)) => {
            let name = match picked.first() {
                Some(0) | None => String::new(),
                Some(&i) => inputs[i - 1].clone(),
            };
            let done = speech::send(&format!("microphone {name}"));
            if done.starts_with("error") {
                return format!("Nie zmieniłem mikrofonu: {done}");
            }
            if name.is_empty() {
                format!("Mikrofon: systemowy ({})", devices.system)
            } else {
                format!("Mikrofon: {name}")
            }
        }
        Some(_) => String::new(),
        None => "Najpierw odpowiedz na pytanie, które czeka".into(),
    }
}
