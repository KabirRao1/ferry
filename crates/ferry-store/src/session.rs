//! The saved workbench session (F07-AC5).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use ferry_core::session::Session;

use crate::fs::Fs;

/// File name of the session inside the data directory.
pub const SESSION_FILE: &str = "session.toml";

// CONTRACT
/// Platform conventions for the data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    /// Linux and other Unix-likes (XDG).
    Xdg,
}

impl Platform {
    // CONTRACT
    /// The platform this build runs on.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Xdg
        }
    }
}

// CONTRACT
/// Ferry's per-user data directory on this platform (see [`data_dir_on`]).
#[must_use]
pub fn data_dir(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    data_dir_on(Platform::current(), env)
}

// CONTRACT
/// Ferry's per-user data directory: `%APPDATA%\Ferry` on Windows,
/// `~/Library/Application Support/Ferry` on macOS, and
/// `$XDG_DATA_HOME/ferry` or `~/.local/share/ferry` elsewhere. `env` looks
/// up environment variables; `None` if the needed ones are unset or empty.
#[must_use]
pub fn data_dir_on(platform: Platform, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    match platform {
        Platform::Windows => non_empty("APPDATA").map(|d| d.join("Ferry")),
        Platform::MacOs => non_empty("HOME").map(|h| h.join("Library/Application Support/Ferry")),
        Platform::Xdg => non_empty("XDG_DATA_HOME")
            .or_else(|| non_empty("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("ferry")),
    }
}

// CONTRACT
/// The result of loading the session.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded {
    /// No session yet: first start.
    Missing,
    Restored(Session),
    /// The file could not be used. It was moved to `backup` (if the move
    /// worked) so the next save does not destroy it.
    Unreadable {
        reason: String,
        backup: Option<PathBuf>,
    },
}

// CONTRACT
/// Why a session could not be saved.
#[derive(Debug)]
pub struct StoreError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Could not save {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

// CONTRACT
/// Loads and saves `session.toml` in a directory.
pub struct SessionStore<F: Fs> {
    fs: F,
    path: PathBuf,
}

impl<F: Fs> SessionStore<F> {
    // CONTRACT
    /// A store for `dir/session.toml`.
    pub fn new(fs: F, dir: &Path) -> Self {
        Self {
            fs,
            path: dir.join(SESSION_FILE),
        }
    }

    // CONTRACT
    /// Where the session lives.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    // CONTRACT
    /// Load the session. A file that cannot be read or parsed, or that has a
    /// newer format version, is renamed to `session.toml.bak`.
    pub fn load(&self) -> Loaded {
        let bytes = match self.fs.read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::Missing,
            Err(e) => return self.unreadable(e.to_string()),
        };
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => return self.unreadable(e.to_string()),
        };
        match toml::from_str::<Session>(&text) {
            Ok(session) if session.version > Session::VERSION => self.unreadable(format!(
                "session format {} is newer than this version of Ferry supports ({})",
                session.version,
                Session::VERSION
            )),
            Ok(session) => Loaded::Restored(session),
            Err(e) => self.unreadable(e.message().to_owned()),
        }
    }

    fn unreadable(&self, reason: String) -> Loaded {
        let mut backup_name = self.path.file_name().unwrap_or_default().to_owned();
        backup_name.push(".bak");
        let backup = self.path.with_file_name(backup_name);
        let backup = self.fs.rename(&self.path, &backup).ok().map(|()| backup);
        Loaded::Unreadable { reason, backup }
    }

    // CONTRACT
    /// Save `session` atomically.
    ///
    /// # Errors
    /// [`StoreError`] if it cannot be written; the previous file is intact.
    pub fn save(&self, session: &Session) -> Result<(), StoreError> {
        let error = |source| StoreError {
            path: self.path.clone(),
            source,
        };
        let text = toml::to_string(session)
            .map_err(|e| error(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        self.fs
            .write_atomic(&self.path, text.as_bytes())
            .map_err(error)
    }
}
