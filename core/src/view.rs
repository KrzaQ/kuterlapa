//! The large chat view (Share): the current session's conversations from
//! the history, drawn by the platform shell ("view <json>" on shell.sock).
//! While it is open, the core sends the controller to it: ↑ ↓ scroll,
//! ← → older and newer conversations, B closes.

use crate::store::Store;
use crate::{computer, sessions, shell};
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};

/// How many of a conversation's newest messages the view shows.
const MESSAGES: usize = 300;

struct View {
    store: Arc<Store>,
    /// While open: which conversation, counted back from the newest (0).
    back: Mutex<Option<usize>>,
}

static VIEW: OnceLock<View> = OnceLock::new();

pub fn init(store: Arc<Store>) {
    let _ = VIEW.set(View { store, back: Mutex::new(None) });
}

pub fn is_open() -> bool {
    VIEW.get().is_some_and(|v| v.back.lock().unwrap().is_some())
}

/// Opens the view at the newest conversation, or closes it.
pub fn toggle() {
    let Some(view) = VIEW.get() else { return };
    let open = {
        let mut back = view.back.lock().unwrap();
        if back.is_some() {
            *back = None;
            false
        } else {
            *back = Some(0);
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

/// Another conversation: -1 older, +1 newer.
pub fn step(direction: i64) {
    let Some(view) = VIEW.get() else { return };
    let count = view.store.conversations(&sessions::current_name()).map(|c| c.len()).unwrap_or(0);
    {
        let mut back = view.back.lock().unwrap();
        let Some(current) = *back else { return };
        let next = current as i64 - direction;
        *back = Some(next.clamp(0, count.saturating_sub(1) as i64) as usize);
    }
    send(view, true);
}

pub fn scroll(how: &str) {
    if is_open() {
        shell::send(&format!("view-scroll {how}"));
    }
}

/// The chat changed (a new question or answer): redraw at the newest
/// conversation, if the view is open.
pub fn changed() {
    let Some(view) = VIEW.get() else { return };
    {
        let mut back = view.back.lock().unwrap();
        if back.is_none() {
            return;
        }
        *back = Some(0);
    }
    send(view, true);
}

fn send(view: &View, follow: bool) {
    let back = view.back.lock().unwrap().unwrap_or(0);
    let session = sessions::current();
    let conversations = view.store.conversations(&session.name).unwrap_or_default();
    let index = conversations.len().saturating_sub(1 + back);
    let (messages, conversation) = match conversations.get(index) {
        Some((id, created)) => {
            let messages: Vec<_> = view
                .store
                .messages_full(*id, MESSAGES)
                .unwrap_or_default()
                .into_iter()
                .map(|(role, text, profile, created)| json!({ "role": role, "text": text, "profile": profile, "created": created }))
                .collect();
            (messages, json!({ "index": index + 1, "count": conversations.len(), "created": created }))
        }
        None => (Vec::new(), json!({ "index": 0, "count": 0 })),
    };
    let list: Vec<_> = sessions::recent()
        .into_iter()
        .take(14)
        .map(|(s, at)| {
            json!({ "name": s.name, "label": s.label, "current": s.name == session.name, "when": sessions::ago(at) })
        })
        .collect();
    let state = json!({
        "visible": true,
        "scale": computer::overlay_scale(),
        "session": session.name,
        "label": session.label,
        "automatic": sessions::automatic(),
        "conversation": conversation,
        "sessions": list,
        "messages": messages,
        "follow": follow,
    });
    shell::send(&format!("view {state}"));
}
