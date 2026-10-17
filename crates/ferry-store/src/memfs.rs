//! An in-memory [`Fs`] for tests, with injectable failures.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::fs::{DirEntry, Fs};

#[derive(Debug, Default)]
struct State {
    /// Files and their contents.
    files: BTreeMap<PathBuf, Vec<u8>>,
    /// Directories, including every ancestor of a file.
    dirs: std::collections::BTreeSet<PathBuf>,
    /// Paths (and everything under them) whose writes, renames, removals and
    /// directory creation fail with this error kind.
    failing: Vec<(PathBuf, io::ErrorKind)>,
    /// Paths whose reads fail with this error kind.
    failing_reads: Vec<(PathBuf, io::ErrorKind)>,
    /// Number of `write_atomic` calls, for asserting that nothing was written.
    writes: usize,
}

// CONTRACT
/// A filesystem held in memory. Clones share the same files.
#[derive(Debug, Clone, Default)]
pub struct MemFs {
    state: Arc<Mutex<State>>,
}

/// `path` without `.` components, so equal paths compare equal.
fn normal(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

impl MemFs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Add a file, creating its parent directories.
    pub fn put(&self, path: impl AsRef<Path>, data: impl AsRef<[u8]>) {
        let path = normal(path.as_ref());
        let mut state = self.state();
        add_parents(&mut state, &path);
        state.files.insert(path, data.as_ref().to_vec());
    }

    /// A file's contents as text, if it exists.
    #[must_use]
    pub fn text(&self, path: impl AsRef<Path>) -> Option<String> {
        self.state()
            .files
            .get(&normal(path.as_ref()))
            .map(|d| String::from_utf8_lossy(d).into_owned())
    }

    /// Every file path, sorted.
    #[must_use]
    pub fn files(&self) -> Vec<PathBuf> {
        self.state().files.keys().cloned().collect()
    }

    /// Make changes at or under `path` fail with `kind`.
    pub fn fail(&self, path: impl AsRef<Path>, kind: io::ErrorKind) {
        let path = normal(path.as_ref());
        self.state().failing.push((path, kind));
    }

    /// Make reads of `path` (and anything under it) fail with `kind`.
    pub fn fail_reads(&self, path: impl AsRef<Path>, kind: io::ErrorKind) {
        let path = normal(path.as_ref());
        self.state().failing_reads.push((path, kind));
    }

    /// How many atomic writes have happened.
    #[must_use]
    pub fn writes(&self) -> usize {
        self.state().writes
    }

    fn check(state: &State, path: &Path) -> io::Result<()> {
        match state.failing.iter().find(|(p, _)| path.starts_with(p)) {
            Some((_, kind)) => Err(io::Error::new(*kind, "injected failure")),
            None => Ok(()),
        }
    }
}

fn add_parents(state: &mut State, path: &Path) {
    let mut parent = path.parent();
    while let Some(p) = parent {
        if p.as_os_str().is_empty() {
            break;
        }
        state.dirs.insert(p.to_path_buf());
        parent = p.parent();
    }
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} not found", path.display()),
    )
}

impl Fs for MemFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normal(path);
        let state = self.state();
        if let Some((_, kind)) = state
            .failing_reads
            .iter()
            .find(|(p, _)| path.starts_with(p))
        {
            return Err(io::Error::new(*kind, "injected read failure"));
        }
        match state.files.get(&path) {
            Some(data) => Ok(data.clone()),
            None if state.dirs.contains(&path) => Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                "is a directory",
            )),
            None => Err(not_found(&path)),
        }
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let path = normal(path);
        let mut state = self.state();
        Self::check(&state, &path)?;
        if state.dirs.contains(&path) {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                "is a directory",
            ));
        }
        state.writes += 1;
        add_parents(&mut state, &path);
        state.files.insert(path, data.to_vec());
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (normal(from), normal(to));
        let mut state = self.state();
        Self::check(&state, &from)?;
        Self::check(&state, &to)?;
        if let Some(data) = state.files.remove(&from) {
            add_parents(&mut state, &to);
            state.files.insert(to, data);
            return Ok(());
        }
        if !state.dirs.contains(&from) {
            return Err(not_found(&from));
        }
        if to.starts_with(&from) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot move a directory into itself",
            ));
        }
        let moved_files: Vec<PathBuf> = state
            .files
            .keys()
            .filter(|p| p.starts_with(&from))
            .cloned()
            .collect();
        for old in moved_files {
            if let (Some(data), Ok(rest)) = (state.files.remove(&old), old.strip_prefix(&from)) {
                state.files.insert(to.join(rest), data);
            }
        }
        let moved_dirs: Vec<PathBuf> = state
            .dirs
            .iter()
            .filter(|p| p.starts_with(&from))
            .cloned()
            .collect();
        for old in moved_dirs {
            state.dirs.remove(&old);
            if let Ok(rest) = old.strip_prefix(&from) {
                state.dirs.insert(to.join(rest));
            }
        }
        add_parents(&mut state, &to.join("x"));
        Ok(())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let path = normal(path);
        let state = self.state();
        if !state.dirs.contains(&path) {
            return Err(not_found(&path));
        }
        let child = |p: &PathBuf| -> Option<String> {
            (p.parent() == Some(path.as_path()))
                .then(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .flatten()
        };
        let mut entries: Vec<DirEntry> = state
            .dirs
            .iter()
            .filter_map(|p| child(p).map(|name| DirEntry { name, is_dir: true }))
            .chain(state.files.keys().filter_map(|p| {
                child(p).map(|name| DirEntry {
                    name,
                    is_dir: false,
                })
            }))
            .collect();
        entries.sort();
        Ok(entries)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let path = normal(path);
        let mut state = self.state();
        Self::check(&state, &path)?;
        if state.files.contains_key(&path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a file is in the way",
            ));
        }
        add_parents(&mut state, &path.join("x"));
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let path = normal(path);
        let mut state = self.state();
        Self::check(&state, &path)?;
        state
            .files
            .remove(&path)
            .map(|_| ())
            .ok_or_else(|| not_found(&path))
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let path = normal(path);
        let mut state = self.state();
        Self::check(&state, &path)?;
        if !state.dirs.contains(&path) {
            return Err(not_found(&path));
        }
        state.files.retain(|p, _| !p.starts_with(&path));
        state.dirs.retain(|p| !p.starts_with(&path));
        Ok(())
    }

    fn kind(&self, path: &Path) -> io::Result<Option<bool>> {
        let path = normal(path);
        let state = self.state();
        Ok(if state.dirs.contains(&path) {
            Some(true)
        } else if state.files.contains_key(&path) {
            Some(false)
        } else {
            None
        })
    }
}
