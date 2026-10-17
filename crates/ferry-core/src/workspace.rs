//! The workspace as loaded from disk, and the file operations that change
//! it (F08-AC8..AC11, F09, ADR-0037).
//!
//! `ferry-app` decides every operation; `ferry-store` executes them and
//! loads a fresh [`Workspace`].

use std::collections::BTreeMap;

use crate::collection::{self, Item, Node, RequestFile};
use crate::environment::VariablesFile;

/// The globals file inside the workspace.
pub const GLOBALS_FILE: &str = "globals.toml";
/// Directory of environments inside the workspace.
pub const ENVIRONMENTS_DIR: &str = "environments";
/// The `.gitignore` line that keeps local files out of git (F09-AC1).
pub const GITIGNORE_LINE: &str = "*.local.toml";

// CONTRACT
/// One environment file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentEntry {
    /// `environments/<file>.toml`.
    pub path: String,
    /// The parsed file, or why it could not be read (F08-AC11).
    pub file: Result<VariablesFile, String>,
}

impl EnvironmentEntry {
    // CONTRACT
    /// The environment's name, or its file name without `.toml`.
    #[must_use]
    pub fn name(&self) -> &str {
        if let Ok(VariablesFile {
            name: Some(name), ..
        }) = &self.file
        {
            name
        } else {
            let file = collection::file_name(&self.path);
            file.strip_suffix(".toml").unwrap_or(file)
        }
    }

    // CONTRACT
    /// `Could not read <path>: <reason>` for an unreadable file.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        self.file
            .as_ref()
            .err()
            .map(|reason| format!("Could not read {}: {reason}", self.path))
    }
}

// CONTRACT
/// Everything Ferry reads from the workspace folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub collections: Vec<Item>,
    /// Sorted by name, ignoring case.
    pub environments: Vec<EnvironmentEntry>,
    /// `globals.toml`, empty when missing, or why it could not be read.
    pub globals: Result<VariablesFile, String>,
    /// The readable `_order.toml` lists, by directory (F09-AC3).
    pub orders: BTreeMap<String, Vec<String>>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            collections: Vec::new(),
            environments: Vec::new(),
            globals: Ok(VariablesFile::default()),
            orders: BTreeMap::new(),
        }
    }
}

impl Workspace {
    // CONTRACT
    /// The environment file at `path`.
    #[must_use]
    pub fn environment(&self, path: &str) -> Option<&EnvironmentEntry> {
        self.environments.iter().find(|e| e.path == path)
    }

    // CONTRACT
    /// The readable request file at `path`.
    #[must_use]
    pub fn request(&self, path: &str) -> Option<&RequestFile> {
        match collection::find(&self.collections, path).map(|item| &item.node) {
            Some(Node::Request(file)) => Some(file),
            _ => None,
        }
    }
}

// CONTRACT
/// One change to the workspace folder. Paths are relative to the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Create a directory and its parents.
    CreateDir(String),
    /// Replace a file atomically, creating parent directories.
    Write { path: String, contents: String },
    /// Delete a file or a directory tree; a missing path is not an error.
    Remove(String),
    /// Rename a file or directory; fails if `to` exists.
    Rename { from: String, to: String },
    /// Copy a file or directory tree; fails if `to` exists.
    Copy { from: String, to: String },
}

// CONTRACT
/// The `.gitignore` text with [`GITIGNORE_LINE`] added, or `None` when it
/// already has it. Other lines are kept.
#[must_use]
pub fn gitignore_with_local(existing: Option<&str>) -> Option<String> {
    let existing = existing.unwrap_or_default();
    if existing.lines().any(|line| line.trim() == GITIGNORE_LINE) {
        return None;
    }
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(GITIGNORE_LINE);
    out.push('\n');
    Some(out)
}
