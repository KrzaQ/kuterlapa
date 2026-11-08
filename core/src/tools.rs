//! Tools the model may call: programs described by TOML files.
//!
//! Tools come from three folders; a later one replaces a tool of the same
//! name from an earlier one:
//!
//!   ~/.local/share/kuterlapa/tools/              packs that `make install`
//!                                                copies for this system
//!   ~/.config/kuterlapa/tools/                   the user's own tools
//!   ~/.config/kuterlapa/sessions/<name>/tools/   tools of one session
//!
//! The file name is the tool name. A command argument that starts with "./"
//! (the program or a script it runs) is relative to the tool's file.
//!
//!   # ~/.config/kuterlapa/tools/youtube_speed.toml
//!   description = "Set the playback speed of YouTube on the TV."
//!   command = ["~/bin/youtube", "speed", "{speed}"]
//!   confirm = false      # ask the user first (A yes, B no); default false
//!   timeout = 30         # seconds; default 60
//!
//!   [params.speed]
//!   type = "number"      # string, number, integer or boolean
//!   description = "0.25 to 2"
//!   required = true      # default true
//!
//! "{name}" in the command becomes the parameter's value. An argument that
//! names an optional parameter the model left out is dropped.
//! The program never runs through a shell, so a value cannot inject
//! commands.

use crate::paths;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    #[serde(skip)]
    pub name: String,
    pub description: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    #[serde(rename = "type", default = "default_type")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub required: bool,
}

fn default_timeout() -> u64 {
    60
}

fn default_type() -> String {
    "string".into()
}

fn yes() -> bool {
    true
}

/// The global tools and the session's tools. A file that does not parse
/// is skipped with a message, so one typo does not hide the other tools.
pub fn load(session: Option<&str>) -> Vec<Tool> {
    let mut tools = BTreeMap::new();
    let mut dirs = vec![paths::data_dir().join("tools"), paths::config_dir().join("tools")];
    if let Some(session) = session {
        dirs.push(paths::config_dir().join("sessions").join(session).join("tools"));
    }
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match read(&path) {
                Ok(tool) => {
                    tools.insert(tool.name.clone(), tool);
                }
                Err(e) => eprintln!("tools: {}: {e:#}", path.display()),
            }
        }
    }
    tools.into_values().collect()
}

fn read(path: &Path) -> Result<Tool> {
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("the file name must be a tool name: letters, digits, _ and -");
    }
    let text = fs::read_to_string(path)?;
    let mut tool: Tool = toml::from_str(&text).context("parsing")?;
    if tool.command.is_empty() {
        bail!("command is empty");
    }
    if let Some(dir) = path.parent() {
        for arg in &mut tool.command {
            if let Some(file) = arg.strip_prefix("./") {
                *arg = dir.join(file).to_string_lossy().into_owned();
            }
        }
    }
    tool.name = name;
    Ok(tool)
}

impl Tool {
    /// The tool's description for the model, as MCP and JSON Schema want it.
    pub fn schema(&self) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for (name, param) in &self.params {
            properties.insert(name.clone(), json!({ "type": param.kind, "description": param.description }));
            if param.required {
                required.push(name.clone());
            }
        }
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
        })
    }

    /// The command line with the arguments filled in.
    pub fn argv(&self, arguments: &Map<String, Value>) -> Result<Vec<String>> {
        for (name, param) in &self.params {
            if param.required && !arguments.contains_key(name) {
                bail!("missing parameter {name}");
            }
        }
        let text = |v: &Value| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut argv = Vec::new();
        for (i, arg) in self.command.iter().enumerate() {
            let missing = self
                .params
                .keys()
                .any(|name| !arguments.contains_key(name) && arg.contains(&format!("{{{name}}}")));
            if missing {
                continue;
            }
            let mut filled = arg.clone();
            for (name, value) in arguments {
                filled = filled.replace(&format!("{{{name}}}"), &text(value));
            }
            if i == 0 || filled.starts_with("~/") {
                filled = paths::expand(&filled).to_string_lossy().into_owned();
            }
            argv.push(filled);
        }
        Ok(argv)
    }

    /// Runs the tool. Returns its output and whether it succeeded.
    pub fn run(&self, arguments: &Map<String, Value>) -> (String, bool) {
        let argv = match self.argv(arguments) {
            Ok(argv) => argv,
            Err(e) => return (format!("{e:#}"), false),
        };
        match run(&argv, Duration::from_secs(self.timeout)) {
            Ok(result) => result,
            Err(e) => (format!("cannot run {}: {e:#}", argv[0]), false),
        }
    }
}

fn run(argv: &[String], timeout: Duration) -> Result<(String, bool)> {
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out = thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let mut text = out.join().unwrap_or_default().trim_end().to_string();
    let errors = err.join().unwrap_or_default();
    if !errors.trim().is_empty() {
        text = format!("{}\n{}", text.trim_end(), errors.trim_end()).trim().to_string();
    }
    match status {
        Some(status) if status.success() => Ok((text, true)),
        Some(status) => Ok((format!("{text}\n({status})").trim().to_string(), false)),
        None => Ok((format!("{text}\n(stopped after {} s)", timeout.as_secs()).trim().to_string(), false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool() -> Tool {
        let mut tool: Tool = toml::from_str(
            r#"
            description = "test"
            command = ["/bin/echo", "speed", "{speed}", "--note={note}", "{note}"]
            [params.speed]
            type = "number"
            [params.note]
            required = false
            "#,
        )
        .unwrap();
        tool.name = "t".into();
        tool
    }

    #[test]
    fn fills_arguments() {
        let args = json!({ "speed": 1.5, "note": "a b" });
        let argv = tool().argv(args.as_object().unwrap()).unwrap();
        assert_eq!(argv, ["/bin/echo", "speed", "1.5", "--note=a b", "a b"]);
    }

    #[test]
    fn drops_missing_optional() {
        let args = json!({ "speed": 2 });
        let argv = tool().argv(args.as_object().unwrap()).unwrap();
        assert_eq!(argv, ["/bin/echo", "speed", "2"]);
    }

    #[test]
    fn needs_required() {
        let args = json!({});
        assert!(tool().argv(args.as_object().unwrap()).is_err());
    }

    #[test]
    fn runs() {
        let args = json!({ "speed": 1 });
        let (out, ok) = tool().run(args.as_object().unwrap());
        assert!(ok);
        assert_eq!(out, "speed 1");
    }
}
