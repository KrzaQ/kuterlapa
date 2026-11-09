//! The help: what each controller button does, built from mapping.json, so
//! it always matches the buttons as they are. The overlay shows it in
//! groups with short texts; the models get it with longer texts through
//! the kuterlapa_help tool (mcp.rs).

use crate::mapping::{Action, Mapping};
use std::sync::atomic::AtomicBool;

/// True while a panel (the help, the recent sessions) covers the overlay.
/// The chat and the draft then wait with their views until it closes.
pub static PANEL: AtomicBool = AtomicBool::new(false);
use serde_json::{json, Value};

/// Buttons in the order the help lists them.
const BUTTONS: &[&str] = &[
    "X", "Y", "A", "B", "L", "R", "ZL", "ZR", "+", "-", "Share", "Up", "Down", "Left", "Right", "L3", "R3",
];

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Speech,
    Model,
    Screen,
}

const GROUPS: &[(Group, &str)] =
    &[(Group::Speech, "Mówienie"), (Group::Model, "Model i lektor"), (Group::Screen, "Ekran i mysz")];

struct Row {
    keys: String,
    group: Group,
    short: String,
    long: String,
}

fn rows(mapping: &mut Mapping) -> Vec<Row> {
    let mut rows = Vec::new();
    for button in BUTTONS {
        for (suffix, shown) in [("", ""), (" double", " ×2"), (" hold", " długo")] {
            if let Some(action) = mapping.action(&format!("{button}{suffix}")) {
                let (group, short, long) = describe(&action);
                rows.push(Row { keys: format!("{}{shown}", name(button)), group, short, long });
            }
        }
    }
    for (stick, role) in mapping.sticks() {
        let keys = if stick.starts_with("left") { "lewa gałka" } else { "prawa gałka" };
        let (short, long) = match role.as_str() {
            "mouse" => ("kursor", "porusza kursorem myszy"),
            "scroll" | "scroll-inverted" => ("przewijanie", "przewija okno pod kursorem"),
            other => (other, other),
        };
        rows.push(Row { keys: keys.into(), group: Group::Screen, short: short.into(), long: long.into() });
    }
    merge(rows)
}

/// Joins rows that do the same, for example "+ ×2 · Share ×2" for the help.
fn merge(rows: Vec<Row>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for row in rows {
        match out.iter_mut().find(|r| r.group == row.group && r.long == row.long) {
            Some(same) => same.keys = format!("{} · {}", same.keys, row.keys),
            None => out.push(row),
        }
    }
    out
}

/// The overlay state of the help: groups of rows, each a key and a short
/// text. The shell lays them out (see OverlayPanel.swift).
pub fn overlay(mapping: &mut Mapping) -> Value {
    let rows = rows(mapping);
    let groups: Vec<Value> = GROUPS
        .iter()
        .map(|(group, title)| {
            let items: Vec<Value> = rows
                .iter()
                .filter(|r| r.group == *group)
                .map(|r| json!({ "keys": r.keys, "text": r.short }))
                .collect();
            json!({ "title": title, "rows": items })
        })
        .collect();
    json!({
        "visible": true,
        "title": "Pomoc · przyciski pada",
        "help": groups,
        "hint": "B: zamknij · zamyka się sama po 30 s · ×2: naciśnij dwa razy · długo: przytrzymaj",
    })
}

/// The help for a model, in Markdown: what each button does and how the
/// main features work.
pub fn text(mapping: &mut Mapping) -> String {
    let rows = rows(mapping);
    let mut out = String::from("# Kuterłapa: przyciski pada\n");
    for (group, title) in GROUPS {
        out.push_str(&format!("\n## {title}\n\n"));
        for r in rows.iter().filter(|r| r.group == *group) {
            out.push_str(&format!("- **{}**: {}\n", r.keys, r.long));
        }
    }
    out.push_str(
        "\n## Jak to działa\n\n\
         - „×2” to dwa szybkie naciśnięcia, „długo” to przytrzymanie przycisku.\n\
         - Rozmowa (Y) wysyła się sama po pauzie w mówieniu; pasek w okienku odlicza czas. Można też \
         nacisnąć Y jeszcze raz.\n\
         - Tryby lektora (przytrzymaj Y): wyłączony, czyta całe odpowiedzi, rozmowa (krótkie odpowiedzi \
         na głos).\n\
         - Gdy narzędzie pyta o zgodę, w okienku są duże przyciski: A to tak, B to nie.\n\
         - B w trakcie myślenia modelu przerywa go razem z narzędziami.\n\
         - Nazwa profilu modelu (Opus, Qwen) jest w tytule okienka.\n\
         - Sesje: osobne konteksty pracy (rozmowa, notatki, słownik). Przejście: „przejdź do sesji X” \
         (zostaje, dopóki nie zmienisz). „Wybieraj sesję automatycznie”: sesję wybiera okno na wierzchu \
         według reguł sesji; w tytule okienka jest wtedy „(auto)”. „Utwórz sesję …” zakłada nową.\n\
         - Pamięć: model zapisuje sobie notatki (np. jak dojść do WhatsAppa) i korzysta z nich następnym \
         razem; globalne dotyczą komputera i użytkownika, notatki sesji jej tematu. Można powiedzieć \
         „zapamiętaj”, „zapomnij” albo „przenieś do globalnych”.\n\
         - Większy albo mniejszy tekst: poproś model (narzędzie computer_zoom), w aplikacji na wierzchu \
         albo w okienku Kuterłapy.\n",
    );
    out
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

/// The group, a short text for the overlay and a longer one for a model.
fn describe(action: &Action) -> (Group, String, String) {
    use Group::*;
    let (group, short, long): (Group, &str, String) = match action {
        Action::Dictate => (
            Speech,
            "dyktuj do pola",
            "dyktowanie do pola tekstowego w oknie na wierzchu; przy otwartym szkicu: dopowiedz do szkicu".into(),
        ),
        Action::Draft => (Speech, "nowy szkic", "nowy szkic w okienku; model poprawia go według kolejnych wypowiedzi".into()),
        Action::Chat => (Speech, "rozmowa", "rozmowa z modelem w bieżącej sesji; wysyła się sama po pauzie".into()),
        Action::Command1 => (Speech, "jednorazowe polecenie", "jednorazowe polecenie do modelu, poza historią rozmowy".into()),
        Action::FinishDictation { then } => {
            let key = then.as_deref().map(|k| format!(" i naciśnij {k}")).unwrap_or_default();
            (
                Speech,
                "wyślij · zgoda: tak",
                format!("zakończ dyktowanie{key}; przy szkicu: wpisz go i wyślij; przy pytaniu o zgodę: tak"),
            )
        }
        Action::EraseDictation { fallback } => {
            let key = fallback.as_deref().map(|k| format!("; gdy nic z tego: {k}")).unwrap_or_default();
            (
                Speech,
                "anuluj · zgoda: nie",
                format!("skasuj dyktowanie, odrzuć szkic, zamknij odpowiedź, przerwij model; przy pytaniu o zgodę: nie{key}"),
            )
        }
        Action::NextProfile => (Model, "następny model", "przełącz na następny profil modelu".into()),
        Action::PreviousProfile => (Model, "poprzedni model", "przełącz na poprzedni profil modelu".into()),
        Action::Recall => (Model, "ostatnia odpowiedź", "pokaż ostatnią odpowiedź jeszcze raz".into()),
        Action::NewConversation => (
            Model,
            "nowa rozmowa / polecenie",
            "zacznij nową rozmowę w bieżącej sesji; gdy w okienku jest odpowiedź polecenia: zakończ tylko \
             wątek polecenia (następne Y ×2 zacznie od nowa), rozmowa w sesji zostaje"
                .into(),
        ),
        Action::VoiceToggle => (Model, "tryb lektora", "zmień tryb lektora: wyłączony → czyta odpowiedzi → rozmowa".into()),
        Action::VoiceStop => (Model, "cisza", "ucisz lektora (i zatrzymaj telewizor)".into()),
        Action::Help => (Model, "ta pomoc", "pokaż albo schowaj tę pomoc".into()),
        Action::Sessions => (Model, "ostatnie sesje", "pokaż albo schowaj ostatnio używane sesje".into()),
        Action::View => (
            Model,
            "widok czatu",
            "otwórz albo zamknij duży widok rozmowy (↑ ↓ przewijanie, ← → inne rozmowy, B zamyka)".into(),
        ),
        Action::Mouse(how) => match how.as_str() {
            "click" => (Screen, "klik", "klik myszą w miejscu kursora".into()),
            "double-click" => (Screen, "podwójny klik", "podwójny klik w miejscu kursora".into()),
            "right-click" => (Screen, "prawy klik", "prawy klik w miejscu kursora".into()),
            other => (Screen, "mysz", other.to_string()),
        },
        Action::Key(combo) => match combo.as_str() {
            "ctrl+Left" => (Screen, "pulpit w lewo", "poprzedni pulpit (ctrl+←)".into()),
            "ctrl+Right" => (Screen, "pulpit w prawo", "następny pulpit (ctrl+→)".into()),
            "PageUp" => (Screen, "strona w górę", "strona w górę (Page Up)".into()),
            "PageDown" => (Screen, "strona w dół", "strona w dół (Page Down)".into()),
            other => (Screen, "", format!("naciśnij {other}")),
        },
        Action::Speech(request) => (Speech, "mowa", format!("rozpoznawanie mowy: {request}")),
        Action::Command(command) => (Screen, "polecenie", format!("uruchom: {command}")),
    };
    let short = match action {
        Action::Key(combo) if short.is_empty() => combo.clone(),
        _ => short.to_string(),
    };
    (group, short, long)
}
