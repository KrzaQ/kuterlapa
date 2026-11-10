//! What a running model request does: its thinking, its tool calls and the
//! tokens it has used. The engines write a stream of JSON events (`claude
//! --output-format stream-json`, `pi --mode json`). `Activity` reads them
//! line by line, so the overlay can show the work while it happens.

use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// How many characters of the thinking to keep: the latest part. The chat
/// view shows all of it, the overlay less (`tail`).
const THINKING: usize = 1200;

/// How many characters of a tool's result to keep.
const RESULT: usize = 90;

/// The prefix of Kuterlapa's own tools in the engines' tool names.
const OURS: [&str; 2] = ["mcp__kuterlapa__", "kuterlapa__"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Running,
    Done,
    Failed,
}

/// One tool call.
#[derive(Clone, Debug)]
pub struct Step {
    id: String,
    pub name: String,
    /// The arguments, short.
    pub detail: String,
    pub state: StepState,
    /// The start of the result, in one line.
    pub result: String,
}

/// One model's use in a request: what the engine reports about it. Claude
/// reports each model once at the end (with the models Claude Code uses on
/// its own); pi reports each answer, and pi-censor each classifier call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Call {
    /// "model": the profile's model (or one the engine used for it);
    /// "classifier": pi-censor judging a tool call.
    pub role: &'static str,
    pub provider: String,
    pub model: String,
    /// Tokens read anew, not from the cache.
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Of `cache_write`, the part kept for an hour (Claude).
    pub cache_write_1h: Option<u64>,
    pub output: u64,
    /// Of `output`, the thinking, where the engine counts it apart.
    pub thinking: Option<u64>,
    pub web_searches: Option<u64>,
    /// In US dollars, as the engine prices it (Claude: list price).
    pub cost: Option<f64>,
    pub ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Activity {
    pub started: Instant,
    /// How long the request took, once it has finished.
    pub finished: Option<Duration>,
    /// The latest thinking block: only its end, at most THINKING characters.
    pub thinking: String,
    pub steps: Vec<Step>,
    /// Tokens the model read, all its calls together (the context of each
    /// call, cached or not).
    pub input: u64,
    /// Tokens the model wrote: thinking, tool calls and text.
    output_done: u64,
    /// Characters written in the message that is still running. Its token
    /// count comes only at its end; until then, about 4 characters count as
    /// one token.
    output_chars: usize,
    /// Each model's use, for the statistics.
    pub calls: Vec<Call>,
    /// The final answer and the error, when the engine has finished.
    pub answer: Option<String>,
    pub error: Option<String>,
}

impl Default for Activity {
    fn default() -> Activity {
        Activity {
            started: Instant::now(),
            finished: None,
            thinking: String::new(),
            steps: Vec::new(),
            input: 0,
            output_done: 0,
            output_chars: 0,
            calls: Vec::new(),
            answer: None,
            error: None,
        }
    }
}

impl Activity {
    /// Tokens written so far, with an estimate for the running message.
    pub fn output(&self) -> u64 {
        self.output_done + (self.output_chars / 4) as u64
    }

    /// Reads one line of an engine's output. Returns true if something
    /// that the overlay shows changed.
    pub fn line(&mut self, engine: &str, line: &str) -> bool {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        match engine {
            "claude" => self.claude(&event),
            "pi" => self.pi(&event),
            _ => false,
        }
    }

    fn think(&mut self, text: &str) {
        self.thinking.push_str(text);
        self.output_chars += text.len();
        if self.thinking.chars().count() > THINKING * 3 / 2 {
            self.thinking = tail(&self.thinking, THINKING);
        }
    }

    fn start_step(&mut self, id: &str, name: &str) {
        let name = OURS.iter().fold(name, |n, prefix| n.strip_prefix(prefix).unwrap_or(n));
        self.steps.push(Step {
            id: id.into(),
            name: name.into(),
            detail: String::new(),
            state: StepState::Running,
            result: String::new(),
        });
    }

    fn step(&mut self, id: &str) -> Option<&mut Step> {
        self.steps.iter_mut().rev().find(|s| s.id == id)
    }

    fn finish_step(&mut self, id: &str, failed: bool, result: &Value) {
        if let Some(step) = self.step(id) {
            step.state = if failed { StepState::Failed } else { StepState::Done };
            step.result = short(&result_text(result), RESULT);
        }
    }

    /// The work for the chat view: live (with the thinking) or, after the
    /// answer, as it is saved with the answer (without it).
    pub fn to_json(&self, thinking: bool) -> Value {
        let steps: Vec<Value> = self
            .steps
            .iter()
            .map(|s| {
                let state = match s.state {
                    StepState::Running => "running",
                    StepState::Done => "done",
                    StepState::Failed => "failed",
                };
                json!({ "name": s.name, "detail": s.detail, "state": state, "result": s.result })
            })
            .collect();
        let mut value = json!({
            "input": self.input,
            "output": self.output(),
            "seconds": self.finished.unwrap_or_else(|| self.started.elapsed()).as_secs_f64(),
            "steps": steps,
        });
        if thinking {
            value["thinking"] = json!(self.thinking.trim());
        }
        value
    }

    /// `claude -p --output-format stream-json --verbose
    /// --include-partial-messages`.
    fn claude(&mut self, e: &Value) -> bool {
        match e["type"].as_str() {
            Some("stream_event") => {
                let event = &e["event"];
                match event["type"].as_str() {
                    Some("message_start") => {
                        self.input += context(&event["message"]["usage"]);
                        self.output_chars = 0;
                        true
                    }
                    Some("content_block_start") => {
                        let block = &event["content_block"];
                        match block["type"].as_str() {
                            Some("thinking") => {
                                self.thinking.clear();
                                false
                            }
                            Some("tool_use") | Some("server_tool_use") => {
                                self.start_step(str(&block["id"]), str(&block["name"]));
                                true
                            }
                            _ => false,
                        }
                    }
                    Some("content_block_delta") => {
                        let delta = &event["delta"];
                        match delta["type"].as_str() {
                            Some("thinking_delta") => {
                                self.think(str(&delta["thinking"]));
                                true
                            }
                            Some("text_delta") => {
                                self.output_chars += str(&delta["text"]).len();
                                true
                            }
                            Some("input_json_delta") => {
                                self.output_chars += str(&delta["partial_json"]).len();
                                true
                            }
                            _ => false,
                        }
                    }
                    Some("message_delta") => {
                        self.output_done += event["usage"]["output_tokens"].as_u64().unwrap_or(0);
                        self.output_chars = 0;
                        true
                    }
                    _ => false,
                }
            }
            // The whole message: the tool calls with their arguments.
            Some("assistant") => {
                let mut changed = false;
                for block in e["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_use" {
                        let detail = arguments(&block["input"]);
                        if let Some(step) = self.step(str(&block["id"])) {
                            step.detail = detail;
                            changed = true;
                        }
                    }
                }
                changed
            }
            // Tool results.
            Some("user") => {
                let mut changed = false;
                for block in e["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_result" {
                        let failed = block["is_error"].as_bool().unwrap_or(false);
                        self.finish_step(str(&block["tool_use_id"]), failed, &block["content"]);
                        changed = true;
                    }
                }
                changed
            }
            Some("result") => {
                for (model, u) in e["modelUsage"].as_object().into_iter().flatten() {
                    self.calls.push(Call {
                        role: "model",
                        provider: "anthropic".into(),
                        model: model.clone(),
                        input: number(&u["inputTokens"]),
                        cache_read: number(&u["cacheReadInputTokens"]),
                        cache_write: number(&u["cacheCreationInputTokens"]),
                        cache_write_1h: u["cacheCreationEphemeral1hInputTokens"].as_u64(),
                        output: number(&u["outputTokens"]),
                        thinking: u["thinkingTokens"].as_u64(),
                        web_searches: u["webSearchRequests"].as_u64(),
                        cost: u["costUSD"].as_f64(),
                        ms: None,
                    });
                }
                if e["is_error"].as_bool().unwrap_or(false) {
                    let error = e["result"].as_str().or(e["subtype"].as_str()).unwrap_or("error");
                    self.error = Some(error.to_string());
                } else {
                    self.answer = Some(str(&e["result"]).trim().to_string());
                }
                true
            }
            _ => false,
        }
    }

    /// `pi -p --mode json`.
    fn pi(&mut self, e: &Value) -> bool {
        match e["type"].as_str() {
            Some("message_update") => {
                let event = &e["assistantMessageEvent"];
                match event["type"].as_str() {
                    Some("thinking_start") => {
                        self.thinking.clear();
                        false
                    }
                    Some("thinking_delta") => {
                        self.think(str(&event["delta"]));
                        true
                    }
                    Some("text_delta") | Some("toolcall_delta") => {
                        self.output_chars += str(&event["delta"]).len();
                        true
                    }
                    Some("toolcall_start") => {
                        self.start_step(str(&event["id"]), str(&event["toolName"]));
                        true
                    }
                    Some("toolcall_end") => {
                        let call = &event["toolCall"];
                        let detail = arguments(&call["arguments"]);
                        if let Some(step) = self.step(str(&call["id"])) {
                            step.detail = detail;
                        }
                        true
                    }
                    _ => false,
                }
            }
            Some("tool_execution_end") => {
                let failed = e["isError"].as_bool().unwrap_or(false);
                self.finish_step(str(&e["toolCallId"]), failed, &e["result"]["content"]);
                true
            }
            // pi-censor's classifier call, reported with pi.appendEntry.
            Some("entry_appended") if e["entry"]["customType"] == "pi-censor-usage" => {
                let data = &e["entry"]["data"];
                self.calls.push(pi_call("classifier", &data["provider"], &data["model"], &data["usage"], &data["ms"]));
                false
            }
            Some("message_end") if e["message"]["role"] == "assistant" => {
                let message = &e["message"];
                let usage = &message["usage"];
                self.calls.push(pi_call("model", &message["provider"], &message["model"], usage, &message["durationMs"]));
                self.input += ["input", "cacheRead", "cacheWrite"].iter().map(|k| usage[k].as_u64().unwrap_or(0)).sum::<u64>();
                self.output_done += usage["output"].as_u64().unwrap_or(0);
                self.output_chars = 0;
                // pi tries again after some errors; the last message counts.
                if message["stopReason"] == "error" || message["stopReason"] == "aborted" {
                    self.error = Some(message["errorMessage"].as_str().unwrap_or("error").to_string());
                } else {
                    let text: Vec<&str> = message["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|b| b["type"] == "text")
                        .map(|b| str(&b["text"]))
                        .collect();
                    if !text.is_empty() {
                        self.answer = Some(text.join("").trim().to_string());
                    }
                    self.error = None;
                }
                true
            }
            _ => false,
        }
    }
}

fn str(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn number(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

/// A call from pi's usage (`Usage` in pi's message types).
fn pi_call(role: &'static str, provider: &Value, model: &Value, usage: &Value, ms: &Value) -> Call {
    // pi writes 0 for thinking it cannot count apart: that is unknown.
    let thinking = usage["reasoning"].as_u64().filter(|&n| n > 0);
    Call {
        role,
        provider: str(provider).into(),
        model: str(model).into(),
        input: number(&usage["input"]),
        cache_read: number(&usage["cacheRead"]),
        cache_write: number(&usage["cacheWrite"]),
        cache_write_1h: usage["cacheWrite1h"].as_u64(),
        output: number(&usage["output"]),
        thinking,
        web_searches: None,
        cost: usage["cost"]["total"].as_f64(),
        ms: ms.as_u64(),
    }
}

/// The tokens of a call's context: new, cached and written to the cache.
fn context(usage: &Value) -> u64 {
    ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
        .iter()
        .map(|k| usage[k].as_u64().unwrap_or(0))
        .sum()
}

/// A tool's arguments in one short line: the values, without the names
/// when there is one, `name: value` otherwise.
fn arguments(input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return String::new();
    };
    let value = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let text = if map.len() == 1 {
        map.values().map(value).collect::<String>()
    } else {
        map.iter().map(|(k, v)| format!("{k}: {}", value(v))).collect::<Vec<_>>().join(", ")
    };
    short(&text, 70)
}

/// A tool result's text: a string, or the text blocks of a list.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// `text` in one line, at most `count` characters.
fn short(text: &str, count: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > count {
        format!("{}…", line.chars().take(count - 1).collect::<String>())
    } else {
        line
    }
}

/// The last `count` characters of `text`, from the start of a word.
pub fn tail(text: &str, count: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= count {
        return text.to_string();
    }
    let rest: String = chars[chars.len() - count..].iter().collect();
    match rest.find(char::is_whitespace) {
        Some(i) if i < 30 => rest[i..].trim_start().to_string(),
        _ => rest,
    }
}

/// A token count for people: 537, 4,2k, 19,8k, 1,2M.
pub fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=99_999 => format!("{:.1}k", n as f64 / 1000.0).replace('.', ","),
        100_000..=999_999 => format!("{}k", n / 1000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace('.', ","),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_stream() {
        let lines = [
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"usage":{"input_tokens":2,"cache_creation_input_tokens":6947,"cache_read_input_tokens":0}}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"I should search."}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"mcp__kuterlapa__chrome_tabs","input":{}}}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__kuterlapa__chrome_tabs","input":{"profile":"Work"}}]}}"#,
            r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":82}}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Window 1\n  Inbox"}]}]}}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":6947}}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"12345678"}}}"#,
            r#"{"type":"result","is_error":false,"result":" 7 tabs. ","modelUsage":{"claude-opus-5-5":{"inputTokens":4,"outputTokens":96,"cacheReadInputTokens":6947,"cacheCreationInputTokens":7049,"cacheCreationEphemeral1hInputTokens":7049,"thinkingTokens":20,"webSearchRequests":0,"costUSD":0.08}}}"#,
        ];
        let mut a = Activity::default();
        for line in lines {
            a.line("claude", line);
        }
        assert_eq!(a.thinking, "I should search.");
        assert_eq!(a.steps.len(), 1);
        assert_eq!(a.steps[0].name, "chrome_tabs");
        assert_eq!(a.steps[0].detail, "Work");
        assert_eq!(a.steps[0].state, StepState::Done);
        assert_eq!(a.steps[0].result, "Window 1 Inbox");
        assert_eq!(a.input, 6949 + 7049);
        assert_eq!(a.output(), 82 + 2);
        assert_eq!(a.answer.as_deref(), Some("7 tabs."));
        assert_eq!(
            a.calls,
            vec![Call {
                role: "model",
                provider: "anthropic".into(),
                model: "claude-opus-5-5".into(),
                input: 4,
                cache_read: 6947,
                cache_write: 7049,
                cache_write_1h: Some(7049),
                output: 96,
                thinking: Some(20),
                web_searches: Some(0),
                cost: Some(0.08),
                ms: None,
            }]
        );
    }

    #[test]
    fn pi_stream_with_retry() {
        let lines = [
            r#"{"type":"message_end","message":{"role":"assistant","content":[],"stopReason":"error","errorMessage":"Connection error.","usage":{"input":0,"output":0}}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_start"}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"The user asks"}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_start","id":"c1","toolName":"mcp__kuterlapa__computer_apps"}}"#,
            r#"{"type":"message_update","assistantMessageEvent":{"type":"toolcall_end","toolCall":{"id":"c1","arguments":{}}}}"#,
            r#"{"type":"entry_appended","entry":{"type":"custom","customType":"pi-censor-usage","data":{"tool":"t","provider":"ollama","model":"qwen","usage":{"input":456,"output":177,"cacheRead":0,"cacheWrite":0,"reasoning":0,"cost":{"total":0}},"ms":2646,"stopReason":"stop"}}}"#,
            r#"{"type":"tool_execution_end","toolCallId":"c1","isError":true,"result":{"content":[{"type":"text","text":"no access"}]}}"#,
            r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"thinking"},{"type":"text","text":"Safari."}],"stopReason":"stop","usage":{"input":519,"output":42,"cacheRead":0,"cacheWrite":0}}}"#,
        ];
        let mut a = Activity::default();
        for line in lines {
            a.line("pi", line);
        }
        assert_eq!(a.thinking, "The user asks");
        assert_eq!(a.steps[0].name, "computer_apps");
        assert_eq!(a.steps[0].state, StepState::Failed);
        assert_eq!(a.steps[0].result, "no access");
        assert_eq!((a.input, a.output()), (519, 42));
        assert_eq!(a.answer.as_deref(), Some("Safari."));
        assert_eq!(a.error, None);
        let roles: Vec<_> = a.calls.iter().map(|c| (c.role, c.model.as_str(), c.input, c.ms)).collect();
        assert_eq!(roles, vec![("model", "", 0, None), ("classifier", "qwen", 456, Some(2646)), ("model", "", 519, None)]);
        assert_eq!(a.calls[1].thinking, None);
    }

    #[test]
    fn short_forms() {
        assert_eq!(tokens(537), "537");
        assert_eq!(tokens(19_810), "19,8k");
        assert_eq!(tokens(250_000), "250k");
        assert_eq!(arguments(&serde_json::json!({"a": 1, "b": "x  y"})), "a: 1, b: x y");
        assert_eq!(tail("one two three four", 9), "four");
    }
}
