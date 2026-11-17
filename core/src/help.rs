//! The help: what each controller button does, built from mapping.json, so
//! it always matches the buttons as they are. The overlay shows it in
//! groups with short texts; the models get it with longer texts through
//! the kuterlapa_help tool (mcp.rs).

use crate::mapping::{Action, Mapping};
use std::sync::atomic::AtomicBool;

/// True while a panel (the help, the recent sessions) covers the overlay.
/// The chat and the draft then wait with their views until it closes.
pub static PANEL: AtomicBool = AtomicBool::new(false);
use crate::ui::tr;
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

/// Each group with its title in Polish and in English.
const GROUPS: &[(Group, &str, &str)] = &[
    (Group::Speech, "Mówienie", "Speech"),
    (Group::Model, "Model i lektor", "Model and narrator"),
    (Group::Screen, "Ekran i mysz", "Screen and mouse"),
];

struct Row {
    keys: String,
    group: Group,
    short: String,
    long: String,
}

fn rows(mapping: &mut Mapping) -> Vec<Row> {
    let mut rows = Vec::new();
    for button in BUTTONS {
        for (suffix, shown) in [("", ""), (" double", " ×2"), (" hold", tr(" długo", " hold"))] {
            if let Some(action) = mapping.action(&format!("{button}{suffix}")) {
                let (group, short, long) = describe(&action);
                rows.push(Row { keys: format!("{}{shown}", name(button)), group, short, long });
            }
        }
    }
    for (stick, role) in mapping.sticks() {
        let keys = if stick.starts_with("left") { tr("lewa gałka", "left stick") } else { tr("prawa gałka", "right stick") };
        let (short, long) = match role.as_str() {
            "mouse" => (tr("kursor", "pointer"), tr("porusza kursorem myszy", "moves the mouse pointer")),
            "scroll" | "scroll-inverted" => {
                (tr("przewijanie", "scroll"), tr("przewija okno pod kursorem", "scrolls the window under the pointer"))
            }
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
        .map(|&(group, pl, en)| {
            let items: Vec<Value> = rows
                .iter()
                .filter(|r| r.group == group)
                .map(|r| json!({ "keys": r.keys, "text": r.short }))
                .collect();
            json!({ "title": tr(pl, en), "rows": items })
        })
        .collect();
    json!({
        "visible": true,
        "title": tr("Pomoc · przyciski pada", "Help · controller buttons"),
        "help": groups,
        "hint": tr(
            "B: zamknij · zamyka się sama po 30 s · ×2: naciśnij dwa razy · długo: przytrzymaj",
            "B: close · closes by itself after 30 s · ×2: press two times · hold: hold the button down",
        ),
    })
}

/// The help for a model, in Markdown: what each button does and how the
/// main features work.
pub fn text(mapping: &mut Mapping) -> String {
    let rows = rows(mapping);
    let mut out = String::from(tr("# Kuterłapa: przyciski pada\n", "# Kuterlapa: controller buttons\n"));
    for &(group, pl, en) in GROUPS {
        out.push_str(&format!("\n## {}\n\n", tr(pl, en)));
        for r in rows.iter().filter(|r| r.group == group) {
            out.push_str(&format!("- **{}**: {}\n", r.keys, r.long));
        }
    }
    out.push_str(if crate::ui::polish() { HOW_PL } else { HOW_EN });
    out
}

/// How the main features work, for the end of the help.
const HOW_PL: &str = "\n## Jak to działa\n\n\
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
         - Wyszukiwanie po znaczeniu w notatkach i dawnych rozmowach: „przypomnij sobie, co ustaliliśmy o …” \
         (narzędzie memory_search).\n\
         - Eksport rozmów do Markdowna: „wyeksportuj tę rozmowę” (pliki w ~/Documents/Kuterlapa).\n\
         - Większy albo mniejszy tekst: poproś model (narzędzie computer_zoom), w aplikacji na wierzchu \
         albo w okienku Kuterłapy.\n";

const HOW_EN: &str = "\n## How it works\n\n\
         - \"×2\" is two quick presses, \"hold\" is holding the button down.\n\
         - Chat (Y) sends itself after a pause in speech; a bar in the overlay counts down the time. You can \
         also press Y again.\n\
         - Narrator modes (hold Y): off, reads whole answers, talk (short answers out loud).\n\
         - When a tool asks for permission, the overlay shows large buttons: A is yes, B is no.\n\
         - B while the model thinks stops it and its tools.\n\
         - The overlay title shows the name of the model profile (Opus, Qwen).\n\
         - Sessions: separate work contexts (chat, notes, dictionary). To switch: \"go to session X\" (it \
         stays until you change it). \"Pick the session automatically\": the window in front picks the \
         session by the session rules; the overlay title then shows \"(auto)\". \"Create session …\" makes a \
         new one.\n\
         - Memory: the model writes notes for itself (for example, how to get to WhatsApp) and uses them the \
         next time. Global notes are about the computer and the user; session notes are about the session's \
         topic. You can say \"remember\", \"forget\" or \"move to global\".\n\
         - Search by meaning in notes and past chats: \"recall what we agreed about …\" (the memory_search \
         tool).\n\
         - Export chats to Markdown: \"export this conversation\" (files in ~/Documents/Kuterlapa).\n\
         - Larger or smaller text: ask the model (the computer_zoom tool), in the app in front or in the \
         Kuterlapa overlay.\n";

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
    let pl = crate::ui::polish();
    let (group, short, long): (Group, &str, String) = match action {
        Action::Dictate => (
            Speech,
            tr("dyktuj do pola", "dictate into a field"),
            tr(
                "dyktowanie do pola tekstowego w oknie na wierzchu; przy otwartym szkicu: dopowiedz do szkicu",
                "dictate into the text field of the window in front; with a draft open: add to the draft",
            )
            .into(),
        ),
        Action::Draft => (
            Speech,
            tr("nowy szkic", "new draft"),
            tr(
                "nowy szkic w okienku; model poprawia go według kolejnych wypowiedzi",
                "a new draft in the overlay; the model changes it as you say more",
            )
            .into(),
        ),
        Action::Chat => (
            Speech,
            tr("rozmowa", "conversation"),
            tr(
                "rozmowa z modelem w bieżącej sesji; wysyła się sama po pauzie",
                "a conversation with the model in the current session; sends itself after a pause",
            )
            .into(),
        ),
        Action::Command1 => (
            Speech,
            tr("jednorazowe polecenie", "one-off command"),
            tr(
                "jednorazowe polecenie do modelu, poza historią rozmowy",
                "a one-off command to the model, outside the conversation history",
            )
            .into(),
        ),
        Action::FinishDictation { then } => {
            if pl {
                let key = then.as_deref().map(|k| format!(" i naciśnij {k}")).unwrap_or_default();
                (
                    Speech,
                    "wyślij · zgoda: tak",
                    format!("zakończ dyktowanie{key}; przy szkicu: wpisz go (bez Entera); przy pytaniu o zgodę: tak"),
                )
            } else {
                let key = then.as_deref().map(|k| format!(" and press {k}")).unwrap_or_default();
                (
                    Speech,
                    "send · permission: yes",
                    format!("end the dictation{key}; with a draft: type it (no Enter); when asked for permission: yes"),
                )
            }
        }
        Action::EraseDictation { fallback } => {
            if pl {
                let key = fallback.as_deref().map(|k| format!("; gdy nic z tego: {k}")).unwrap_or_default();
                (
                    Speech,
                    "anuluj · zgoda: nie",
                    format!("skasuj dyktowanie, odrzuć szkic, zamknij odpowiedź, przerwij model; przy pytaniu o zgodę: nie{key}"),
                )
            } else {
                let key = fallback.as_deref().map(|k| format!("; if none of these: {k}")).unwrap_or_default();
                (
                    Speech,
                    "cancel · permission: no",
                    format!(
                        "erase the dictation, drop the draft, close the answer, stop the model; when asked for \
                         permission: no{key}"
                    ),
                )
            }
        }
        Action::NextProfile => (
            Model,
            tr("następny model", "next model"),
            tr("przełącz na następny profil modelu", "switch to the next model profile").into(),
        ),
        Action::PreviousProfile => (
            Model,
            tr("poprzedni model", "previous model"),
            tr("przełącz na poprzedni profil modelu", "switch to the previous model profile").into(),
        ),
        Action::Recall => (
            Model,
            tr("ostatnia odpowiedź", "last answer"),
            tr("pokaż ostatnią odpowiedź jeszcze raz", "show the last answer again").into(),
        ),
        Action::NewConversation => (
            Model,
            tr("nowa rozmowa / polecenie", "new conversation / command"),
            tr(
                "zacznij nową rozmowę w bieżącej sesji; gdy w okienku jest odpowiedź polecenia: zakończ tylko \
                 wątek polecenia (następne Y ×2 zacznie od nowa), rozmowa w sesji zostaje",
                "start a new chat in the current session; when the overlay shows a command answer: end only the \
                 command thread (the next Y ×2 starts again from the start), the chat in the session stays",
            )
            .into(),
        ),
        Action::VoiceToggle => (
            Model,
            tr("tryb lektora", "narrator mode"),
            tr(
                "zmień tryb lektora: wyłączony → czyta odpowiedzi → rozmowa",
                "change the narrator mode: off → reads answers → talk",
            )
            .into(),
        ),
        Action::VoiceStop => (
            Model,
            tr("cisza", "silence"),
            tr("ucisz lektora (i zatrzymaj telewizor)", "silence the narrator (and stop the TV)").into(),
        ),
        Action::Help => (Model, tr("ta pomoc", "this help"), tr("pokaż albo schowaj tę pomoc", "show or hide this help").into()),
        Action::Sessions => (
            Model,
            tr("ostatnie sesje", "recent sessions"),
            tr("pokaż albo schowaj ostatnio używane sesje", "show or hide the recently used sessions").into(),
        ),
        Action::Usage => (
            Model,
            tr("zużycie tokenów", "token use"),
            tr("pokaż albo schowaj dzisiejsze zużycie modeli", "show or hide the model use for today").into(),
        ),
        Action::ClaudeAsk => (
            Model,
            tr("pytanie Claude Code", "Claude Code question"),
            tr(
                "odpowiedz na pytanie Claude Code z bieżącej karty iTerma",
                "answer the Claude Code question in the current iTerm tab",
            )
            .into(),
        ),
        Action::NarratorVoice => (
            Model,
            tr("głos lektora", "narrator voice"),
            tr("wybierz głos lektora (↑ ↓, A wybiera)", "choose the narrator voice (↑ ↓, A chooses)").into(),
        ),
        Action::Microphone => (
            Model,
            tr("mikrofon", "microphone"),
            tr(
                "wybierz mikrofon: systemowy albo inny (↑ ↓, A wybiera)",
                "choose the microphone: the system one or another (↑ ↓, A chooses)",
            )
            .into(),
        ),
        Action::View => (
            Model,
            tr("widok czatu", "chat view"),
            tr(
                "otwórz albo zamknij duży widok rozmowy (↑ ↓ przewijanie, ← → inne rozmowy, B zamyka)",
                "open or close the large chat view (↑ ↓ scroll, ← → other conversations, B closes)",
            )
            .into(),
        ),
        Action::Mouse(how) => match how.as_str() {
            "click" => (Screen, tr("klik", "click"), tr("klik myszą w miejscu kursora", "mouse click at the pointer").into()),
            "double-click" => (
                Screen,
                tr("podwójny klik", "double click"),
                tr("podwójny klik w miejscu kursora", "double click at the pointer").into(),
            ),
            "right-click" => (
                Screen,
                tr("prawy klik", "right click"),
                tr("prawy klik w miejscu kursora", "right click at the pointer").into(),
            ),
            other => (Screen, tr("mysz", "mouse"), other.to_string()),
        },
        Action::Key(combo) => match combo.as_str() {
            "ctrl+Left" => (
                Screen,
                tr("pulpit w lewo", "desktop to the left"),
                tr("poprzedni pulpit (ctrl+←)", "previous desktop (ctrl+←)").into(),
            ),
            "ctrl+Right" => (
                Screen,
                tr("pulpit w prawo", "desktop to the right"),
                tr("następny pulpit (ctrl+→)", "next desktop (ctrl+→)").into(),
            ),
            "PageUp" => (Screen, tr("strona w górę", "page up"), tr("strona w górę (Page Up)", "page up (Page Up)").into()),
            "PageDown" => {
                (Screen, tr("strona w dół", "page down"), tr("strona w dół (Page Down)", "page down (Page Down)").into())
            }
            other => (Screen, "", if pl { format!("naciśnij {other}") } else { format!("press {other}") }),
        },
        Action::Speech(request) => (
            Speech,
            tr("mowa", "speech"),
            if pl { format!("rozpoznawanie mowy: {request}") } else { format!("speech recognition: {request}") },
        ),
        Action::Command(command) => (
            Screen,
            tr("polecenie", "command"),
            if pl { format!("uruchom: {command}") } else { format!("run: {command}") },
        ),
    };
    let short = match action {
        Action::Key(combo) if short.is_empty() => combo.clone(),
        _ => short.to_string(),
    };
    (group, short, long)
}
