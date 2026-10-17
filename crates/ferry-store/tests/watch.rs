//! F09-AC10: changes made by other programs are noticed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use ferry_store::watch::{Watcher, is_relevant};

#[test]
fn f09_ac10_relevant_paths() {
    let root = Path::new("/ws");
    assert!(is_relevant(root, Path::new("/ws/collections/a/r.toml")));
    assert!(is_relevant(
        root,
        Path::new("/ws/environments/dev.local.toml")
    ));
    assert!(is_relevant(root, Path::new("/ws/collections")));
    assert!(!is_relevant(root, Path::new("/ws/.git/index")));
    assert!(!is_relevant(
        root,
        Path::new("/ws/collections/.hidden/x.toml")
    ));
    assert!(!is_relevant(
        root,
        Path::new("/ws/collections/a/r.toml.tmp")
    ));
    assert!(!is_relevant(root, Path::new("/ws/collections/a/r.TMP")));
    assert!(!is_relevant(root, Path::new("/elsewhere/r.toml")));
}

#[test]
fn f09_ac10_external_writes_are_reported() {
    let dir = std::env::temp_dir().join(format!("ferry-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("collections")).unwrap();
    // Canonical, so event paths (which notify reports resolved) match.
    let dir = dir.canonicalize().unwrap();
    let (tx, rx) = mpsc::channel();
    let watcher = Watcher::start(&dir, move || {
        let _ = tx.send(());
    })
    .unwrap();
    std::fs::write(dir.join("collections/new.toml"), "name = \"n\"\n").unwrap();
    rx.recv_timeout(Duration::from_secs(5))
        .expect("a change is reported");
    drop(watcher);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn f09_ac10_watching_a_missing_folder_fails() {
    let missing = std::env::temp_dir().join("ferry-watch-missing-folder-does-not-exist");
    let _ = std::fs::remove_dir_all(&missing);
    assert!(Watcher::start(&missing, || {}).is_err());
}
