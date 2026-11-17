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
        Err(e) if crate::ui::polish() => return format!("Nie mam listy głosów: {e}"),
        Err(e) => return format!("I have no list of voices: {e}"),
    };
    if v.voices.len() < 2 {
        return crate::ui::tr("Jest tylko jeden głos lektora", "The narrator has only one voice").into();
    }
    let names: Vec<&String> = v.voices.iter().take(crate::approvals::MAX_OPTIONS).collect();
    let options = names.iter().map(|n| if **n == v.chosen { format!("{n} ✓") } else { n.to_string() }).collect();
    let question = lejce::Question {
        tool: None,
        text: crate::ui::tr("Którym głosem czytać?", "Which voice do I read with?").into(),
        detail: String::new(),
        options,
        multiple: false,
    };
    // The mark starts on the current voice. With more voices than rows,
    // the current one may not be shown; then it starts on the first.
    let start = names.iter().position(|n| **n == v.chosen).unwrap_or(0);
    match crate::approvals::show_marked(crate::ui::tr("Lektor", "Narrator"), &question, start, restore) {
        Some(lejce::Answer::Chose(picked)) => {
            let Some(name) = picked.first().map(|&i| names[i].clone()) else { return String::new() };
            let done = choose(&name);
            if done != "ok" {
                return if crate::ui::polish() {
                    format!("Nie zmieniłem głosu: {done}")
                } else {
                    format!("I did not change the voice: {done}")
                };
            }
            send(crate::ui::tr("say Tak teraz brzmię.", "say This is how I sound now."));
            format!("{}: {name}", crate::ui::tr("Lektor", "Narrator"))
        }
        Some(_) => String::new(),
        None => crate::ui::tr("Najpierw odpowiedz na pytanie, które czeka", "First answer the question that waits").into(),
    }
}
