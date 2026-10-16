//! F07-AC5: saving and restoring the session.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ferry_core::draft::RequestDraft;
use ferry_core::session::Session;
use ferry_store::{
    Fs, Loaded, Platform, RealFs, SESSION_FILE, SessionStore, data_dir, data_dir_on,
};

/// An in-memory filesystem with injectable failures.
#[derive(Clone, Default)]
struct MemFs {
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    fail_reads: Arc<Mutex<Option<io::ErrorKind>>>,
    fail_writes: Arc<Mutex<bool>>,
    fail_renames: Arc<Mutex<bool>>,
}

impl MemFs {
    fn put(&self, path: &Path, data: &[u8]) {
        self.files.lock().unwrap().insert(path.into(), data.into());
    }
    fn get(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(path).cloned()
    }
}

impl Fs for MemFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if let Some(kind) = *self.fail_reads.lock().unwrap() {
            return Err(io::Error::new(kind, "injected read failure"));
        }
        self.get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))
    }
    fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if *self.fail_writes.lock().unwrap() {
            return Err(io::Error::other("disk full"));
        }
        self.put(path, data);
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if *self.fail_renames.lock().unwrap() {
            return Err(io::Error::other("injected rename failure"));
        }
        let data = self
            .files
            .lock()
            .unwrap()
            .remove(from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not found"))?;
        self.put(to, &data);
        Ok(())
    }
}

fn dir() -> PathBuf {
    PathBuf::from("/data/ferry")
}

fn sample() -> Session {
    Session {
        tabs: vec![
            RequestDraft {
                url: "https://a.test/one".into(),
                ..RequestDraft::default()
            },
            RequestDraft::default(),
        ],
        active: 1,
        sidebar_open: false,
        split: 0.6,
        ..Session::default()
    }
}

#[test]
fn f07_ac5_first_start_has_no_session() {
    let store = SessionStore::new(MemFs::default(), &dir());
    assert_eq!(store.load(), Loaded::Missing);
    assert_eq!(store.path(), dir().join(SESSION_FILE));
}

#[test]
fn f07_ac5_save_then_load_restores() {
    let fs = MemFs::default();
    let store = SessionStore::new(fs.clone(), &dir());
    store.save(&sample()).unwrap();
    assert_eq!(store.load(), Loaded::Restored(sample()));
    let text = String::from_utf8(fs.get(&dir().join("session.toml")).unwrap()).unwrap();
    assert!(text.contains("version = 1"), "{text}");
}

#[test]
fn f07_ac5_unparsable_session_is_backed_up() {
    let fs = MemFs::default();
    let path = dir().join(SESSION_FILE);
    fs.put(&path, b"tabs = [ this is not toml");
    let store = SessionStore::new(fs.clone(), &dir());
    match store.load() {
        Loaded::Unreadable { reason, backup } => {
            assert!(!reason.is_empty());
            assert_eq!(backup, Some(dir().join("session.toml.bak")));
        }
        other => panic!("expected unreadable, got {other:?}"),
    }
    assert_eq!(fs.get(&path), None);
    assert_eq!(
        fs.get(&dir().join("session.toml.bak")).unwrap(),
        b"tabs = [ this is not toml"
    );
    // The next start is a fresh one.
    assert_eq!(store.load(), Loaded::Missing);
}

#[test]
fn f07_ac5_invalid_utf8_newer_version_and_bad_method_are_unreadable() {
    for (contents, needle) in [
        (&b"\xff\xfe"[..], "utf-8"),
        (&b"version = 99\n"[..], "newer"),
        (
            &b"[[tabs]]\nmethod = \"BAD METHOD\"\n"[..],
            "invalid character",
        ),
    ] {
        let fs = MemFs::default();
        fs.put(&dir().join(SESSION_FILE), contents);
        match SessionStore::new(fs, &dir()).load() {
            Loaded::Unreadable { reason, .. } => {
                assert!(reason.to_lowercase().contains(needle), "{reason}");
            }
            other => panic!("expected unreadable, got {other:?}"),
        }
    }
}

#[test]
fn f07_ac5_read_error_is_unreadable_and_failed_backup_is_reported() {
    let fs = MemFs::default();
    *fs.fail_reads.lock().unwrap() = Some(io::ErrorKind::PermissionDenied);
    *fs.fail_renames.lock().unwrap() = true;
    match SessionStore::new(fs, &dir()).load() {
        Loaded::Unreadable { reason, backup } => {
            assert!(reason.contains("injected read failure"));
            assert_eq!(backup, None);
        }
        other => panic!("expected unreadable, got {other:?}"),
    }
}

#[test]
fn f07_ac5_failed_save_reports_path_and_keeps_old_file() {
    let fs = MemFs::default();
    let store = SessionStore::new(fs.clone(), &dir());
    store.save(&sample()).unwrap();
    *fs.fail_writes.lock().unwrap() = true;
    let e = store.save(&Session::default()).unwrap_err();
    assert_eq!(
        e.to_string(),
        format!(
            "Could not save {}: disk full",
            dir().join("session.toml").display()
        )
    );
    assert!(std::error::Error::source(&e).is_some());
    assert_eq!(store.load(), Loaded::Restored(sample()));
}

#[test]
fn f07_ac5_real_fs_atomic_write_replaces_and_leaves_no_temp_file() {
    let root = std::env::temp_dir().join(format!("ferry-store-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let nested = root.join("a/b");
    let store = SessionStore::new(RealFs, &nested);
    assert_eq!(store.load(), Loaded::Missing);
    store.save(&Session::default()).unwrap();
    store.save(&sample()).unwrap();
    assert_eq!(store.load(), Loaded::Restored(sample()));
    let names: Vec<String> = std::fs::read_dir(&nested)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["session.toml"]);
    // Writing into a path whose parent is a file fails cleanly.
    let blocked = SessionStore::new(RealFs, &nested.join("session.toml"));
    assert!(blocked.save(&sample()).is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name: &str| {
        vars.iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

#[test]
fn f07_ac5_data_dir_follows_platform_conventions() {
    let win = |vars| data_dir_on(Platform::Windows, env(vars));
    assert_eq!(
        win(&[("APPDATA", r"C:\Users\u\AppData\Roaming")]),
        Some(PathBuf::from(r"C:\Users\u\AppData\Roaming").join("Ferry"))
    );
    assert_eq!(win(&[("APPDATA", "")]), None);
    assert_eq!(win(&[("HOME", "/home/u")]), None);

    let mac = |vars| data_dir_on(Platform::MacOs, env(vars));
    assert_eq!(
        mac(&[("HOME", "/Users/u")]),
        Some(PathBuf::from("/Users/u").join("Library/Application Support/Ferry"))
    );
    assert_eq!(mac(&[]), None);

    let xdg = |vars| data_dir_on(Platform::Xdg, env(vars));
    assert_eq!(
        xdg(&[("HOME", "/home/u")]),
        Some(PathBuf::from("/home/u").join(".local/share").join("ferry"))
    );
    assert_eq!(
        xdg(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "/xdg")]),
        Some(PathBuf::from("/xdg").join("ferry"))
    );
    assert_eq!(
        xdg(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "")]),
        Some(PathBuf::from("/home/u").join(".local/share").join("ferry"))
    );
    assert_eq!(xdg(&[]), None);
}

#[test]
fn f07_ac5_data_dir_uses_this_platform() {
    let current = Platform::current();
    let vars: &'static [(&'static str, &'static str)] =
        &[("APPDATA", "a"), ("HOME", "h"), ("XDG_DATA_HOME", "x")];
    assert_eq!(data_dir(env(vars)), data_dir_on(current, env(vars)));
    if cfg!(windows) {
        assert_eq!(current, Platform::Windows);
    } else if cfg!(target_os = "macos") {
        assert_eq!(current, Platform::MacOs);
    } else {
        assert_eq!(current, Platform::Xdg);
    }
}

#[test]
fn f07_ac5_real_fs_failed_write_leaves_target_and_no_temp_file() {
    let root = std::env::temp_dir().join(format!("ferry-store-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // A directory where the session file should be: the final rename fails.
    std::fs::create_dir_all(root.join(SESSION_FILE).join("occupied")).unwrap();
    let store = SessionStore::new(RealFs, &root);
    assert!(store.save(&sample()).is_err());
    let names: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [SESSION_FILE], "temporary file left behind");
    assert!(root.join(SESSION_FILE).join("occupied").is_dir());
    // Reading a directory is an I/O error, reported as unreadable.
    assert!(matches!(store.load(), Loaded::Unreadable { .. }));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f07_ac5_real_fs_moves_an_unreadable_session_aside() {
    let root = std::env::temp_dir().join(format!("ferry-store-bak-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(SESSION_FILE), "not = [valid").unwrap();
    let store = SessionStore::new(RealFs, &root);
    let backup = root.join("session.toml.bak");
    assert!(matches!(
        store.load(),
        Loaded::Unreadable { backup: Some(ref b), .. } if *b == backup
    ));
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), "not = [valid");
    assert!(!root.join(SESSION_FILE).exists());
    std::fs::remove_dir_all(&root).unwrap();
}
