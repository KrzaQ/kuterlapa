//! Button-to-action mapping, read from ~/.config/kuterlapa/mapping.json:
//!   { "A": "Return", "X": "dictate", "X double": "draft", "R3": "click",
//!     "LeftStick": "scroll", "RightStick": "mouse" }
//! A key is a button name, optionally followed by " double" or " hold".
//! Keys ending in "Stick" set what an analog stick does; the platform shell
//! handles stick movement itself. The file is read again when it changes,
//! so a new mapping needs no restart.

use crate::gestures::Has;
use crate::paths;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Press a key combination, for example "Return" or "ctrl+Left". The
    /// platform shell knows the key names.
    Key(String),
    /// Send a request to the speech provider, for example "toggle".
    Speech(String),
    /// Run a shell command.
    Command(String),
    /// Erase the text of the last dictation and stop the recording.
    /// With nothing to erase, press the fallback key, if any.
    EraseDictation { fallback: Option<String> },
    /// End the dictation, if one runs, wait for its last words, then press
    /// the key, if any. "finish-dictation|Return" makes one button both
    /// "I am done speaking" and "send".
    FinishDictation { then: Option<String> },
    /// Dictate into a draft in the overlay. The first press starts the
    /// draft; each next one adds speech that the model applies to it.
    Draft,
    /// Switch to the next model profile.
    NextProfile,
    /// Switch to the previous model profile, after one press too many.
    PreviousProfile,
    /// Start or stop dictation into the application in front. While a
    /// draft is open, work on the draft instead.
    Dictate,
    /// A mouse action at the pointer: "click", "double-click", "right-click".
    Mouse(String),
    /// Record a message to the session's conversation, or send it.
    Chat,
    /// Record a one-off command, or send it.
    Command1,
    /// Read answers aloud, or stop.
    VoiceToggle,
    /// Show the last answer again.
    Recall,
    /// Start a new conversation in the current session.
    NewConversation,
    /// Stop the narrator now, without changing the voice mode.
    VoiceStop,
    /// Show what each button does, or hide it.
    Help,
    /// Show the recently used sessions, or hide them.
    Sessions,
}

impl Action {
    pub fn parse(spec: &str) -> Option<Action> {
        match spec {
            "chat" => return Some(Action::Chat),
            "command" => return Some(Action::Command1),
            "voice-toggle" => return Some(Action::VoiceToggle),
            "recall" => return Some(Action::Recall),
            "previous-profile" => return Some(Action::PreviousProfile),
            "new-conversation" => return Some(Action::NewConversation),
            "voice-stop" => return Some(Action::VoiceStop),
            "help" => return Some(Action::Help),
            "sessions" => return Some(Action::Sessions),
            _ => {}
        }
        if spec == "draft" {
            return Some(Action::Draft);
        }
        if spec == "dictate" {
            return Some(Action::Dictate);
        }
        if matches!(spec, "click" | "double-click" | "right-click") {
            return Some(Action::Mouse(spec.to_string()));
        }
        if spec == "next-profile" {
            return Some(Action::NextProfile);
        }
        if spec == "finish-dictation" {
            return Some(Action::FinishDictation { then: None });
        }
        if let Some(key) = spec.strip_prefix("finish-dictation|") {
            return Some(Action::FinishDictation { then: Some(key.to_string()) });
        }
        if spec == "erase-dictation" {
            return Some(Action::EraseDictation { fallback: None });
        }
        if let Some(key) = spec.strip_prefix("erase-dictation|") {
            return Some(Action::EraseDictation { fallback: Some(key.to_string()) });
        }
        if let Some(command) = spec.strip_prefix("cmd:") {
            return Some(Action::Command(command.to_string()));
        }
        if let Some(request) = spec.strip_prefix("speech:") {
            return Some(Action::Speech(request.to_string()));
        }
        if spec.is_empty() || spec.contains(char::is_whitespace) {
            return None;
        }
        Some(Action::Key(spec.to_string()))
    }
}

pub struct Mapping {
    path: PathBuf,
    loaded: Option<SystemTime>,
    actions: HashMap<String, Action>,
    /// "leftstick" -> "scroll", and so on; sent to the platform shell.
    sticks: BTreeMap<String, String>,
}

impl Mapping {
    pub fn new() -> Mapping {
        Mapping {
            path: paths::config_dir().join("mapping.json"),
            loaded: None,
            actions: HashMap::new(),
            sticks: BTreeMap::new(),
        }
    }

    /// The action for a key such as "X" or "X double".
    pub fn action(&mut self, key: &str) -> Option<Action> {
        self.reload_if_changed();
        self.actions.get(&key.to_lowercase()).cloned()
    }

    /// Which gestures besides a single press the button has actions for.
    pub fn has(&mut self, button: &str) -> Has {
        self.reload_if_changed();
        let b = button.to_lowercase();
        Has {
            double: self.actions.contains_key(&format!("{b} double")),
            hold: self.actions.contains_key(&format!("{b} hold")),
        }
    }

    pub fn sticks(&self) -> &BTreeMap<String, String> {
        &self.sticks
    }

    /// Returns true if the file changed and was read again.
    pub fn reload_if_changed(&mut self) -> bool {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified == self.loaded {
            return false;
        }
        self.loaded = modified;
        self.actions.clear();
        self.sticks.clear();
        let raw: HashMap<String, String> = match fs::read_to_string(&self.path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(raw) => raw,
            Err(e) => {
                eprintln!("cannot read {}: {e}", self.path.display());
                return true;
            }
        };
        let mut listed: Vec<_> = raw.iter().collect();
        listed.sort();
        for (button, spec) in listed {
            if button.to_lowercase().ends_with("stick") {
                self.sticks.insert(button.to_lowercase(), spec.clone());
                continue;
            }
            match Action::parse(spec) {
                Some(action) => {
                    self.actions.insert(button.to_lowercase(), action);
                }
                None => eprintln!("bad action {spec:?} for button {button}"),
            }
        }
        eprintln!("mapping: {} buttons from {}", self.actions.len(), self.path.display());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions() {
        assert_eq!(Action::parse("Return"), Some(Action::Key("Return".into())));
        assert_eq!(Action::parse("speech:toggle"), Some(Action::Speech("toggle".into())));
        assert_eq!(Action::parse("cmd:open -a Slack"), Some(Action::Command("open -a Slack".into())));
        assert_eq!(
            Action::parse("erase-dictation|Escape"),
            Some(Action::EraseDictation { fallback: Some("Escape".into()) })
        );
        assert_eq!(Action::parse("erase-dictation"), Some(Action::EraseDictation { fallback: None }));
        assert_eq!(
            Action::parse("finish-dictation|Return"),
            Some(Action::FinishDictation { then: Some("Return".into()) })
        );
        assert_eq!(Action::parse("two words"), None);
    }
}
