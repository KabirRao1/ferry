//! Two-way sync between the URL text and the params table (F01-AC2..AC4,
//! F01-AC6).
//!
//! Like Postman, the table shows each pair as written in the URL, without
//! percent-decoding; encoding happens when the request is built. The URL is
//! plain text here, so `{{var}}` placeholders and half-typed URLs survive.

use serde::{Deserialize, Serialize};

use crate::kv::KeyValue;

// CONTRACT
/// One row of the params table. Rows compare by their visible content;
/// `original` is bookkeeping.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Param {
    #[serde(flatten)]
    pub row: KeyValue,
    /// The pair exactly as it appeared in the URL, so an untouched row is
    /// written back unchanged. Ignored once the row no longer matches it.
    #[serde(skip)]
    pub original: Option<String>,
}

impl PartialEq for Param {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row
    }
}

impl Eq for Param {}

impl Param {
    // CONTRACT
    /// An enabled row with no original text.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            row: KeyValue::new(key, value),
            original: None,
        }
    }
}

/// The URL split around its query: `base ? query # fragment`.
struct Parts<'a> {
    base: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn split(url: &str) -> Parts<'_> {
    let (rest, fragment) = match url.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (url, None),
    };
    let (base, query) = match rest.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (rest, None),
    };
    Parts {
        base,
        query,
        fragment,
    }
}

/// `key` and `value` of one `key=value` segment; no `=` means an empty value.
fn pair(segment: &str) -> (&str, &str) {
    segment.split_once('=').unwrap_or((segment, ""))
}

// CONTRACT
/// The query pairs of `url`, in order, as written. Empty segments are skipped.
#[must_use]
pub fn parse_query(url: &str) -> Vec<(String, String)> {
    split(url)
        .query
        .into_iter()
        .flat_map(|q| q.split('&'))
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let (key, value) = pair(segment);
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

// CONTRACT
/// The table after the user edited the URL: enabled rows are replaced by the
/// URL's pairs, while disabled rows in `previous` keep their positions.
/// Descriptions stay with rows whose key is unchanged at the same position.
#[must_use]
pub fn rows_from_url(url: &str, previous: &[Param]) -> Vec<Param> {
    let segments: Vec<&str> = split(url)
        .query
        .into_iter()
        .flat_map(|q| q.split('&'))
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut fresh = segments.into_iter().map(|segment| {
        let (key, value) = pair(segment);
        Param {
            row: KeyValue::new(key, value),
            original: Some(segment.to_owned()),
        }
    });
    let mut rows = Vec::with_capacity(previous.len());
    for old in previous {
        if !old.row.enabled {
            rows.push(old.clone());
        } else if let Some(mut param) = fresh.next() {
            if param.row.key == old.row.key {
                param.row.description.clone_from(&old.row.description);
            }
            rows.push(param);
        }
    }
    rows.extend(fresh);
    rows
}

// CONTRACT
/// The URL after the user edited the table: the query is rebuilt from enabled
/// rows with a non-empty key. An untouched row is written as it appeared; an
/// edited one as typed, with the characters that would change the query's
/// structure escaped (see [`escape`]). A row with an
/// empty value is written as its key alone. The fragment is kept, and an
/// empty query drops the `?`.
#[must_use]
pub fn url_with_rows(url: &str, rows: &[Param]) -> String {
    let parts = split(url);
    let query: Vec<String> = rows
        .iter()
        .filter(|p| p.row.enabled && !p.row.key.is_empty())
        .map(segment)
        .collect();
    let mut out = String::with_capacity(url.len());
    out.push_str(parts.base);
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    if let Some(fragment) = parts.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

fn segment(param: &Param) -> String {
    if let Some(original) = &param.original {
        let (key, value) = pair(original);
        if key == param.row.key && value == param.row.value {
            return original.clone();
        }
    }
    let key = escape(&param.row.key, true);
    if param.row.value.is_empty() {
        key
    } else {
        format!("{key}={}", escape(&param.row.value, false))
    }
}

// CONTRACT
/// Escape the characters that would change the query's structure.
#[must_use]
pub fn escape(text: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '=' if is_key => out.push_str("%3D"),
            c => out.push(c),
        }
    }
    out
}
