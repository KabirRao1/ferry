//! Filesystem persistence for Ferry: atomic writes and the saved session
//! (F07-AC5). Collections and file watching arrive with F09.
//!
//! The filesystem sits behind the [`Fs`] trait so error cases are testable.

mod fs;
mod session;

pub use fs::{Fs, RealFs};
pub use session::{
    Loaded, Platform, SESSION_FILE, SessionStore, StoreError, data_dir, data_dir_on,
};
