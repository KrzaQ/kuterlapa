//! Token statistics: how much each model used, kept in the `usage` table.
//! The engines report each model's use in a request (Lejce reads it); this
//! module saves it and sums it up for `kuterlapa stats`, the `usage_stats`
//! tool and the overlay panel.
//!
//! The texts are in Polish or English (`crate::ui`). The inner functions take
//! the language as `pl`, so the tests can check each language.

use crate::profiles::Profile;
use crate::store::{Store, UsageRow, UsageSum};
use anyhow::{bail, Result};
use lejce::Call;
use serde_json::json;
use std::sync::{Arc, OnceLock};

static STORE: OnceLock<Arc<Store>> = OnceLock::new();

/// The daemon's store; without it (the CLI, the MCP server) nothing is
/// saved.
pub fn init(store: Arc<Store>) {
    let _ = STORE.set(store);
}

/// Saves what a request used. `purpose`: "chat" or "draft".
pub fn record(profile: &Profile, session: &str, purpose: &str, calls: &[Call]) {
    let Some(store) = STORE.get() else { return };
    for call in calls {
        // A failed answer that used nothing says nothing.
        if call.is_empty() {
            continue;
        }
        let row = UsageRow { session, profile: &profile.name, purpose, engine: &profile.engine, call };
        if let Err(e) = store.add_usage(&row) {
            eprintln!("usage: {e:#}");
        }
    }
}

/// A token count for people: 537, 4,2k, 19,8k, 1,2M (in English 4.2k,
/// 19.8k, 1.2M).
pub fn tokens(n: u64) -> String {
    tokens_in(n, crate::ui::polish())
}

fn tokens_in(n: u64, pl: bool) -> String {
    let text = match n {
        0..=999 => n.to_string(),
        1_000..=99_999 => format!("{:.1}k", n as f64 / 1000.0),
        100_000..=999_999 => format!("{}k", n / 1000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    };
    if pl {
        text.replace('.', ",")
    } else {
        text
    }
}

/// A period of the statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Today,
    Week,
    Month,
    All,
}

impl Period {
    pub fn parse(text: &str) -> Result<Period> {
        Ok(match text.trim().to_lowercase().as_str() {
            "" | "today" | "dziś" | "dzis" | "dzisiaj" => Period::Today,
            "week" | "tydzień" | "tydzien" | "7d" => Period::Week,
            "month" | "miesiąc" | "miesiac" | "30d" => Period::Month,
            "all" | "wszystko" | "zawsze" => Period::All,
            other => bail!("unknown period {other:?}: today, week, month or all"),
        })
    }

    /// SQLite's UTC time where the period starts.
    fn since(self) -> Option<&'static str> {
        match self {
            Period::Today => Some("datetime('now', 'localtime', 'start of day', 'utc')"),
            Period::Week => Some("datetime('now', '-7 days')"),
            Period::Month => Some("datetime('now', '-30 days')"),
            Period::All => None,
        }
    }

    fn label(self, pl: bool) -> &'static str {
        match (self, pl) {
            (Period::Today, true) => "dziś",
            (Period::Week, true) => "ostatnie 7 dni",
            (Period::Month, true) => "ostatnie 30 dni",
            (Period::All, true) => "od początku",
            (Period::Today, false) => "today",
            (Period::Week, false) => "last 7 days",
            (Period::Month, false) => "last 30 days",
            (Period::All, false) => "all time",
        }
    }
}

pub fn sums(store: &Store, period: Period) -> Result<Vec<UsageSum>> {
    store.usage_sums(period.since())
}

/// The groups in the order they are shown, with their titles in Polish and
/// in English.
const GROUPS: [(&str, &str, &str, &str); 3] = [
    ("chat", "model", "Odpowiedzi i polecenia", "Answers and commands"),
    ("draft", "model", "Poprawki szkicu", "Draft changes"),
    ("chat", "classifier", "Klasyfikator pi-censora", "pi-censor classifier"),
];

fn group_title(sum: &UsageSum, pl: bool) -> String {
    GROUPS
        .iter()
        .find(|(purpose, role, _, _)| *purpose == sum.purpose && *role == sum.role)
        .map(|g| if pl { g.2 } else { g.3 }.to_string())
        .unwrap_or_else(|| format!("{} · {}", sum.purpose, sum.role))
}

fn group_order(title: &str) -> usize {
    GROUPS.iter().position(|g| g.2 == title || g.3 == title).unwrap_or(GROUPS.len())
}

/// "12 wywołań", or for Claude, which reports a whole request at once,
/// "w 12 zapytaniach". In English "12 calls" and "in 12 requests".
fn count(sum: &UsageSum, pl: bool) -> String {
    let n = sum.calls;
    if !pl {
        return match (sum.provider == "anthropic", n) {
            (true, 1) => "in 1 request".into(),
            (true, _) => format!("in {n} requests"),
            (false, 1) => "1 call".into(),
            (false, _) => format!("{n} calls"),
        };
    }
    if sum.provider == "anthropic" {
        return if n == 1 { "w 1 zapytaniu".into() } else { format!("w {n} zapytaniach") };
    }
    let word = match (n % 10, n % 100) {
        _ if n == 1 => "wywołanie",
        (2..=4, d) if !(12..=14).contains(&d) => "wywołania",
        _ => "wywołań",
    };
    format!("{n} {word}")
}

fn duration(ms: u64, pl: bool) -> String {
    let s = ms / 1000;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        _ if pl => format!("{} godz. {} min", s / 3600, s % 3600 / 60),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

/// The numbers of a group in one line.
fn numbers(sum: &UsageSum, pl: bool) -> String {
    let t = |p: &'static str, e: &'static str| if pl { p } else { e };
    let mut parts = vec![format!("{} {}", t("wejście", "input"), tokens_in(sum.input, pl))];
    if sum.cache_read > 0 {
        parts.push(format!("{} {}", t("z cache", "from cache"), tokens_in(sum.cache_read, pl)));
    }
    if sum.cache_write > 0 {
        parts.push(format!("{} {}", t("zapis do cache", "cache write"), tokens_in(sum.cache_write, pl)));
    }
    let thinking = sum
        .thinking
        .filter(|&n| n > 0)
        .map(|n| format!(" ({} {})", t("myślenie", "thinking"), tokens_in(n, pl)))
        .unwrap_or_default();
    parts.push(format!("{} {}{thinking}", t("wyjście", "output"), tokens_in(sum.output, pl)));
    if let Some(n) = sum.web_searches.filter(|&n| n > 0) {
        parts.push(format!("{} {n}", t("wyszukiwania", "web searches")));
    }
    if let Some(cost) = sum.cost.filter(|&c| c > 0.0) {
        let cost = format!("${cost:.2}");
        parts.push(if pl { cost.replace('.', ",") } else { cost });
    }
    if let Some(ms) = sum.ms.filter(|&n| n > 0) {
        parts.push(duration(ms, pl));
    }
    parts.join(" · ")
}

fn model_name(sum: &UsageSum) -> String {
    format!("{}/{}", sum.provider, sum.model)
}

/// Groups in the order of GROUPS, each with its rows.
fn grouped(sums: &[UsageSum], pl: bool) -> Vec<(String, Vec<&UsageSum>)> {
    let mut groups: Vec<(String, Vec<&UsageSum>)> = Vec::new();
    for sum in sums {
        let title = group_title(sum, pl);
        match groups.iter_mut().find(|(t, _)| *t == title) {
            Some((_, rows)) => rows.push(sum),
            None => groups.push((title, vec![sum])),
        }
    }
    groups.sort_by_key(|(title, _)| group_order(title));
    groups
}

/// The statistics as text, for the CLI and the models.
pub fn text(sums: &[UsageSum], period: Period) -> String {
    text_in(sums, period, crate::ui::polish())
}

fn text_in(sums: &[UsageSum], period: Period, pl: bool) -> String {
    let heading = if pl { "Zużycie modeli" } else { "Model use" };
    if sums.is_empty() {
        let none = if pl { "nic" } else { "none" };
        return format!("{heading} · {}: {none}.", period.label(pl));
    }
    let mut out = format!("{heading} · {}\n", period.label(pl));
    for (title, rows) in grouped(sums, pl) {
        out.push_str(&format!("\n{title}\n"));
        for sum in rows {
            out.push_str(&format!("- {} — {}\n  {}\n", model_name(sum), count(sum, pl), numbers(sum, pl)));
        }
    }
    let total = UsageSum {
        input: sums.iter().map(|s| s.input).sum(),
        cache_read: sums.iter().map(|s| s.cache_read).sum(),
        cache_write: sums.iter().map(|s| s.cache_write).sum(),
        output: sums.iter().map(|s| s.output).sum(),
        cost: Some(sums.iter().filter_map(|s| s.cost).sum()),
        ..Default::default()
    };
    out.push_str(&format!("\n{}: {}\n", if pl { "Razem" } else { "Total" }, numbers(&total, pl)));
    out
}

/// The statistics in the overlay, like the help (the daemon's store).
pub fn panel(period: Period) -> serde_json::Value {
    let pl = crate::ui::polish();
    let sums = STORE.get().and_then(|store| sums(store, period).ok()).unwrap_or_default();
    let groups: Vec<_> = grouped(&sums, pl)
        .into_iter()
        .map(|(title, rows)| {
            let rows: Vec<_> = rows
                .into_iter()
                .map(|s| json!({ "keys": s.model, "text": format!("{} · {}", count(s, pl), numbers(s, pl)) }))
                .collect();
            json!({ "title": title, "rows": rows })
        })
        .collect();
    let groups = if groups.is_empty() {
        let (title, text) = if pl {
            ("Brak danych", "Żaden model jeszcze nic nie zużył.")
        } else {
            ("No data", "No model has used anything yet.")
        };
        vec![json!({ "title": title, "rows": [{ "keys": "", "text": text }] })]
    } else {
        groups
    };
    json!({
        "visible": true,
        "title": format!("{} · {}", if pl { "Zużycie modeli" } else { "Model use" }, period.label(pl)),
        "help": groups,
        "help_key_width": 210,
        "help_columns": 1,
        "width": 780,
        "hint": if pl {
            "Powiedz: „pokaż zużycie z tygodnia” · B: zamknij · zamyka się sama po 30 s"
        } else {
            "Say: “show the use for this week” · B: close · closes by itself after 30 s"
        },
    })
}

/// `kuterlapa stats [today|week|month|all]`.
pub fn run(args: &[String]) -> Result<()> {
    let period = Period::parse(args.first().map(String::as_str).unwrap_or(""))?;
    let store = Store::open()?;
    print!("{}", text(&sums(&store, period)?, period));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counts() {
        assert_eq!(tokens_in(537, true), "537");
        assert_eq!(tokens_in(19_810, true), "19,8k");
        assert_eq!(tokens_in(250_000, true), "250k");
        assert_eq!(tokens_in(1_000_000, true), "1M");
        assert_eq!(tokens_in(1_200_000, true), "1,2M");
        assert_eq!(tokens_in(19_810, false), "19.8k");
        assert_eq!(tokens_in(1_200_000, false), "1.2M");
    }

    fn example() -> Vec<UsageSum> {
        vec![
            UsageSum {
                purpose: "chat".into(),
                role: "classifier".into(),
                provider: "ollama".into(),
                model: "qwen".into(),
                calls: 3,
                input: 1400,
                output: 500,
                ms: Some(7_500),
                ..Default::default()
            },
            UsageSum {
                purpose: "chat".into(),
                role: "model".into(),
                provider: "anthropic".into(),
                model: "claude-opus-5-5".into(),
                calls: 2,
                input: 10,
                cache_read: 20_000,
                cache_write: 5_000,
                output: 300,
                thinking: Some(120),
                cost: Some(0.123),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn text_groups_and_numbers() {
        let text = text_in(&example(), Period::Today, true);
        let answers = text.find("Odpowiedzi").unwrap();
        let classifier = text.find("Klasyfikator").unwrap();
        assert!(answers < classifier, "{text}");
        assert!(text.contains("anthropic/claude-opus-5-5 — w 2 zapytaniach"), "{text}");
        assert!(text.contains("wejście 10 · z cache 20,0k · zapis do cache 5,0k · wyjście 300 (myślenie 120) · $0,12"), "{text}");
        assert!(text.contains("ollama/qwen — 3 wywołania\n  wejście 1,4k · wyjście 500 · 7 s"), "{text}");
        assert!(text.contains("Razem: wejście 1,4k · z cache 20,0k"), "{text}");
    }

    #[test]
    fn text_in_english() {
        let text = text_in(&example(), Period::Today, false);
        let answers = text.find("Answers").unwrap();
        let classifier = text.find("classifier").unwrap();
        assert!(answers < classifier, "{text}");
        assert!(text.contains("anthropic/claude-opus-5-5 — in 2 requests"), "{text}");
        assert!(text.contains("input 10 · from cache 20.0k · cache write 5.0k · output 300 (thinking 120) · $0.12"), "{text}");
        assert!(text.contains("ollama/qwen — 3 calls\n  input 1.4k · output 500 · 7 s"), "{text}");
        assert!(text.contains("Total: input 1.4k · from cache 20.0k"), "{text}");
    }
}
