//! History and cookies in the data directory (F10-AC6, F17-AC6,
//! ADR-0012, ADR-0039). Unreadable files are moved aside to `.bak`, as the
//! session is.

use std::io;
use std::path::{Path, PathBuf};

use ferry_core::cookies::{self, Jar};
use ferry_core::history::{self, History};

use crate::fs::Fs;
use crate::session::StoreError;

/// History directory inside the data directory.
pub const HISTORY_DIR: &str = "history";
/// History file inside [`HISTORY_DIR`].
pub const HISTORY_FILE: &str = "history.toml";
/// Body directory inside [`HISTORY_DIR`].
pub const BODIES_DIR: &str = "bodies";
/// Cookie jar file inside the data directory.
pub const COOKIES_FILE: &str = "cookies.toml";

// CONTRACT
/// The result of loading a data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restored<T> {
    /// No file yet.
    Missing,
    Ready(T),
    /// The file could not be used; it was moved to `backup` if possible.
    Unreadable {
        reason: String,
        backup: Option<PathBuf>,
    },
}

impl<T: Default> Restored<T> {
    // CONTRACT
    /// The loaded value, or the default when missing or unreadable.
    #[must_use]
    pub fn or_default(self) -> T {
        match self {
            Self::Ready(value) => value,
            Self::Missing | Self::Unreadable { .. } => T::default(),
        }
    }
}

/// Read and parse `path`, moving it to `<path>.bak` if it cannot be used.
fn restore<F: Fs, T>(
    fs: &F,
    path: &Path,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Restored<T> {
    let unreadable = |reason: String| {
        let mut name = path.file_name().unwrap_or_default().to_owned();
        name.push(".bak");
        let backup = path.with_file_name(name);
        let backup = fs.rename(path, &backup).ok().map(|()| backup);
        Restored::Unreadable { reason, backup }
    };
    let bytes = match fs.read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Restored::Missing,
        Err(e) => return unreadable(e.to_string()),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return unreadable("the file is not valid UTF-8".to_owned());
    };
    match parse(&text) {
        Ok(value) => Restored::Ready(value),
        Err(reason) => unreadable(reason),
    }
}

fn save_error(path: &Path, source: io::Error) -> StoreError {
    StoreError {
        path: path.to_path_buf(),
        source,
    }
}

// CONTRACT
/// The history and its bodies.
pub struct HistoryStore<F: Fs> {
    fs: F,
    dir: PathBuf,
}

impl<F: Fs> HistoryStore<F> {
    // CONTRACT
    /// History in `data/history`.
    pub fn new(fs: F, data: &Path) -> Self {
        Self {
            fs,
            dir: data.join(HISTORY_DIR),
        }
    }

    // CONTRACT
    #[must_use]
    pub fn load(&self) -> Restored<History> {
        restore(&self.fs, &self.dir.join(HISTORY_FILE), |text| {
            history::parse(text).map_err(|e| e.to_string())
        })
    }

    fn body_path(&self, id: u64) -> PathBuf {
        self.dir.join(BODIES_DIR).join(id.to_string())
    }

    // CONTRACT
    /// Write `history.toml` atomically.
    ///
    /// # Errors
    /// [`StoreError`] if it cannot be written.
    pub fn save(&self, history: &History) -> Result<(), StoreError> {
        let path = self.dir.join(HISTORY_FILE);
        self.fs
            .write_atomic(&path, history::write(history).as_bytes())
            .map_err(|e| save_error(&path, e))
    }

    // CONTRACT
    /// Store an entry's body.
    ///
    /// # Errors
    /// [`StoreError`] if it cannot be written.
    pub fn write_body(&self, id: u64, body: &[u8]) -> Result<(), StoreError> {
        let path = self.body_path(id);
        self.fs
            .write_atomic(&path, body)
            .map_err(|e| save_error(&path, e))
    }

    // CONTRACT
    /// An entry's body; `None` if there is none or it cannot be read.
    #[must_use]
    pub fn read_body(&self, id: u64) -> Option<Vec<u8>> {
        self.fs.read(&self.body_path(id)).ok()
    }

    // CONTRACT
    /// Delete the bodies of dropped entries; missing ones are fine.
    pub fn remove_bodies(&self, ids: &[u64]) {
        for id in ids {
            // Missing bodies are fine: the entry may have failed.
            let _ = self.fs.remove_file(&self.body_path(*id));
        }
    }
}

// CONTRACT
/// The cookie jar file.
pub struct CookieStore<F: Fs> {
    fs: F,
    path: PathBuf,
}

impl<F: Fs> CookieStore<F> {
    // CONTRACT
    /// The jar in `data/cookies.toml`.
    pub fn new(fs: F, data: &Path) -> Self {
        Self {
            fs,
            path: data.join(COOKIES_FILE),
        }
    }

    // CONTRACT
    /// Load the jar without cookies expired at `now`.
    #[must_use]
    pub fn load(&self, now: i64) -> Restored<Jar> {
        restore(&self.fs, &self.path, |text| {
            cookies::parse(text, now).map_err(|e| e.to_string())
        })
    }

    // CONTRACT
    /// Write the jar atomically.
    ///
    /// # Errors
    /// [`StoreError`] if it cannot be written.
    pub fn save(&self, jar: &Jar) -> Result<(), StoreError> {
        self.fs
            .write_atomic(&self.path, cookies::write(jar).as_bytes())
            .map_err(|e| save_error(&self.path, e))
    }
}
