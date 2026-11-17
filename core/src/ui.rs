//! The language of what Kuterlapa shows and says, and of the prompts it
//! gives the models: English, or Polish (`language` in config.toml).
//!
//! Texts stand in the code in both languages, next to each other, so that a
//! change to one shows the other right there: `tr("Słucham…", "Listening…")`.

use std::sync::OnceLock;

static POLISH: OnceLock<bool> = OnceLock::new();

/// Whether the user chose Polish. Read once; a change needs a restart.
pub fn polish() -> bool {
    *POLISH.get_or_init(|| crate::config::load().map(|c| c.language.trim() == "pl").unwrap_or(false))
}

/// The Polish or the English text.
pub fn tr(pl: &'static str, en: &'static str) -> &'static str {
    if polish() {
        pl
    } else {
        en
    }
}
