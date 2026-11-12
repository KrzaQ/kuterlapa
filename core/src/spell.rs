//! Spelled words, back to letters. Speech recognition writes "K-o-t-e-k"
//! spoken in Polish as "K-o-ty-e-ky": the names of the letters, often
//! changed. A table for each language (spelling/<code>.toml, built in, or
//! the user's own in ~/.config/kuterlapa/spelling/) lists the names of each
//! letter and of a few symbols; the decoder matches them, the longest
//! names first, and reads "K jak Katarzyna" (Polish) or "K as in Kilo" as
//! one letter. It gives a best guess and says which parts it did not know:
//! a model decides the rest.

use crate::paths;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

const BUILT_IN: [(&str, &str); 2] =
    [("pl", include_str!("../../spelling/pl.toml")), ("en", include_str!("../../spelling/en.toml"))];

#[derive(Deserialize, Default)]
struct Table {
    #[serde(default)]
    connectors: Vec<String>,
    #[serde(default)]
    letters: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    symbols: BTreeMap<String, Vec<String>>,
}

/// The table of a language: the user's own file, or the built-in one.
fn table(language: &str) -> Result<Table> {
    let code = language.trim().to_lowercase();
    let own = paths::config_dir().join("spelling").join(format!("{code}.toml"));
    let text = match std::fs::read_to_string(&own) {
        Ok(text) => text,
        Err(_) => match BUILT_IN.iter().find(|(c, _)| *c == code) {
            Some((_, text)) => text.to_string(),
            None => bail!(
                "no spelling table for {code:?}: built in are {}; add {}",
                BUILT_IN.map(|(c, _)| c).join(", "),
                own.display()
            ),
        },
    };
    toml::from_str(&text).with_context(|| format!("reading the spelling table for {code}"))
}

/// The language of the speech, from speech.toml.
pub fn speech_language() -> String {
    std::fs::read_to_string(paths::config_dir().join("speech.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("language").and_then(|v| v.as_str()).map(str::to_string))
        .unwrap_or_else(|| "en".into())
}

/// The decoder's answer: its best guess, and the parts it did not know.
#[derive(Debug, PartialEq)]
pub struct Decoded {
    pub text: String,
    pub unknown: Vec<String>,
}

/// The pieces of spelled speech: words, with "-", "," and "." around
/// letters taken apart ("ty.cy" is "ty", ".", "cy").
fn pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Dashes of every kind separate letters: speech recognition may write
    // a hyphen (‐ ‑), an en or em dash (– —) or a minus sign (−).
    let separator = |c: char| {
        c.is_whitespace() || matches!(c, '-' | '‐' | '‑' | '‒' | '–' | '—' | '−' | ',' | ';' | '!' | '?' | '"' | '„' | '”')
    };
    for word in text.split(separator) {
        let mut current = String::new();
        let mut previous_lower = false;
        for c in word.chars() {
            // Speech recognition glues spelled parts in capitals:
            // "MałpaKotek" is "Małpa", "Kotek".
            if c.is_uppercase() && previous_lower && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            previous_lower = c.is_lowercase();
            if c == '.' {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(".".into());
            } else {
                current.extend(c.to_lowercase());
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    // A full stop at the very end ends the sentence; it is not spelled.
    if out.last().map(String::as_str) == Some(".") {
        out.pop();
    }
    out
}

pub fn decode(text: &str, language: &str) -> Result<Decoded> {
    Ok(decode_with(&table(language)?, text))
}

fn decode_with(table: &Table, text: &str) -> Decoded {
    // Every name, lowercased, with what it stands for; the longest names
    // (in words) are tried first.
    let mut names: Vec<(Vec<String>, String)> = Vec::new();
    for (letter, list) in &table.letters {
        for name in list {
            names.push((name.to_lowercase().split_whitespace().map(str::to_string).collect(), letter.clone()));
        }
    }
    for (symbol, list) in &table.symbols {
        for name in list {
            names.push((name.to_lowercase().split_whitespace().map(str::to_string).collect(), symbol.clone()));
        }
    }
    names.sort_by_key(|(words, _)| std::cmp::Reverse(words.len()));
    let connectors: Vec<Vec<String>> = table
        .connectors
        .iter()
        .map(|c| c.to_lowercase().split_whitespace().map(str::to_string).collect())
        .collect();

    let pieces = pieces(text);
    let starts = |at: usize, words: &[String]| pieces.get(at..at + words.len()) == Some(words);
    let (mut out, mut unknown) = (String::new(), Vec::new());
    let mut i = 0;
    while i < pieces.len() {
        // "K jak Katarzyna": the letter came already; skip the connector
        // and the word after it.
        if let Some(c) = connectors.iter().find(|c| starts(i, c)) {
            if out.is_empty() {
                // No letter before it: the word gives it.
                if let Some(word) = pieces.get(i + c.len()) {
                    out.extend(word.chars().next());
                }
            }
            i += c.len() + 1;
            continue;
        }
        if let Some((words, value)) = names.iter().find(|(words, _)| starts(i, words)) {
            out.push_str(value);
            i += words.len();
            continue;
        }
        let piece = &pieces[i];
        if piece.chars().count() == 1 {
            out.push_str(piece);
        } else {
            // Not a letter's name: a part of a word, kept as heard.
            out.push_str(piece);
            unknown.push(piece.clone());
        }
        i += 1;
    }
    Decoded { text: out, unknown }
}

/// The answer for a model and for `kuterlapa spell`.
pub fn describe(decoded: &Decoded, language: &str) -> String {
    let mut text = format!("Best guess ({language}): {}", decoded.text);
    if !decoded.unknown.is_empty() {
        text.push_str(&format!(
            "\nNot letter names, kept as heard: {}. Decide from the context what they were.",
            decoded.unknown.join(", ")
        ));
    }
    text
}

/// `kuterlapa spell [language] <text>`: language from speech.toml when the
/// first word is not a table's code.
pub fn run(args: &[String]) -> Result<()> {
    let (language, rest) = match args.split_first() {
        Some((first, rest)) if first.len() == 2 && table(first).is_ok() && !rest.is_empty() => (first.clone(), rest),
        _ => (speech_language(), args),
    };
    if rest.is_empty() {
        bail!("usage: kuterlapa spell [language] <spelled text>");
    }
    let decoded = decode(&rest.join(" "), &language)?;
    println!("{}", describe(&decoded, &language));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(text: &str) -> Decoded {
        decode_with(&toml::from_str(BUILT_IN[0].1).unwrap(), text)
    }

    fn en(text: &str) -> Decoded {
        decode_with(&toml::from_str(BUILT_IN[1].1).unwrap(), text)
    }

    #[test]
    fn polish_letter_names_as_heard() {
        // Spelled "kotek.com", as Whisper writes such spelling.
        let d = pl("K-o-ty-e-ky.cy-o-my");
        assert_eq!(d.text, "kotek.com");
        assert!(d.unknown.is_empty());
    }

    #[test]
    fn polish_words_and_symbols() {
        assert_eq!(pl("K jak Katarzyna, E jak Ewa, R jak Ryszard").text, "ker");
        assert_eq!(pl("jan małpa kotek kropka pl").text, "jan@kotek.pl");
        assert_eq!(pl("MałpaKotek.pl").text, "@kotek.pl");
        let d = pl("ka i e kot");
        assert_eq!(d.text, "kiekot");
        assert_eq!(d.unknown, vec!["kot".to_string()]);
    }

    #[test]
    fn any_dash_separates() {
        assert_eq!(pl("K–o—ty‐e‑ky").text, "kotek");
        assert_eq!(pl("jan łącznik kowal").text, "jan-kowal");
    }

    #[test]
    fn english_names_and_nato() {
        assert_eq!(en("Kilo India Echo, double u, dot com").text, "kiew.com");
        assert_eq!(en("K as in Kilo, E for Echo").text, "ke");
        assert_eq!(en("bee ar ay").text, "bra");
    }
}
