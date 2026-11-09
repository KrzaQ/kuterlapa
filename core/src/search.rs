//! Search by meaning in the models' notes and in the conversations. The
//! vectors live in the history database; each search first adds what is
//! new or changed (notes are compared by a hash of their text), then
//! ranks everything by cosine similarity to the query.

use crate::store::Store;
use crate::{embed, memory, sessions};
use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// How many texts go to the provider at once.
const BATCH: usize = 16;
/// Long messages are cut for their vector.
const TEXT_CHARS: usize = 2000;
/// Shorter messages ("tak", "ok") say nothing to search for.
const MIN_CHARS: usize = 15;

struct Item {
    key: String,
    text: String,
    /// How the result is shown: where it comes from.
    label: String,
}

fn hash(text: &str) -> String {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Everything that can be found: the notes of every session and the
/// global ones, and the messages of all conversations.
fn items(store: &Store) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for session in sessions::all() {
        for note in memory::list(&session.name) {
            let place = match note.scope {
                memory::Scope::Global => "notatka globalna".to_string(),
                memory::Scope::Session => format!("notatka sesji {}", session.name),
            };
            let key = format!("note:{place}:{}", note.name);
            if !seen.insert(key.clone()) {
                continue;
            }
            items.push(Item {
                key,
                text: format!("{}: {}\n{}", note.name, note.description, note.text),
                label: format!("{place} „{}”", note.name),
            });
        }
    }
    for (id, session, role, text, created) in store.all_messages()? {
        if text.chars().count() < MIN_CHARS {
            continue;
        }
        let who = if role == "user" { "użytkownik" } else { "asystent" };
        items.push(Item {
            key: format!("msg:{id}"),
            text: text.chars().take(TEXT_CHARS).collect(),
            label: format!("rozmowa w sesji {session}, {created}, {who}"),
        });
    }
    Ok(items)
}

/// Adds vectors for new or changed items.
fn index(store: &Store, items: &[Item]) -> Result<()> {
    let known = store.embedding_hashes()?;
    let missing: Vec<&Item> = items.iter().filter(|i| known.get(&i.key) != Some(&hash(&i.text))).collect();
    if !missing.is_empty() {
        eprintln!("search: indexing {} texts", missing.len());
    }
    for chunk in missing.chunks(BATCH) {
        let texts: Vec<String> = chunk.iter().map(|i| i.text.clone()).collect();
        let vectors = embed::vectors(&texts, false)?;
        for (item, vector) in chunk.iter().zip(vectors) {
            store.put_embedding(&item.key, &hash(&item.text), &vector)?;
        }
    }
    Ok(())
}

/// The `limit` items closest in meaning to `query`, as text for a model.
pub fn search(store: &Store, query: &str, limit: usize) -> Result<String> {
    let items = items(store)?;
    index(store, &items)?;
    let query = embed::vectors(&[query.to_string()], true)?.remove(0);
    let vectors = store.embeddings()?;
    let mut scored: Vec<(f32, &Item)> = items
        .iter()
        .filter_map(|item| {
            let v = vectors.get(&item.key)?;
            Some((v.iter().zip(&query).map(|(a, b)| a * b).sum(), item))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    if scored.is_empty() {
        return Ok("Nothing to search yet.".into());
    }
    let mut out = String::new();
    for (score, item) in scored.into_iter().take(limit) {
        let text: String = item.text.chars().take(500).collect();
        out.push_str(&format!("[{:.2}] {}:\n{}\n\n", score, item.label, text.trim()));
    }
    Ok(out.trim_end().to_string())
}
