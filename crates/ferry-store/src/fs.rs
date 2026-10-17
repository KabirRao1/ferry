//! The filesystem operations persistence needs.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;

// CONTRACT
/// An entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirEntry {
    /// The entry's name. Entries whose names are not valid UTF-8 are left out.
    pub name: String,
    pub is_dir: bool,
}

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

    /// Rename `from` to `to`, replacing a file at `to`.
    ///
    /// # Errors
    /// Any I/O error.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// The entries of a directory, sorted by name.
    ///
    /// # Errors
    /// Any I/O error; `NotFound` if it does not exist.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>>;

    /// Create a directory and its missing parents.
    ///
    /// # Errors
    /// Any I/O error.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Delete a file.
    ///
    /// # Errors
    /// Any I/O error; `NotFound` if it does not exist.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Delete a directory and everything in it.
    ///
    /// # Errors
    /// Any I/O error; `NotFound` if it does not exist.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;

    /// `Some(true)` for a directory, `Some(false)` for a file, `None` if
    /// nothing is there.
    ///
    /// # Errors
    /// Any I/O error other than not found.
    fn kind(&self, path: &Path) -> io::Result<Option<bool>>;
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

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            entries.push(DirEntry {
                name,
                is_dir: entry.file_type()?.is_dir(),
            });
        }
        entries.sort();
        Ok(entries)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn kind(&self, path: &Path) -> io::Result<Option<bool>> {
        match fs::metadata(path) {
            Ok(meta) => Ok(Some(meta.is_dir())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}
