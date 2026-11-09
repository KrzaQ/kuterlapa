//! Sessions: named contexts for work, one folder each in
//! ~/.config/kuterlapa/sessions/<name>/:
//!
//!   context.md      what the session is about; the model reads it
//!   dictionary.txt  words for speech recognition in this session
//!   memory/         the models' notes about this session's topic
//!   session.toml    optional settings:
//!                     folder = "~/code/my-project"   # the project folder
//!                     # The window in front picks this session when the
//!                     # session is chosen automatically:
//!                     apps = ["com.googlecode.iterm2"]  # bundle ids or names
//!                     titles = ["bill", "rent"]         # parts of the title
//!
//! The current session's name is kept in ~/.local/state/kuterlapa/session.
//! Without one, the core uses "general". How it is chosen is in
//! session-mode: "sticky" (the default) keeps it until the user switches;
//! "auto" picks it from the window in front each time a recording starts.

use crate::{paths, shell};
use anyhow::{bail, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

pub const DEFAULT: &str = "general";

#[derive(Debug, Clone)]
pub struct Session {
    pub name: String,
    /// The name as the user said it; the same as `name` when not set.
    pub label: String,
    pub context: String,
    /// The project folder; engines run there, so they see its CLAUDE.md.
    pub folder: Option<PathBuf>,
    apps: Vec<String>,
    titles: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct SessionFile {
    /// The name as the user said it, with Polish letters; the folder has
    /// the plain form (see `slug`).
    name: Option<String>,
    folder: Option<String>,
    apps: Vec<String>,
    titles: Vec<String>,
}

fn sessions_dir() -> PathBuf {
    paths::config_dir().join("sessions")
}

fn dir(name: &str) -> PathBuf {
    sessions_dir().join(name)
}

fn state_file() -> PathBuf {
    paths::state_dir().join("session")
}

fn mode_file() -> PathBuf {
    paths::state_dir().join("session-mode")
}

fn previous_file() -> PathBuf {
    paths::state_dir().join("session-previous")
}

/// Makes `name` current and remembers the one before, for `back`.
fn set_current(name: &str) -> Result<()> {
    fs::create_dir_all(paths::state_dir())?;
    let old = current_name();
    if old != name {
        fs::write(previous_file(), format!("{old}\n"))?;
    }
    fs::write(state_file(), format!("{name}\n"))?;
    touch(name);
    Ok(())
}

fn times_file() -> PathBuf {
    paths::state_dir().join("session-times.json")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn times() -> std::collections::BTreeMap<String, u64> {
    fs::read_to_string(times_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Notes that a session was used now: a message, or a switch to it.
pub fn touch(name: &str) {
    let mut all = times();
    all.insert(name.to_string(), now());
    if let Ok(text) = serde_json::to_string_pretty(&all) {
        let _ = fs::write(times_file(), text);
    }
}

/// Sessions with the time they were last used, most recent first.
pub fn recent() -> Vec<(Session, Option<u64>)> {
    let used = times();
    let mut list: Vec<(Session, Option<u64>)> = all()
        .into_iter()
        .map(|s| {
            let at = used.get(&s.name).copied();
            (s, at)
        })
        .collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    list
}

/// "przed chwilą", "5 min temu", "3 godz. temu", "wczoraj", "4 dni temu".
pub fn ago(at: Option<u64>) -> String {
    let Some(at) = at else { return "nieużywana".into() };
    let s = now().saturating_sub(at);
    match s {
        0..=59 => "przed chwilą".into(),
        60..=3599 => format!("{} min temu", s / 60),
        3600..=86_399 => format!("{} godz. temu", s / 3600),
        86_400..=172_799 => "wczoraj".into(),
        _ => format!("{} dni temu", s / 86_400),
    }
}

/// The overlay state of the recent sessions panel, in the help's layout.
pub fn panel() -> serde_json::Value {
    let current = current_name();
    let rows: Vec<serde_json::Value> = recent()
        .into_iter()
        .take(12)
        .map(|(s, at)| {
            // The first line of the context that is not a heading.
            let about = s.context.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("");
            let about: String = about.chars().take(56).collect();
            let mark = if s.name == current { "● teraz · " } else { "" };
            let when = if s.name == current { String::new() } else { ago(at) };
            let text = [when, about].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" · ");
            serde_json::json!({ "keys": s.label, "text": format!("{mark}{text}") })
        })
        .collect();
    let mode = if automatic() { "wybiera okno (auto)" } else { "lepka" };
    serde_json::json!({
        "visible": true,
        "title": format!("Sesje · bieżąca: {current} · {mode}"),
        "help": [{ "title": "Ostatnio używane", "rows": rows }],
        "help_key_width": 120,
        "width": 560,
        "hint": "Powiedz: „przejdź do sesji …” · B: zamknij · zamyka się sama po 30 s",
    })
}

/// Goes back to the session before the current one; it stays (sticky).
pub fn back() -> Result<String> {
    let previous = fs::read_to_string(previous_file()).unwrap_or_default().trim().to_string();
    if previous.is_empty() {
        bail!("there is no previous session");
    }
    switch(&previous, true)
}

pub fn current_name() -> String {
    let name = fs::read_to_string(state_file()).unwrap_or_default().trim().to_string();
    if name.is_empty() { DEFAULT.to_string() } else { name }
}

pub fn current() -> Session {
    load(&current_name())
}

/// True when the window in front picks the session.
pub fn automatic() -> bool {
    fs::read_to_string(mode_file()).unwrap_or_default().trim() == "auto"
}

pub fn load(name: &str) -> Session {
    let dir = dir(name);
    let context = fs::read_to_string(dir.join("context.md")).unwrap_or_default();
    let settings: SessionFile = fs::read_to_string(dir.join("session.toml"))
        .ok()
        .and_then(|t| toml::from_str(&t).map_err(|e| eprintln!("session {name}: {e}")).ok())
        .unwrap_or_default();
    Session {
        name: name.to_string(),
        label: settings.name.clone().unwrap_or_else(|| name.to_string()),
        context,
        folder: settings.folder.map(|f| paths::expand(&f)).filter(|p| p.is_dir()),
        apps: settings.apps,
        titles: settings.titles,
    }
}

/// All sessions, by name.
pub fn all() -> Vec<Session> {
    let mut names: Vec<String> = fs::read_dir(sessions_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if !names.iter().any(|n| n == DEFAULT) {
        names.push(DEFAULT.to_string());
    }
    names.sort();
    names.iter().map(|n| load(n)).collect()
}

/// The plain form of a name, for the folder and for comparing names:
/// lowercase ASCII, accents removed ("Zakupy świąteczne" → "zakupy-swiateczne"),
/// other characters as "-".
pub fn slug(name: &str) -> Result<String> {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        let plain = match c {
            'ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
            'ć' | 'č' | 'ç' => "c",
            'ę' | 'è' | 'é' | 'ê' | 'ë' | 'ě' => "e",
            'ł' => "l",
            'ń' | 'ñ' | 'ň' => "n",
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
            'ś' | 'š' | 'ß' => "s",
            'ź' | 'ż' | 'ž' => "z",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'ù' | 'ú' | 'û' | 'ü' | 'ů' => "u",
            'ý' | 'ÿ' => "y",
            'ř' => "r",
            'ť' => "t",
            'ď' => "d",
            c if c.is_ascii_alphanumeric() || c == '_' => {
                out.push(c);
                continue;
            }
            _ => "-",
        };
        if plain == "-" && out.ends_with('-') {
            continue;
        }
        out.push_str(plain);
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() || out.len() > 40 {
        bail!("a session name needs 1 to 40 letters or digits");
    }
    Ok(out)
}

fn clean(name: &str) -> Result<String> {
    slug(name)
}

/// Edit distance between two plain names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let current = row[j + 1];
            row[j + 1] = (row[j + 1] + 1).min(row[j] + 1).min(previous + usize::from(ca != b[j]));
            previous = current;
        }
    }
    row[b.len()]
}

/// Sessions whose plain name is the same as `name`'s or close to it: one
/// or two letters apart, or one containing the other.
pub fn similar(name: &str) -> Vec<String> {
    let Ok(wanted) = slug(name) else { return Vec::new() };
    all()
        .into_iter()
        .map(|s| s.name)
        .filter(|n| {
            let short = n.len().min(wanted.len());
            n == &wanted
                || distance(n, &wanted) <= if short <= 4 { 1 } else { 2 }
                || (short >= 3 && (n.contains(&wanted) || wanted.contains(n.as_str())))
        })
        .collect()
}

/// Makes `name` the current session. `sticky`: keep it until the user
/// switches; otherwise the window in front picks sessions from now on.
pub fn switch(name: &str, sticky: bool) -> Result<String> {
    let name = clean(name)?;
    if name != DEFAULT && !dir(&name).is_dir() {
        let known: Vec<String> = all().into_iter().map(|s| s.name).collect();
        let close = similar(&name);
        let hint = if close.is_empty() { String::new() } else { format!(" Similar: {}.", close.join(", ")) };
        bail!(
            "no session {name}.{hint} All sessions: {}. Do not create one unless the user asks; you may offer it.",
            known.join(", ")
        );
    }
    set_current(&name)?;
    fs::write(mode_file(), if sticky { "sticky\n" } else { "auto\n" })?;
    Ok(name)
}

/// Lets the window in front pick the session from now on.
pub fn set_automatic() -> Result<()> {
    fs::create_dir_all(paths::state_dir())?;
    fs::write(mode_file(), "auto\n")?;
    Ok(())
}

/// Creates a session folder with its context and rules. Refuses when a
/// session with the same or a similar name exists, unless
/// `allow_similar`: the user may mean that one.
pub fn create(
    name: &str,
    context: &str,
    folder: Option<&str>,
    apps: &[String],
    titles: &[String],
    allow_similar: bool,
) -> Result<String> {
    let label = name.trim().to_string();
    let name = slug(name)?;
    let dir = dir(&name);
    if dir.exists() || name == DEFAULT {
        bail!("session {name} exists already");
    }
    let close = similar(&name);
    if !close.is_empty() && !allow_similar {
        bail!(
            "similar sessions exist: {}. Ask the user whether they mean one of them; create a new one only \
             if they confirm (allow_similar).",
            close.join(", ")
        );
    }
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("context.md"), format!("{}\n", context.trim()))?;
    let mut toml = String::new();
    if label != name {
        toml.push_str(&format!("name = {label:?}\n"));
    }
    if let Some(folder) = folder.filter(|f| !f.trim().is_empty()) {
        toml.push_str(&format!("folder = {:?}\n", folder.trim()));
    }
    if !apps.is_empty() {
        toml.push_str(&format!("apps = {apps:?}\n"));
    }
    if !titles.is_empty() {
        toml.push_str(&format!("titles = {titles:?}\n"));
    }
    if !toml.is_empty() {
        fs::write(dir.join("session.toml"), toml)?;
    }
    Ok(name)
}

/// Moves a session's folder (its context, notes, words and rules) to the
/// Trash. The conversations stay in the history. If it was current,
/// "general" becomes current.
pub fn delete(name: &str) -> Result<String> {
    let name = slug(name)?;
    if name == DEFAULT {
        bail!("the general session cannot be deleted");
    }
    let folder = dir(&name);
    if !folder.is_dir() {
        let close = similar(&name);
        let hint = if close.is_empty() { String::new() } else { format!(" Similar: {}.", close.join(", ")) };
        bail!("no session {name}.{hint}");
    }
    let trash = paths::home().join(".Trash");
    let mut target = trash.join(format!("kuterlapa-session-{name}"));
    let mut n = 2;
    while target.exists() {
        target = trash.join(format!("kuterlapa-session-{name}-{n}"));
        n += 1;
    }
    fs::rename(&folder, &target)?;
    if current_name() == name {
        switch(DEFAULT, true)?;
    }
    Ok(name)
}

/// The window in front: its bundle id, application name and title.
pub fn front_window() -> Option<(String, String, String)> {
    let window = shell::send("window");
    if window.starts_with("error") || window.is_empty() {
        return None;
    }
    let mut parts = window.splitn(3, '\t');
    Some((
        parts.next()?.to_string(),
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    ))
}

/// Adds the window in front to a session's rules: its application and,
/// if given, a part of its title that stays the same. Returns the window
/// that was added, to show the user.
pub fn bind(name: &str, title_part: &str, by_app: bool) -> Result<String> {
    let name = clean(name)?;
    let path = dir(&name).join("session.toml");
    if !dir(&name).is_dir() {
        bail!("no session {name}");
    }
    let (bundle, app, title) = front_window().ok_or_else(|| anyhow::anyhow!("cannot read the window in front"))?;
    let part = title_part.trim();
    if !part.is_empty() && !title.to_lowercase().contains(&part.to_lowercase()) {
        bail!("the window title {title:?} does not contain {part:?}; pick a part of it");
    }
    if part.is_empty() && !by_app {
        bail!("give a part of the title, or bind the whole application");
    }
    let mut file: SessionFile = fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    if by_app && !file.apps.iter().any(|a| a.eq_ignore_ascii_case(&app) || a.eq_ignore_ascii_case(&bundle)) {
        file.apps.push(app.clone());
    }
    if !part.is_empty() && !file.titles.iter().any(|t| t.eq_ignore_ascii_case(part)) {
        file.titles.push(part.to_string());
    }
    let mut toml = String::new();
    if let Some(label) = &file.name {
        toml.push_str(&format!("name = {label:?}\n"));
    }
    if let Some(folder) = &file.folder {
        toml.push_str(&format!("folder = {folder:?}\n"));
    }
    if !file.apps.is_empty() {
        toml.push_str(&format!("apps = {:?}\n", file.apps));
    }
    if !file.titles.is_empty() {
        toml.push_str(&format!("titles = {:?}\n", file.titles));
    }
    fs::write(&path, toml)?;
    Ok(format!("{app}: {title}"))
}

impl Session {
    /// How well this session fits a window: 0 when it does not, more for
    /// each kind of rule that fits.
    fn score(&self, bundle: &str, app: &str, title: &str) -> u32 {
        if self.apps.is_empty() && self.titles.is_empty() {
            return 0;
        }
        let lower = title.to_lowercase();
        let app_fits = self.apps.iter().any(|a| a.eq_ignore_ascii_case(bundle) || a.eq_ignore_ascii_case(app));
        let title_fits = self.titles.iter().any(|t| lower.contains(&t.to_lowercase()));
        if (!self.apps.is_empty() && !app_fits) || (!self.titles.is_empty() && !title_fits) {
            return 0;
        }
        u32::from(app_fits) + u32::from(title_fits)
    }

    /// The rules, for a list of sessions.
    pub fn rules(&self) -> String {
        let mut parts = Vec::new();
        if !self.apps.is_empty() {
            parts.push(format!("apps {}", self.apps.join(", ")));
        }
        if !self.titles.is_empty() {
            parts.push(format!("titles {}", self.titles.join(", ")));
        }
        parts.join("; ")
    }
}

/// When sessions are automatic, picks the session for the window in front
/// and makes it current: the best fit, or "general" when none fits.
/// Returns the new name when it changed.
pub fn follow_window() -> Option<(String, String)> {
    if !automatic() {
        return None;
    }
    let (bundle, app, title) = front_window()?;
    let best = all()
        .into_iter()
        .map(|s| (s.score(&bundle, &app, &title), s.name))
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score)
        .map(|(_, name)| name)
        .unwrap_or_else(|| DEFAULT.to_string());
    let old = current_name();
    if best == old {
        return None;
    }
    let _ = set_current(&best);
    Some((old, best))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(apps: &[&str], titles: &[&str]) -> Session {
        Session {
            name: "s".into(),
            label: "s".into(),
            context: String::new(),
            folder: None,
            apps: apps.iter().map(|s| s.to_string()).collect(),
            titles: titles.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn scores() {
        let iterm = session(&["com.googlecode.iterm2"], &[]);
        assert_eq!(iterm.score("com.googlecode.iterm2", "iTerm2", "zsh"), 1);
        assert_eq!(iterm.score("com.google.Chrome", "Google Chrome", "zsh"), 0);
        let bills = session(&["Google Chrome"], &["bill"]);
        assert_eq!(bills.score("com.google.Chrome", "Google Chrome", "Bills - Sheets"), 2);
        assert_eq!(bills.score("com.google.Chrome", "Google Chrome", "News"), 0);
        assert_eq!(session(&[], &[]).score("a", "b", "c"), 0);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Zakupy świąteczne").unwrap(), "zakupy-swiateczne");
        assert_eq!(slug("  Żółć  ").unwrap(), "zolc");
        assert_eq!(slug("Hex-Rays").unwrap(), "hex-rays");
        assert!(slug("!!!").is_err());
    }

    #[test]
    fn distances() {
        assert_eq!(distance("zakupy", "zakupy"), 0);
        assert_eq!(distance("zakupy", "zakup"), 1);
        assert_eq!(distance("hexrays", "hex-rays"), 1);
        assert!(distance("kuterlapa", "zakupy") > 2);
    }
}
