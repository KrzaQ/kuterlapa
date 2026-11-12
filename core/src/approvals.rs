//! Questions to the user before a tool runs (`confirm = true` in the tool's
//! file). The MCP server sends "approve <json>" to the core. The core shows
//! the question in the overlay, reads it aloud when the voice is on, and
//! waits for the finish button (A: yes) or the erase button (B: no).
//!
//! A model's own question (ask_user) can instead offer a few options
//! ("options" in the JSON): the d-pad's up and down move between them, A
//! chooses, B cancels. With "multiple": true the user may pick several:
//! A ticks or unticks the marked option, and A on the last row, "Gotowe",
//! answers.

use crate::{hooks, shell, voice};
use lejce::{Answer, Question};
use serde_json::{json, Value};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// The core answers "no" after this time, a little before the MCP server
/// stops waiting.
const WAIT: Duration = Duration::from_secs(140);

/// At most this many options: more do not fit, and are hard to go through.
pub const MAX_OPTIONS: usize = 6;

struct Pending {
    sender: Sender<Answer>,
    /// The options of a choice (empty for yes or no) and the marked one.
    options: Vec<String>,
    selected: usize,
    /// Several may be picked: which are, and one more row to answer.
    multiple: bool,
    ticked: Vec<bool>,
    /// The overlay as shown, to draw it again with another option marked.
    overlay: Value,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

/// The overlay's buttons: the options, with the marked one. For several
/// answers, each with a box, and a last row that answers.
fn choices(p: &Pending) -> Value {
    let mut rows: Vec<Value> = p
        .options
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let text = if p.multiple { format!("{} {text}", if p.ticked[i] { "☑" } else { "☐" }) } else { text.clone() };
            json!({ "text": text, "selected": i == p.selected })
        })
        .collect();
    if p.multiple {
        rows.push(json!({ "text": "✓ Gotowe", "selected": p.selected == p.options.len(), "done": true }));
    }
    json!(rows)
}

/// Asks the user a question from the MCP server and waits. `restore`
/// shows the overlay as it was before. Returns the answer for the MCP
/// server: "yes", "chose <n>: <option>" (from 1) or "no: <why>".
pub fn ask(question: &str, restore: impl Fn()) -> String {
    let question: Value = serde_json::from_str(question).unwrap_or(Value::Null);
    let text = |key: &str| question.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let options: Vec<String> = question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .take(MAX_OPTIONS)
        .collect();
    let question = Question {
        // A model's own question (ask_user) has no tool waiting.
        tool: (text("kind") != "question").then(|| text("tool")),
        text: text("description"),
        detail: text("command"),
        options,
        multiple: question["multiple"].as_bool().unwrap_or(false),
    };
    let options = &question.options;
    match show(&text("title"), &question, restore) {
        None => "no: another question is waiting".into(),
        Some(Answer::Yes) => "yes".into(),
        Some(Answer::Chose(picked)) => format!(
            "chose {}: {}",
            picked.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
            picked.iter().map(|&i| options[i].as_str()).collect::<Vec<_>>().join(", ")
        ),
        Some(Answer::No) if is_choice(&question) => "no: the user cancelled".into(),
        Some(Answer::No) => "no: the user said no".into(),
        Some(Answer::Unanswered) => "no: the user did not answer".into(),
    }
}

/// Several may be picked: only in a model's own question with options.
fn is_multiple(question: &Question) -> bool {
    question.tool.is_none() && question.multiple && !question.options.is_empty()
}

/// A choice between options, not yes or no.
fn is_choice(question: &Question) -> bool {
    question.tool.is_none() && (question.options.len() >= 2 || is_multiple(question))
}

/// Shows a question in the overlay, reads it aloud when the voice is on,
/// and waits for the answer. `title` may be empty. `restore` shows the
/// overlay as it was before. None if another question is waiting.
pub fn show(title: &str, question: &Question, restore: impl Fn()) -> Option<Answer> {
    let description = &question.text;
    let question_only = question.tool.is_none();
    let options = &question.options;
    let multiple = is_multiple(question);
    let choice = is_choice(question);
    let mut overlay = json!({
        "visible": true,
        "title": match title {
            t if !t.is_empty() => t.to_string(),
            _ if question_only => "Pytanie modelu".into(),
            _ => "Zgoda".into(),
        },
        "status": "Czekam na Twoją decyzję",
        "heading": match &question.tool {
            None => description.clone(),
            Some(tool) => tool.replace('_', " "),
        },
        "text": if question_only { String::new() } else { description.clone() },
        "detail": question.detail,
        "buttons": if choice {
            json!([{ "key": "A", "label": "Wybierz", "tone": "yes" }, { "key": "B", "label": "Anuluj", "tone": "no" }])
        } else {
            json!([{ "key": "A", "label": "Tak", "tone": "yes" }, { "key": "B", "label": "Nie", "tone": "no" }])
        },
        "attention": true,
    });
    let (sender, receiver) = mpsc::channel();
    {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_some() {
            return None;
        }
        let mut p = Pending {
            sender,
            options: if choice { options.clone() } else { Vec::new() },
            selected: 0,
            multiple,
            ticked: vec![false; options.len()],
            overlay: Value::Null,
        };
        if choice {
            overlay["choices"] = choices(&p);
            overlay["hint"] = json!(if multiple {
                "↑ ↓: wybór · A: zaznacz, na „Gotowe”: odpowiedz · B: anuluj"
            } else {
                "↑ ↓: wybór · A: wybierz · B: anuluj"
            });
            if multiple {
                overlay["buttons"] = json!([{ "key": "A", "label": "Zaznacz", "tone": "yes" }, { "key": "B", "label": "Anuluj", "tone": "no" }]);
            }
        }
        p.overlay = overlay.clone();
        *pending = Some(p);
    }
    shell::overlay(&overlay);
    if hooks::voice_mode() != "off" {
        if choice {
            let list: Vec<String> = options.iter().enumerate().map(|(i, o)| format!("{}: {o}", i + 1)).collect();
            voice::send(&format!("say {description} {}.", list.join(", ")));
        } else if question_only {
            voice::send(&format!("say {description}"));
        } else {
            voice::send(&format!("say Czy pozwalasz? {description}"));
        }
    }
    let answer = receiver.recv_timeout(WAIT).unwrap_or(Answer::Unanswered);
    PENDING.lock().unwrap().take();
    restore();
    Some(answer)
}

/// Moves the mark of a waiting choice up (-1) or down (1), around the
/// ends. Returns false if no choice waits, so the button does its usual
/// work.
pub fn move_mark(step: i64) -> bool {
    let overlay = {
        let mut pending = PENDING.lock().unwrap();
        let Some(p) = pending.as_mut().filter(|p| !p.options.is_empty()) else {
            return false;
        };
        let count = (p.options.len() + usize::from(p.multiple)) as i64;
        p.selected = (p.selected as i64 + step).rem_euclid(count) as usize;
        p.overlay["choices"] = choices(p);
        p.overlay["attention"] = json!(true);
        p.overlay.clone()
    };
    shell::overlay(&overlay);
    true
}

/// True while a question waits for A or B.
pub fn pending() -> bool {
    PENDING.lock().unwrap().is_some()
}

/// Answers the waiting question (the A and B buttons): for a choice, A
/// chooses the marked option. Returns false if none waits.
pub fn answer(yes: bool) -> bool {
    let mut pending = PENDING.lock().unwrap();
    // Several answers: A on an option ticks it, and the question waits on.
    if let Some(p) = pending.as_mut().filter(|p| yes && p.multiple && p.selected < p.options.len()) {
        p.ticked[p.selected] = !p.ticked[p.selected];
        p.overlay["choices"] = choices(p);
        let overlay = p.overlay.clone();
        drop(pending);
        shell::overlay(&overlay);
        return true;
    }
    match pending.take() {
        Some(p) => {
            let answer = match (yes, p.options.is_empty(), p.multiple) {
                (false, _, _) => Answer::No,
                (true, true, _) => Answer::Yes,
                (true, false, true) => Answer::Chose(p.ticked.iter().enumerate().filter(|(_, &t)| t).map(|(i, _)| i).collect()),
                (true, false, false) => Answer::Chose(vec![p.selected]),
            };
            let _ = p.sender.send(answer);
            true
        }
        None => false,
    }
}
