//! Importing collections, environments and requests from other tools
//! (F11-AC2..AC5, F30, ADR-0053).
//!
//! Importers are pure: text in, the files to create and a list of what was
//! not carried over out. `ferry-app` writes the result with workspace
//! operations, so nothing is written unless the whole text was read.

use std::fmt;

use serde_json::Value;

use crate::collection::{CollectionFile, FolderFile, Item, Node, RequestFile};
use crate::curl;
use crate::draft::RequestDraft;
use crate::environment::VariablesFile;

pub mod postman;

/// Shown when the text is none of the formats Ferry reads (F11-AC2).
pub const NOT_IMPORTABLE: &str = "Ferry cannot import this: it is not a Postman collection or environment, OpenAPI, Swagger, HAR or cURL command";

// CONTRACT
/// Why text could not be imported. `Display` is the exact text shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError(pub String);

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImportError {}

// CONTRACT
/// A collection to create: its settings and its items in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Collection {
    pub file: CollectionFile,
    pub items: Vec<Entry>,
}

// CONTRACT
/// A folder with its items, or a request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // a few hundred bytes per item
pub enum Entry {
    Folder { file: FolderFile, items: Vec<Entry> },
    Request(RequestFile),
}

impl Collection {
    // CONTRACT
    /// The number of requests and of folders, at every depth.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        fn walk(items: &[Entry], counts: &mut (usize, usize)) {
            for item in items {
                match item {
                    Entry::Folder { items, .. } => {
                        counts.1 += 1;
                        walk(items, counts);
                    }
                    Entry::Request(_) => counts.0 += 1,
                }
            }
        }
        let mut counts = (0, 0);
        walk(&self.items, &mut counts);
        counts
    }
}

// CONTRACT
/// The collection at the root of a workspace tree, for export; folders and
/// requests that could not be read are left out. `None` for anything else.
#[must_use]
pub fn from_tree(item: &Item) -> Option<Collection> {
    fn entries(items: &[Item]) -> Vec<Entry> {
        items
            .iter()
            .filter_map(|item| match &item.node {
                Node::Folder(file) => Some(Entry::Folder {
                    file: file.clone(),
                    items: entries(&item.items),
                }),
                Node::Request(file) => Some(Entry::Request(file.clone())),
                Node::Collection(_) | Node::Broken { .. } => None,
            })
            .collect()
    }
    match &item.node {
        Node::Collection(file) => Some(Collection {
            file: file.clone(),
            items: entries(&item.items),
        }),
        _ => None,
    }
}

// CONTRACT
/// What the text turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Import {
    Collection(Collection),
    Environment(VariablesFile),
    /// A cURL command, which opens in a new tab.
    Request(RequestDraft),
}

// CONTRACT
/// An import and each thing it did not carry over, one line each
/// (F11-AC4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub import: Import,
    pub skipped: Vec<String>,
}

// CONTRACT
/// Read `text` as whichever supported format it is (F11-AC2).
///
/// # Errors
/// [`ImportError`] with the text to show when it is no supported format or
/// cannot be read as a whole.
pub fn import(text: &str) -> Result<Imported, ImportError> {
    let text = text.trim_start_matches('\u{feff}').trim();
    if curl::is_curl(text) {
        return curl::parse(text)
            .map(|draft| Imported {
                import: Import::Request(draft),
                skipped: Vec::new(),
            })
            .map_err(|e| ImportError(e.to_string()));
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) if text.starts_with('{') => {
            return Err(ImportError(format!(
                "Ferry cannot import this: it is not valid JSON (line {}, column {})",
                e.line(),
                e.column()
            )));
        }
        Err(_) => return Err(ImportError(NOT_IMPORTABLE.to_owned())),
    };
    if postman::is_collection(&value) {
        Ok(postman::import_collection(&value))
    } else if postman::is_environment(&value) {
        Ok(postman::import_environment(&value))
    } else {
        Err(ImportError(NOT_IMPORTABLE.to_owned()))
    }
}

/// A JSON value as text: strings as they are, `null` as empty, anything
/// else as JSON.
pub(crate) fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The items of a JSON array, or none.
pub(crate) fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

/// `text`, or `fallback` when it is empty.
pub(crate) fn or(text: String, fallback: &str) -> String {
    if text.trim().is_empty() {
        fallback.to_owned()
    } else {
        text
    }
}
