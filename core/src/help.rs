//! The help: what each controller button does, built from mapping.json, so
//! it always matches the buttons as they are.

use crate::mapping::{Action, Mapping};
use serde_json::{json, Value};

/// Buttons in the order the help lists them.
const BUTTONS: &[&str] = &[
    "X", "Y", "A", "B", "L", "R", "ZL", "ZR", "+", "-", "Share", "Up", "Down", "Left", "Right", "L3", "R3",
];

/// The overlay state of the help.
pub fn overlay(mapping: &mut Mapping) -> Value {
    let mut lines = Vec::new();
    for button in BUTTONS {
        for (suffix, shown) in [("", ""), (" double", " ×2"), (" hold", " (przytrzymaj)")] {
            if let Some(action) = mapping.action(&format!("{button}{suffix}")) {
                lines.push(format!("{}{shown}  —  {}", name(button), describe(&action)));
            }
        }
    }
    for (stick, role) in mapping.sticks() {
        let stick = if stick.starts_with("left") { "lewa gałka" } else { "prawa gałka" };
        let role = match role.as_str() {
            "mouse" => "kursor",
            "scroll" | "scroll-inverted" => "przewijanie",
            other => other,
        };
        lines.push(format!("{stick}  —  {role}"));
    }
    json!({
        "visible": true,
        "title": "Pomoc · przyciski pada",
        "text": lines.join("\n"),
        "hint": "B: zamknij · znika sama po 30 s",
    })
}

fn name(button: &str) -> &str {
    match button {
        "Up" => "↑",
        "Down" => "↓",
        "Left" => "←",
        "Right" => "→",
        "-" => "−",
        other => other,
    }
}

fn describe(action: &Action) -> String {
    match action {
        Action::Dictate => "dyktowanie do pola (przy szkicu: dopowiedz do szkicu)".into(),
        Action::Draft => "nowy szkic, który model poprawia głosem".into(),
        Action::Chat => "rozmowa w sesji; wysyła się sama po pauzie".into(),
        Action::Command1 => "polecenie jednorazowe".into(),
        Action::VoiceToggle => "lektor: wyłączony → czyta → rozmowa".into(),
        Action::FinishDictation { then } => {
            let key = then.as_deref().map(|k| format!(" + {k}")).unwrap_or_default();
            format!("koniec dyktowania{key}; szkic: wyślij; pytanie: TAK")
        }
        Action::EraseDictation { fallback } => {
            let key = fallback.as_deref().map(|k| format!("; inaczej {k}")).unwrap_or_default();
            format!("skasuj dyktowanie / zamknij / przerwij model; pytanie: NIE{key}")
        }
        Action::NextProfile => "następny model".into(),
        Action::PreviousProfile => "poprzedni model".into(),
        Action::Recall => "pokaż ostatnią odpowiedź".into(),
        Action::NewConversation => "nowa rozmowa".into(),
        Action::VoiceStop => "ucisz lektora i zatrzymaj telewizor".into(),
        Action::Help => "ta pomoc".into(),
        Action::Mouse(how) => match how.as_str() {
            "click" => "klik myszą".into(),
            "double-click" => "podwójny klik".into(),
            "right-click" => "prawy klik".into(),
            other => other.into(),
        },
        Action::Key(combo) => match combo.as_str() {
            "ctrl+Left" => "poprzedni pulpit".into(),
            "ctrl+Right" => "następny pulpit".into(),
            "PageUp" => "strona w górę".into(),
            "PageDown" => "strona w dół".into(),
            other => format!("klawisz {other}"),
        },
        Action::Speech(request) => format!("mowa: {request}"),
        Action::Command(command) => format!("polecenie: {command}"),
    }
}
