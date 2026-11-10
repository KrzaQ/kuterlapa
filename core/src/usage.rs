//! Token statistics: how much each model used, kept in the `usage` table.
//! The engines report each model's use in a request (activity.rs); this
//! module saves it and sums it up for `kuterlapa stats`, the `usage_stats`
//! tool and the overlay panel.

use crate::activity::{tokens, Call};
use crate::profiles::Profile;
use crate::store::{Store, UsageRow, UsageSum};
use anyhow::{bail, Result};
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
        if call.input + call.cache_read + call.cache_write + call.output == 0 {
            continue;
        }
        let row = UsageRow { session, profile: &profile.name, purpose, engine: &profile.engine, call };
        if let Err(e) = store.add_usage(&row) {
            eprintln!("usage: {e:#}");
        }
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

    fn label(self) -> &'static str {
        match self {
            Period::Today => "dziś",
            Period::Week => "ostatnie 7 dni",
            Period::Month => "ostatnie 30 dni",
            Period::All => "od początku",
        }
    }
}

pub fn sums(store: &Store, period: Period) -> Result<Vec<UsageSum>> {
    store.usage_sums(period.since())
}

/// The groups in the order they are shown, with their titles.
const GROUPS: [(&str, &str, &str); 3] = [
    ("chat", "model", "Odpowiedzi i polecenia"),
    ("draft", "model", "Poprawki szkicu"),
    ("chat", "classifier", "Klasyfikator pi-censora"),
];

fn group_title(sum: &UsageSum) -> String {
    GROUPS
        .iter()
        .find(|(purpose, role, _)| *purpose == sum.purpose && *role == sum.role)
        .map(|g| g.2.to_string())
        .unwrap_or_else(|| format!("{} · {}", sum.purpose, sum.role))
}

fn group_order(title: &str) -> usize {
    GROUPS.iter().position(|g| g.2 == title).unwrap_or(GROUPS.len())
}

/// "12 wywołań", or for Claude, which reports a whole request at once,
/// "w 12 zapytaniach".
fn count(sum: &UsageSum) -> String {
    let n = sum.calls;
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

fn duration(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        _ => format!("{} godz. {} min", s / 3600, s % 3600 / 60),
    }
}

/// The numbers of a group in one line.
fn numbers(sum: &UsageSum) -> String {
    let mut parts = vec![format!("wejście {}", tokens(sum.input))];
    if sum.cache_read > 0 {
        parts.push(format!("z cache {}", tokens(sum.cache_read)));
    }
    if sum.cache_write > 0 {
        parts.push(format!("zapis do cache {}", tokens(sum.cache_write)));
    }
    let thinking = sum.thinking.filter(|&n| n > 0).map(|n| format!(" (myślenie {})", tokens(n))).unwrap_or_default();
    parts.push(format!("wyjście {}{thinking}", tokens(sum.output)));
    if let Some(n) = sum.web_searches.filter(|&n| n > 0) {
        parts.push(format!("wyszukiwania {n}"));
    }
    if let Some(cost) = sum.cost.filter(|&c| c > 0.0) {
        parts.push(format!("${cost:.2}").replace('.', ","));
    }
    if let Some(ms) = sum.ms.filter(|&n| n > 0) {
        parts.push(duration(ms));
    }
    parts.join(" · ")
}

fn model_name(sum: &UsageSum) -> String {
    format!("{}/{}", sum.provider, sum.model)
}

/// Groups in the order of GROUPS, each with its rows.
fn grouped(sums: &[UsageSum]) -> Vec<(String, Vec<&UsageSum>)> {
    let mut groups: Vec<(String, Vec<&UsageSum>)> = Vec::new();
    for sum in sums {
        let title = group_title(sum);
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
    if sums.is_empty() {
        return format!("Zużycie modeli · {}: nic.", period.label());
    }
    let mut out = format!("Zużycie modeli · {}\n", period.label());
    for (title, rows) in grouped(sums) {
        out.push_str(&format!("\n{title}\n"));
        for sum in rows {
            out.push_str(&format!("- {} — {}\n  {}\n", model_name(sum), count(sum), numbers(sum)));
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
    out.push_str(&format!("\nRazem: {}\n", numbers(&total)));
    out
}

/// The statistics in the overlay, like the help (the daemon's store).
pub fn panel(period: Period) -> serde_json::Value {
    let sums = STORE.get().and_then(|store| sums(store, period).ok()).unwrap_or_default();
    let groups: Vec<_> = grouped(&sums)
        .into_iter()
        .map(|(title, rows)| {
            let rows: Vec<_> = rows
                .into_iter()
                .map(|s| json!({ "keys": s.model, "text": format!("{} · {}", count(s), numbers(s)) }))
                .collect();
            json!({ "title": title, "rows": rows })
        })
        .collect();
    let groups = if groups.is_empty() {
        vec![json!({ "title": "Brak danych", "rows": [{ "keys": "", "text": "Żaden model jeszcze nic nie zużył." }] })]
    } else {
        groups
    };
    json!({
        "visible": true,
        "title": format!("Zużycie modeli · {}", period.label()),
        "help": groups,
        "help_key_width": 210,
        "help_columns": 1,
        "width": 780,
        "hint": "Powiedz: „pokaż zużycie z tygodnia” · B: zamknij · zamyka się sama po 30 s",
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
    fn text_groups_and_numbers() {
        let sums = vec![
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
        ];
        let text = text(&sums, Period::Today);
        let answers = text.find("Odpowiedzi").unwrap();
        let classifier = text.find("Klasyfikator").unwrap();
        assert!(answers < classifier, "{text}");
        assert!(text.contains("anthropic/claude-opus-5-5 — w 2 zapytaniach"), "{text}");
        assert!(text.contains("wejście 10 · z cache 20,0k · zapis do cache 5,0k · wyjście 300 (myślenie 120) · $0,12"), "{text}");
        assert!(text.contains("ollama/qwen — 3 wywołania\n  wejście 1,4k · wyjście 500 · 7 s"), "{text}");
        assert!(text.contains("Razem: wejście 1,4k · z cache 20,0k"), "{text}");
    }
}
