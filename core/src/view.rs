//! The large chat view (Share): the current session's conversations and
//! the one-off commands that saw its context, in the order they started,
//! drawn by the platform shell ("view <json>" on shell.sock). While it is
//! open, the core sends the controller to it: ↑ ↓ scroll, ← → older and
//! newer items, B closes. A model can open it at a message it found
//! (conversation_show).

use crate::store::{FullMessage, Store};
use crate::{computer, sessions, shell};
use serde_json::json;
use std::sync::{Mutex, OnceLock};

/// How many of an item's newest messages the view shows.
const MESSAGES: usize = 300;

struct Position {
    /// Another session than the current one, when opened at a found message.
    session: Option<String>,
    /// Which item, counted back from the newest (0).
    back: usize,
    /// A message to scroll to and mark.
    focus: Option<i64>,
}

struct View {
    store: std::sync::Arc<Store>,
    position: Mutex<Option<Position>>,
}

static VIEW: OnceLock<View> = OnceLock::new();

#[derive(Clone)]
enum Kind {
    Conversation,
    Command,
}

struct Item {
    kind: Kind,
    id: i64,
    created: String,
}

pub fn init(store: std::sync::Arc<Store>) {
    let _ = VIEW.set(View { store, position: Mutex::new(None) });
}

pub fn is_open() -> bool {
    VIEW.get().is_some_and(|v| v.position.lock().unwrap().is_some())
}

fn items(store: &Store, session: &str) -> Vec<Item> {
    let mut items: Vec<Item> = store
        .conversations(session)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, created)| Item { kind: Kind::Conversation, id, created })
        .chain(
            store
                .threads(session)
                .unwrap_or_default()
                .into_iter()
                .map(|(id, created)| Item { kind: Kind::Command, id, created }),
        )
        .collect();
    items.sort_by(|a, b| a.created.cmp(&b.created));
    items
}

/// Opens the view at the newest item, or closes it.
pub fn toggle() {
    let Some(view) = VIEW.get() else { return };
    let open = {
        let mut position = view.position.lock().unwrap();
        if position.is_some() {
            *position = None;
            false
        } else {
            *position = Some(Position { session: None, back: 0, focus: None });
            true
        }
    };
    if open {
        send(view, true);
        // The overlay steps aside for the view.
        shell::overlay(&json!({ "visible": false }));
    } else {
        shell::send(r#"view {"visible": false}"#);
    }
}

pub fn close() {
    if is_open() {
        toggle();
    }
}

/// Opens the view at a message: its session and conversation, scrolled to
/// it and marked. Returns false if there is no such message.
pub fn open_at(message: i64) -> bool {
    let Some(view) = VIEW.get() else { return false };
    let Ok(Some((session, conversation))) = view.store.message_place(message) else { return false };
    let list = items(&view.store, &session);
    let Some(index) = list.iter().position(|i| matches!(i.kind, Kind::Conversation) && i.id == conversation) else {
        return false;
    };
    let session = (session != sessions::current_name()).then_some(session);
    *view.position.lock().unwrap() = Some(Position { session, back: list.len() - 1 - index, focus: Some(message) });
    send(view, false);
    shell::overlay(&json!({ "visible": false }));
    true
}

/// Another item: -1 older, +1 newer.
pub fn step(direction: i64) {
    let Some(view) = VIEW.get() else { return };
    {
        let mut position = view.position.lock().unwrap();
        let Some(p) = position.as_mut() else { return };
        let session = p.session.clone().unwrap_or_else(sessions::current_name);
        let count = items(&view.store, &session).len();
        let next = p.back as i64 - direction;
        p.back = next.clamp(0, count.saturating_sub(1) as i64) as usize;
        p.focus = None;
    }
    send(view, true);
}

pub fn scroll(how: &str) {
    if is_open() {
        shell::send(&format!("view-scroll {how}"));
    }
}

/// The chat changed (a new question or answer): show the newest item of
/// the current session, if the view is open.
pub fn changed() {
    let Some(view) = VIEW.get() else { return };
    {
        let mut position = view.position.lock().unwrap();
        match position.as_ref() {
            None => return,
            // The view shows a message a model found: keep it there.
            Some(p) if p.focus.is_some() => return,
            Some(_) => {}
        }
        *position = Some(Position { session: None, back: 0, focus: None });
    }
    send(view, true);
}

fn send(view: &View, follow: bool) {
    let (session_name, back, focus) = {
        let position = view.position.lock().unwrap();
        let Some(p) = position.as_ref() else { return };
        (p.session.clone().unwrap_or_else(sessions::current_name), p.back, p.focus)
    };
    let session = sessions::load(&session_name);
    let list = items(&view.store, &session.name);
    let index = list.len().saturating_sub(1 + back);
    let (messages, item) = match list.get(index) {
        Some(item) => {
            let messages: Vec<FullMessage> = match item.kind {
                Kind::Conversation => view.store.messages_full(item.id, MESSAGES),
                Kind::Command => view.store.thread_full(item.id),
            }
            .unwrap_or_default();
            let kind = match item.kind {
                Kind::Conversation => "Rozmowa",
                Kind::Command => "Polecenie",
            };
            (messages, json!({ "kind": kind, "index": index + 1, "count": list.len(), "created": item.created }))
        }
        None => (Vec::new(), json!({ "kind": "", "index": 0, "count": 0 })),
    };
    let focus_index = focus.and_then(|id| messages.iter().position(|m| m.id == id));
    // The session's items for the side list, newest first.
    let shown_items: Vec<_> = list
        .iter()
        .enumerate()
        .rev()
        .take(20)
        .map(|(i, it)| {
            let command = matches!(it.kind, Kind::Command);
            let title = view.store.first_question(it.id, command).unwrap_or_default();
            let title: String = title.chars().take(60).collect();
            json!({ "kind": if command { "Polecenie" } else { "Rozmowa" }, "title": title,
                    "created": it.created, "current": i == index })
        })
        .collect();
    let messages: Vec<_> = messages
        .into_iter()
        .map(|m| json!({ "role": m.role, "text": m.text, "profile": m.profile, "created": m.created }))
        .collect();
    let current = sessions::current_name();
    let sessions_list: Vec<_> = sessions::recent()
        .into_iter()
        .take(14)
        .map(|(s, at)| {
            json!({ "name": s.name, "label": s.label, "current": s.name == session.name && s.name == current,
                    "shown": s.name == session.name, "when": sessions::ago(at) })
        })
        .collect();
    let state = json!({
        "visible": true,
        "scale": computer::overlay_scale(),
        "session": session.name,
        "label": session.label,
        "other_session": session.name != current,
        "automatic": sessions::automatic(),
        "conversation": item,
        "sessions": sessions_list,
        "items": shown_items,
        "messages": messages,
        "focus": focus_index,
        "follow": follow && focus_index.is_none(),
    });
    shell::send(&format!("view {state}"));
}
