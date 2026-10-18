//! Filesystem persistence for Ferry: atomic writes, the saved session
//! (F07-AC5), and the workspace of collections and environments (F08, F09).
//!
//! The filesystem sits behind the [`Fs`] trait so error cases are testable;
//! [`MemFs`] is an in-memory implementation for tests.

mod data;
mod fs;
mod memfs;
mod session;
pub mod watch;
mod workspace;

pub use data::{
    BODIES_DIR, COOKIES_FILE, CookieStore, HISTORY_DIR, HISTORY_FILE, HistoryStore, Restored,
};
pub use fs::{DirEntry, Fs, RealFs};
pub use memfs::MemFs;
pub use session::{
    Loaded, Platform, SESSION_FILE, SessionStore, StoreError, data_dir, data_dir_on,
};
pub use workspace::{OpError, WorkspaceStore};
