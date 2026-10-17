//! Editable key-value rows shared by params, headers and form bodies, and
//! their bulk-edit text form (F01-AC8, F02-AC1, F02-AC4).

use serde::{Deserialize, Serialize};

// CONTRACT
/// One row of a key-value table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    /// Shown in the table only; never sent.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "crate::is_true")]
    pub enabled: bool,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            description: String::new(),
            enabled: true,
        }
    }
}

impl KeyValue {
    // CONTRACT
    /// An enabled row with no description.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    // CONTRACT
    /// Whether the row takes part in the request: enabled with a non-blank key.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

// CONTRACT
/// Rows as bulk-edit text: one `key: value` line per row, disabled rows
/// prefixed with `//`. Rows that are entirely empty are left out.
#[must_use]
pub fn to_bulk(rows: &[KeyValue]) -> String {
    let mut out = String::new();
    for row in rows {
        if row.key.is_empty() && row.value.is_empty() {
            continue;
        }
        if !row.enabled {
            out.push_str("//");
        }
        out.push_str(&row.key);
        out.push_str(": ");
        out.push_str(&row.value);
        out.push('\n');
    }
    out
}

// CONTRACT
/// Parse bulk-edit text back into rows. A line's key is the text before the
/// first `:`, trimmed; its value is the rest with one leading space removed.
/// Blank lines are skipped. A row keeps the description of the first unused
/// row in `previous` with the same key.
#[must_use]
pub fn from_bulk(text: &str, previous: &[KeyValue]) -> Vec<KeyValue> {
    let mut used = vec![false; previous.len()];
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (enabled, line) = match line.trim_start().strip_prefix("//") {
            Some(rest) => (false, rest),
            None => (true, line),
        };
        let (key, value) = match line.split_once(':') {
            Some((key, value)) => (key.trim(), value.strip_prefix(' ').unwrap_or(value)),
            None => (line.trim(), ""),
        };
        let description = previous
            .iter()
            .enumerate()
            .find(|(i, row)| !used[*i] && row.key == key)
            .map(|(i, row)| {
                used[i] = true;
                row.description.clone()
            })
            .unwrap_or_default();
        rows.push(KeyValue {
            key: key.to_owned(),
            value: value.to_owned(),
            description,
            enabled,
        });
    }
    rows
}
