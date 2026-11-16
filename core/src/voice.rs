//! The voice provider: reads text aloud (docs/protocol.md).

use crate::{paths, provider};

pub fn supervise(command: Vec<String>) {
    provider::supervise("voice", command);
}

/// Sends a request ("say <text>", "stop", "status").
pub fn send(request: &str) -> String {
    provider::send("voice", &paths::voice_socket(), request)
}

#[derive(serde::Deserialize)]
struct Voices {
    chosen: String,
    voices: Vec<String>,
}

fn voices() -> Result<Voices, String> {
    let answer = send("voices");
    serde_json::from_str(&answer).map_err(|_| answer)
}

/// The narrator's voices, for a model: the list, with the chosen one.
pub fn list() -> String {
    match voices() {
        Ok(v) if v.voices.is_empty() => "No voices to choose from: the voice provider has no clips.".into(),
        Ok(v) => v
            .voices
            .iter()
            .map(|name| if *name == v.chosen { format!("* {name} (chosen)") } else { format!("  {name}") })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(e) => format!("error: {e}"),
    }
}

/// Reads with this voice from now on. Returns the provider's answer.
pub fn choose(name: &str) -> String {
    send(&format!("voice {name}"))
}

/// Asks which voice the narrator reads with, with the d-pad, then lets
/// the new voice say a sentence. Returns a note for the overlay.
pub fn choose_in_overlay(restore: impl Fn()) -> String {
    let v = match voices() {
        Ok(v) => v,
        Err(e) => return format!("Nie mam listy głosów: {e}"),
    };
    if v.voices.len() < 2 {
        return "Jest tylko jeden głos lektora".into();
    }
    let names: Vec<&String> = v.voices.iter().take(crate::approvals::MAX_OPTIONS).collect();
    let options = names.iter().map(|n| if **n == v.chosen { format!("{n} ✓") } else { n.to_string() }).collect();
    let question = lejce::Question {
        tool: None,
        text: "Którym głosem czytać?".into(),
        detail: String::new(),
        options,
        multiple: false,
    };
    // The mark starts on the current voice. With more voices than rows,
    // the current one may not be shown; then it starts on the first.
    let start = names.iter().position(|n| **n == v.chosen).unwrap_or(0);
    match crate::approvals::show_marked("Lektor", &question, start, restore) {
        Some(lejce::Answer::Chose(picked)) => {
            let Some(name) = picked.first().map(|&i| names[i].clone()) else { return String::new() };
            let done = choose(&name);
            if done != "ok" {
                return format!("Nie zmieniłem głosu: {done}");
            }
            send("say Tak teraz brzmię.");
            format!("Lektor: {name}")
        }
        Some(_) => String::new(),
        None => "Najpierw odpowiedz na pytanie, które czeka".into(),
    }
}
