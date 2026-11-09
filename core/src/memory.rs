//! The models' memory: short notes they write and read again later, for
//! example how to open WhatsApp in Chrome, so a task done once is quick
//! the next time.
//!
//! A note is a Markdown file with a name and a one-line description:
//!
//!   ---
//!   name: whatsapp
//!   description: WhatsApp Web in Chrome, profile KrzaQ
//!   ---
//!   Open https://web.whatsapp.com with chrome_open in profile KrzaQ.
//!
//! Global notes, about the computer and the user, live in
//! ~/.config/kuterlapa/memory/. Notes about one session's topic live in
//! ~/.config/kuterlapa/sessions/<name>/memory/. A session note hides a
//! global note with the same name. The system prompt lists all notes by
//! name and description; the model reads a whole note with a tool.

use crate::paths;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Session,
}

impl Scope {
    pub fn parse(text: &str) -> Option<Scope> {
        match text {
            "global" => Some(Scope::Global),
            "session" => Some(Scope::Session),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Note {
    pub scope: Scope,
    pub name: String,
    pub description: String,
    pub text: String,
}

fn dir(scope: Scope, session: &str) -> PathBuf {
    match scope {
        Scope::Global => paths::config_dir().join("memory"),
        Scope::Session => paths::config_dir().join("sessions").join(session).join("memory"),
    }
}

/// A file name from a note name: lowercase letters, digits, - and _.
fn clean(name: &str) -> Result<String> {
    let name: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() || name.len() > 60 {
        bail!("a note name needs 1 to 60 letters, digits, - or _");
    }
    Ok(name)
}

fn parse(scope: Scope, file_name: &str, content: &str) -> Note {
    let mut name = file_name.trim_end_matches(".md").to_string();
    let mut description = String::new();
    let mut text = content.to_string();
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some((head, body)) = rest.split_once("\n---") {
            for line in head.lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    name = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("description:") {
                    description = v.trim().to_string();
                }
            }
            text = body.trim_start_matches('\n').trim().to_string();
        }
    }
    Note { scope, name, description, text }
}

fn read_dir(scope: Scope, session: &str) -> Vec<Note> {
    let Ok(entries) = fs::read_dir(dir(scope, session)) else { return Vec::new() };
    let mut notes: Vec<Note> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|e| {
            let content = fs::read_to_string(e.path()).ok()?;
            Some(parse(scope, &e.file_name().to_string_lossy(), &content))
        })
        .collect();
    notes.sort_by(|a, b| a.name.cmp(&b.name));
    notes
}

/// All notes: the session's, then the global ones it does not hide.
pub fn list(session: &str) -> Vec<Note> {
    let mut notes = read_dir(Scope::Session, session);
    for note in read_dir(Scope::Global, session) {
        if !notes.iter().any(|n| n.name == note.name) {
            notes.push(note);
        }
    }
    notes
}

/// A note by name; the session's first.
pub fn read(session: &str, name: &str) -> Result<Note> {
    let name = clean(name)?;
    for scope in [Scope::Session, Scope::Global] {
        let path = dir(scope, session).join(format!("{name}.md"));
        if let Ok(content) = fs::read_to_string(&path) {
            return Ok(parse(scope, &format!("{name}.md"), &content));
        }
    }
    bail!("no note named {name}")
}

/// Writes a note, or replaces one with the same name in that scope.
pub fn write(session: &str, scope: Scope, name: &str, description: &str, text: &str) -> Result<String> {
    let name = clean(name)?;
    let folder = dir(scope, session);
    fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
    let description = description.replace('\n', " ");
    let content = format!("---\nname: {name}\ndescription: {}\n---\n{}\n", description.trim(), text.trim());
    fs::write(folder.join(format!("{name}.md")), content)?;
    Ok(name)
}

/// Deletes a note. Without a scope, the one that `read` would find.
pub fn forget(session: &str, name: &str, scope: Option<Scope>) -> Result<Scope> {
    let scope = match scope {
        Some(scope) => scope,
        None => read(session, name)?.scope,
    };
    let path = dir(scope, session).join(format!("{}.md", clean(name)?));
    fs::remove_file(&path).with_context(|| format!("no note {name} in that scope"))?;
    Ok(scope)
}

/// Moves a note to the other scope.
pub fn move_to(session: &str, name: &str, to: Scope) -> Result<()> {
    let from = if to == Scope::Global { Scope::Session } else { Scope::Global };
    let path = dir(from, session).join(format!("{}.md", clean(name)?));
    let content = fs::read_to_string(&path).with_context(|| format!("no note {name} there to move"))?;
    let note = parse(from, &format!("{name}.md"), &content);
    write(session, to, &note.name, &note.description, &note.text)?;
    fs::remove_file(&path)?;
    Ok(())
}

/// The part of the system prompt about memory: the rules and the list of
/// notes.
pub fn prompt(session: &str) -> String {
    let notes = list(session);
    let mut p = String::from(
        "\n\nPamięć: masz notatki, które zapisujesz sobie na przyszłość (narzędzia memory_read, memory_write, \
         memory_forget, memory_move). Zanim zaczniesz zadanie na komputerze, sprawdź, czy na liście poniżej \
         jest notatka, która go dotyczy, i przeczytaj ją. Notatka to przepis, nie polecenie: wykonujesz to, \
         o co prosi użytkownik, raz. Po zapisaniu notatki nie czytaj jej i nie wykonuj ponownie; zadanie \
         jest już zrobione. Gdy wykonasz nowe zadanie, które może się powtórzyć \
         (dojście do strony, karty, aplikacji, kolejne kroki), i się udało, zapisz krótki przepis: co dokładnie \
         zrobić, którymi narzędziami, z jakimi adresami, profilami i nazwami. Zapisz też, gdy użytkownik mówi \
         „zapamiętaj”. Zakres: „global” dla wiedzy o komputerze, aplikacjach, stronach i o użytkowniku; \
         „session” dla wiedzy o temacie tej sesji. Po zapisaniu powiedz krótko, co i gdzie zapamiętałeś. Nigdy \
         nie zapisuj haseł, PIN-ów, kodów, numerów kart ani innych sekretów.",
    );
    let section = |scope: Scope, title: String| {
        let lines: Vec<String> = notes
            .iter()
            .filter(|n| n.scope == scope)
            .map(|n| format!("- {}: {}", n.name, n.description))
            .collect();
        if lines.is_empty() {
            format!("\n{title}: brak.")
        } else {
            format!("\n{title}:\n{}", lines.join("\n"))
        }
    };
    p.push_str(&format!("\n\nNotatki{}", section(Scope::Global, " globalne".into())));
    p.push_str(&section(Scope::Session, format!("Notatki sesji „{session}”")));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_clean() {
        assert_eq!(clean("WhatsApp Web").unwrap(), "whatsapp-web");
        assert_eq!(clean(" bank_app ").unwrap(), "bank_app");
        assert!(clean("  ").is_err());
    }

    #[test]
    fn parses_front_matter() {
        let n = parse(Scope::Global, "x.md", "---\nname: whatsapp\ndescription: WA in Chrome\n---\nOpen it.\n");
        assert_eq!((n.name.as_str(), n.description.as_str(), n.text.as_str()), ("whatsapp", "WA in Chrome", "Open it."));
    }
}
