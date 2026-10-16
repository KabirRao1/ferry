//! The filesystem operations persistence needs.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

// CONTRACT
/// Filesystem access, injectable for tests.
pub trait Fs: Send + Sync + 'static {
    /// The whole file. `io::ErrorKind::NotFound` if it does not exist.
    ///
    /// # Errors
    /// Any I/O error.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Replace `path` with `data` so that readers see either the old or the
    /// new contents, never a mix: write a sibling temporary file, flush it
    /// to disk, then rename it over `path`. Creates missing parent
    /// directories.
    ///
    /// # Errors
    /// Any I/O error; `path` is then unchanged.
    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()>;

    /// Rename `from` to `to`, replacing `to`.
    ///
    /// # Errors
    /// Any I/O error.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

// CONTRACT
/// The real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl Fs for RealFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut tmp_name = path.file_name().unwrap_or_default().to_owned();
        tmp_name.push(".tmp");
        let tmp = path.with_file_name(tmp_name);
        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(data)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}
