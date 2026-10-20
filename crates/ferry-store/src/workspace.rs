//! The workspace folder: loading collections, environments and globals, and
//! applying the file operations `ferry-app` decides (F08, F09, ADR-0037).

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use ferry_core::collection::{
    self, COLLECTION_FILE, COLLECTIONS_DIR, CollectionFile, FOLDER_FILE, FolderFile, Item,
    ItemKind, Node, ORDER_FILE, REQUEST_EXT,
};
use ferry_core::environment::{self, VariablesFile};
use ferry_core::workspace::{
    ENVIRONMENTS_DIR, EnvironmentEntry, GLOBALS_FILE, Op, Workspace, gitignore_with_local,
};

use crate::fs::Fs;

const GITIGNORE: &str = ".gitignore";
const LOCAL_EXT: &str = ".local.toml";

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Whether a file in a collection folder is a request file.
fn is_request_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    !is_hidden(name)
        && lower.ends_with(REQUEST_EXT)
        && !lower.ends_with(LOCAL_EXT)
        && ![COLLECTION_FILE, FOLDER_FILE, ORDER_FILE].contains(&lower.as_str())
}

/// `parent/name` with `/` separators.
fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

// CONTRACT
/// Why a workspace operation failed. `Display` is the reason shown after
/// `Could not <action>: `.
#[derive(Debug)]
pub struct OpError {
    /// The workspace-relative path involved.
    pub path: String,
    pub source: io::Error,
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.source)
    }
}

impl std::error::Error for OpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

// CONTRACT
/// Reads and writes one workspace folder.
pub struct WorkspaceStore<F: Fs> {
    fs: F,
    root: PathBuf,
}

impl<F: Fs> WorkspaceStore<F> {
    // CONTRACT
    #[must_use]
    pub fn new(fs: F, root: &Path) -> Self {
        Self {
            fs,
            root: root.to_path_buf(),
        }
    }

    // CONTRACT
    /// The workspace folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    // CONTRACT
    /// Create the folder with `collections/` and `environments/` if needed,
    /// and make sure `.gitignore` lists `*.local.toml` (F09-AC1).
    ///
    /// # Errors
    /// [`OpError`] if the folder cannot be prepared.
    pub fn init(&self) -> Result<(), OpError> {
        for dir in [COLLECTIONS_DIR, ENVIRONMENTS_DIR] {
            let path = self.root.join(dir);
            self.fs.create_dir_all(&path).map_err(|source| OpError {
                path: dir.to_owned(),
                source,
            })?;
        }
        self.ensure_gitignore()
    }

    fn ensure_gitignore(&self) -> Result<(), OpError> {
        let error = |source| OpError {
            path: GITIGNORE.to_owned(),
            source,
        };
        let path = self.root.join(GITIGNORE);
        let existing = match self.fs.read(&path) {
            Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(error(e)),
        };
        match gitignore_with_local(existing.as_deref()) {
            Some(text) => self.fs.write_atomic(&path, text.as_bytes()).map_err(error),
            None => Ok(()),
        }
    }

    /// A file's text: `Ok(None)` if it does not exist.
    fn read_text(&self, relative: &str) -> Result<Option<String>, String> {
        match self.fs.read(&self.root.join(relative)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "the file is not valid UTF-8".to_owned()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn list(&self, relative: &str) -> Vec<crate::fs::DirEntry> {
        self.fs
            .read_dir(&self.root.join(relative))
            .unwrap_or_default()
    }

    /// A collection or folder directory and everything inside it. Order
    /// files found on the way are added to `orders`.
    fn load_container(
        &self,
        path: &str,
        kind: ItemKind,
        orders: &mut BTreeMap<String, Vec<String>>,
    ) -> Item {
        let dir_name = collection::file_name(path);
        let settings = if kind == ItemKind::Collection {
            COLLECTION_FILE
        } else {
            FOLDER_FILE
        };
        let node = match self.read_text(&join(path, settings)) {
            Ok(None) if kind == ItemKind::Collection => Node::Collection(CollectionFile {
                name: dir_name.to_owned(),
                ..CollectionFile::default()
            }),
            Ok(None) => Node::Folder(FolderFile {
                name: dir_name.to_owned(),
                ..FolderFile::default()
            }),
            Ok(Some(text)) if kind == ItemKind::Collection => {
                match collection::parse_collection(&text, dir_name) {
                    Ok(file) => Node::Collection(file),
                    Err(e) => Node::Broken {
                        kind,
                        reason: e.to_string(),
                    },
                }
            }
            Ok(Some(text)) => match collection::parse_folder(&text, dir_name) {
                Ok(file) => Node::Folder(file),
                Err(e) => Node::Broken {
                    kind,
                    reason: e.to_string(),
                },
            },
            Err(reason) => Node::Broken { kind, reason },
        };
        // Values scripts set live in `collection.local.toml` (F15-AC5).
        let node = match node {
            Node::Collection(mut file) => {
                let local = environment::local_path(&join(path, COLLECTION_FILE));
                let merged = self.read_text(&local).and_then(|text| match text {
                    Some(text) => collection::merge_collection_local(&mut file, &text)
                        .map_err(|e| format!("{}: {e}", collection::file_name(&local))),
                    None => Ok(()),
                });
                match merged {
                    Ok(()) => Node::Collection(file),
                    Err(reason) => Node::Broken { kind, reason },
                }
            }
            other => other,
        };
        let mut items = Vec::new();
        for entry in self.list(path) {
            let child = join(path, &entry.name);
            if entry.is_dir && !is_hidden(&entry.name) {
                items.push(self.load_container(&child, ItemKind::Folder, orders));
            } else if !entry.is_dir && is_request_file(&entry.name) {
                items.push(self.load_request(&child));
            }
        }
        let order = match self.read_text(&join(path, ORDER_FILE)) {
            Ok(Some(text)) => collection::parse_order(&text).ok(),
            _ => None,
        };
        if let Some(order) = &order {
            orders.insert(path.to_owned(), order.clone());
        }
        let order = order.unwrap_or_default();
        Item {
            path: path.to_owned(),
            node,
            items: collection::arrange(items, &order),
        }
    }

    fn load_request(&self, path: &str) -> Item {
        let file_name = collection::file_name(path);
        let stem = &file_name[..file_name.len() - REQUEST_EXT.len()];
        let node = match self.read_text(path) {
            Ok(Some(text)) => match collection::parse_request(&text, stem) {
                Ok(file) => Node::Request(file),
                Err(e) => Node::Broken {
                    kind: ItemKind::Request,
                    reason: e.to_string(),
                },
            },
            Ok(None) => Node::Broken {
                kind: ItemKind::Request,
                reason: "the file disappeared while loading".to_owned(),
            },
            Err(reason) => Node::Broken {
                kind: ItemKind::Request,
                reason,
            },
        };
        Item {
            path: path.to_owned(),
            node,
            items: Vec::new(),
        }
    }

    /// A shared variables file and its local file; `Ok(None)` if neither
    /// exists.
    fn load_variables(&self, path: &str) -> Result<Option<VariablesFile>, String> {
        let shared = self.read_text(path)?;
        let local = self.read_text(&environment::local_path(path))?;
        if shared.is_none() && local.is_none() {
            return Ok(None);
        }
        environment::parse(shared.as_deref().unwrap_or_default(), local.as_deref())
            .map(Some)
            .map_err(|e| e.to_string())
    }

    // CONTRACT
    /// Read everything (F08-AC10, AC11, F09-AC2, AC3, AC12). Never fails:
    /// unreadable files become problems inside the [`Workspace`]. Hidden
    /// entries (starting with `.`) and other files are ignored.
    #[must_use]
    pub fn load(&self) -> Workspace {
        let mut orders = BTreeMap::new();
        let collections = self
            .list(COLLECTIONS_DIR)
            .into_iter()
            .filter(|e| e.is_dir && !is_hidden(&e.name))
            .map(|e| {
                self.load_container(
                    &join(COLLECTIONS_DIR, &e.name),
                    ItemKind::Collection,
                    &mut orders,
                )
            })
            .collect();
        let collections = collection::arrange(collections, &[]);
        let mut environments: Vec<EnvironmentEntry> = self
            .list(ENVIRONMENTS_DIR)
            .into_iter()
            .filter(|e| !e.is_dir && is_request_file(&e.name))
            .map(|e| {
                let path = join(ENVIRONMENTS_DIR, &e.name);
                let file = self.load_variables(&path).map(Option::unwrap_or_default);
                EnvironmentEntry { path, file }
            })
            .collect();
        environments.sort_by_cached_key(|e| (e.name().to_lowercase(), e.path.clone()));
        let globals = self
            .load_variables(GLOBALS_FILE)
            .map(Option::unwrap_or_default);
        Workspace {
            collections,
            environments,
            globals,
            orders,
        }
    }

    // CONTRACT
    /// Apply `ops` in order, stopping at the first failure. Every path must
    /// be relative and stay inside the workspace. `.gitignore` is checked
    /// first (F09-AC1).
    ///
    /// # Errors
    /// [`OpError`] for the operation that failed; earlier ones stay applied.
    pub fn apply(&self, ops: &[Op]) -> Result<(), OpError> {
        self.ensure_gitignore()?;
        for op in ops {
            self.apply_one(op)?;
        }
        Ok(())
    }

    fn apply_one(&self, op: &Op) -> Result<(), OpError> {
        let fail = |path: &str, source: io::Error| OpError {
            path: path.to_owned(),
            source,
        };
        match op {
            Op::CreateDir(path) => {
                let abs = self.resolve(path)?;
                self.fs.create_dir_all(&abs).map_err(|e| fail(path, e))
            }
            Op::Write { path, contents } => {
                let abs = self.resolve(path)?;
                self.fs
                    .write_atomic(&abs, contents.as_bytes())
                    .map_err(|e| fail(path, e))
            }
            Op::Remove(path) => {
                let abs = self.resolve(path)?;
                match self.fs.kind(&abs).map_err(|e| fail(path, e))? {
                    None => Ok(()),
                    Some(true) => self.fs.remove_dir_all(&abs).map_err(|e| fail(path, e)),
                    Some(false) => self.fs.remove_file(&abs).map_err(|e| fail(path, e)),
                }
            }
            Op::Rename { from, to } => {
                let (abs_from, abs_to) = (self.resolve(from)?, self.resolve(to)?);
                // A change of case only is the same entry on case-insensitive
                // filesystems, so it may already "exist".
                if !from.eq_ignore_ascii_case(to) {
                    self.must_not_exist(to, &abs_to)?;
                }
                self.fs
                    .rename(&abs_from, &abs_to)
                    .map_err(|e| fail(from, e))
            }
            Op::Copy { from, to } => {
                let (abs_from, abs_to) = (self.resolve(from)?, self.resolve(to)?);
                self.must_not_exist(to, &abs_to)?;
                self.copy(from, &abs_from, &abs_to)
            }
        }
    }

    fn must_not_exist(&self, relative: &str, abs: &Path) -> Result<(), OpError> {
        match self.fs.kind(abs) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(OpError {
                path: relative.to_owned(),
                source: io::Error::new(io::ErrorKind::AlreadyExists, "already exists"),
            }),
            Err(source) => Err(OpError {
                path: relative.to_owned(),
                source,
            }),
        }
    }

    fn copy(&self, relative: &str, from: &Path, to: &Path) -> Result<(), OpError> {
        let fail = |source| OpError {
            path: relative.to_owned(),
            source,
        };
        match self.fs.kind(from).map_err(fail)? {
            None => Err(fail(io::Error::new(io::ErrorKind::NotFound, "not found"))),
            Some(false) => {
                let data = self.fs.read(from).map_err(fail)?;
                self.fs.write_atomic(to, &data).map_err(fail)
            }
            Some(true) => {
                self.fs.create_dir_all(to).map_err(fail)?;
                for entry in self.fs.read_dir(from).map_err(fail)? {
                    let child = join(relative, &entry.name);
                    self.copy(&child, &from.join(&entry.name), &to.join(&entry.name))?;
                }
                Ok(())
            }
        }
    }

    /// The absolute path for a workspace-relative one, refusing anything
    /// that could leave the workspace.
    fn resolve(&self, relative: &str) -> Result<PathBuf, OpError> {
        let path = Path::new(relative);
        let safe =
            !relative.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)));
        if safe {
            Ok(self.root.join(path))
        } else {
            Err(OpError {
                path: relative.to_owned(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path is outside the workspace",
                ),
            })
        }
    }
}
