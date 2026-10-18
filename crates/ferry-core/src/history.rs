//! Request history (F10, ADR-0012, ADR-0039).
//!
//! Entries hold metadata and the request as edited; response bodies are
//! stored separately by `ferry-store`, keyed by entry id.

use serde::{Deserialize, Serialize};

use crate::collection::{FormatError, toml_error};
use crate::draft::RequestDraft;
use crate::environment::to_toml;
use crate::response::Header;
use crate::time;

/// Entries kept (F10-AC2).
pub const MAX_ENTRIES: usize = 100;
/// Bytes of each response body kept (F10-AC2).
pub const MAX_BODY: usize = 1024 * 1024;

// CONTRACT
/// How a recorded send ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Outcome {
    Response {
        status: u16,
        #[serde(default)]
        reason: String,
        elapsed_ms: u64,
        /// The whole body's size, before truncation.
        size: u64,
        #[serde(default)]
        headers: Vec<Header>,
        /// The stored body is the first [`MAX_BODY`] bytes only.
        #[serde(default)]
        truncated: bool,
    },
    Failed {
        error: String,
    },
}

// CONTRACT
/// One recorded send (F10-AC1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    /// Milliseconds since the Unix epoch.
    pub at: i64,
    pub method: String,
    /// The URL as sent, variables resolved.
    pub url: String,
    /// The request as edited, placeholders kept.
    #[serde(default)]
    pub request: RequestDraft,
    pub outcome: Outcome,
}

// CONTRACT
/// The history, newest entry first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct History {
    /// The id the next entry gets.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl History {
    // CONTRACT
    /// Add `entry` as the newest, giving it the next id. Returns its id and
    /// the ids of entries dropped to stay within [`MAX_ENTRIES`].
    pub fn record(&mut self, mut entry: Entry) -> (u64, Vec<u64>) {
        let id = self.next_id;
        self.next_id += 1;
        entry.id = id;
        self.entries.insert(0, entry);
        let dropped = self
            .entries
            .split_off(self.entries.len().min(MAX_ENTRIES))
            .into_iter()
            .map(|e| e.id)
            .collect();
        (id, dropped)
    }

    // CONTRACT
    /// Remove an entry; `true` if it existed.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    // CONTRACT
    /// Remove every entry, returning their ids.
    pub fn clear(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.entries)
            .into_iter()
            .map(|e| e.id)
            .collect()
    }

    // CONTRACT
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    // CONTRACT
    /// Entries whose URL or method contains `query`, ignoring case, newest
    /// first; all of them for an empty query (F10-AC3).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&Entry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.url.to_lowercase().contains(&query) || e.method.to_lowercase().contains(&query)
            })
            .collect()
    }
}

// CONTRACT
/// `entries` (newest first) grouped by local day: `Today`, `Yesterday`,
/// `October 15`, or `October 15, 2025` for another year (F10-AC3).
#[must_use]
pub fn groups<'a>(
    entries: &[&'a Entry],
    now: i64,
    utc_offset: i32,
) -> Vec<(String, Vec<&'a Entry>)> {
    let today = time::date_at(now, utc_offset);
    let yesterday = time::date_at(now - 86_400_000, utc_offset);
    let mut out: Vec<(time::Date, Vec<&'a Entry>)> = Vec::new();
    for entry in entries {
        let date = time::date_at(entry.at, utc_offset);
        match out.last_mut() {
            Some((last, items)) if *last == date => items.push(entry),
            _ => out.push((date, vec![entry])),
        }
    }
    out.into_iter()
        .map(|(date, items)| {
            let label = if date == today {
                "Today".to_owned()
            } else if date == yesterday {
                "Yesterday".to_owned()
            } else if date.year == today.year {
                format!("{} {}", time::month_name(date.month), date.day)
            } else {
                format!(
                    "{} {}, {}",
                    time::month_name(date.month),
                    date.day,
                    date.year
                )
            };
            (label, items)
        })
        .collect()
}

// CONTRACT
/// Parse `history.toml`.
///
/// # Errors
/// [`FormatError`] if the text is not valid.
pub fn parse(text: &str) -> Result<History, FormatError> {
    toml::from_str(text).map_err(|e| toml_error(text, &e))
}

// CONTRACT
#[must_use]
pub fn write(history: &History) -> String {
    to_toml(history)
}
