//! History and cookie files: F10-AC6, F17-AC6.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::io;
use std::path::PathBuf;

use ferry_core::cookies::Jar;
use ferry_core::draft::RequestDraft;
use ferry_core::history::{Entry, History, Outcome};
use ferry_store::{CookieStore, HistoryStore, MemFs, Restored};

/// 2026-10-17T19:07:20Z.
const NOW: i64 = 1_792_264_040_000;

fn data() -> PathBuf {
    PathBuf::from("/data")
}

fn history() -> History {
    let mut h = History::default();
    h.record(Entry {
        id: 0,
        at: NOW,
        method: "GET".into(),
        url: "https://a.test".into(),
        request: RequestDraft::default(),
        outcome: Outcome::Failed {
            error: "boom".into(),
        },
    });
    h
}

#[test]
fn f10_ac6_history_round_trips() {
    let fs = MemFs::new();
    let store = HistoryStore::new(fs.clone(), &data());
    assert_eq!(store.load(), Restored::Missing);
    store.save(&history()).unwrap();
    assert!(fs.text("/data/history/history.toml").is_some());
    assert_eq!(store.load(), Restored::Ready(history()));
}

#[test]
fn f10_ac6_bodies_are_separate_files() {
    let fs = MemFs::new();
    let store = HistoryStore::new(fs.clone(), &data());
    store.write_body(7, b"{\"ok\":true}").unwrap();
    store.write_body(8, &[0, 255]).unwrap();
    assert_eq!(store.read_body(7).as_deref(), Some(&b"{\"ok\":true}"[..]));
    assert_eq!(store.read_body(8).as_deref(), Some(&[0u8, 255][..]));
    assert_eq!(store.read_body(9), None);
    store.remove_bodies(&[7, 9]);
    assert_eq!(store.read_body(7), None);
    assert!(store.read_body(8).is_some());
    assert!(
        fs.files()
            .contains(&PathBuf::from("/data/history/bodies/8"))
    );
}

#[test]
fn f10_ac6_unreadable_history_is_moved_aside() {
    let fs = MemFs::new();
    fs.put("/data/history/history.toml", "entries = 1");
    let store = HistoryStore::new(fs.clone(), &data());
    match store.load() {
        Restored::Unreadable { reason, backup } => {
            assert!(reason.starts_with("line 1"), "{reason}");
            assert_eq!(
                backup,
                Some(PathBuf::from("/data/history/history.toml.bak"))
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs.text("/data/history/history.toml.bak").as_deref(),
        Some("entries = 1")
    );
    assert_eq!(store.load(), Restored::Missing);
    // Not UTF-8, and unreadable files, are moved aside too.
    fs.put("/data/history/history.toml", [0xff]);
    assert!(matches!(store.load(), Restored::Unreadable { .. }));
    fs.put("/data/history/history.toml", "");
    fs.fail_reads(
        "/data/history/history.toml",
        io::ErrorKind::PermissionDenied,
    );
    match store.load() {
        Restored::Unreadable { reason, .. } => assert!(reason.contains("injected")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f10_ac6_failed_saves_report_the_path() {
    let fs = MemFs::new();
    fs.fail("/data/history", io::ErrorKind::PermissionDenied);
    let store = HistoryStore::new(fs, &data());
    let error = store.save(&history()).unwrap_err();
    assert!(error.to_string().contains("history.toml"), "{error}");
    assert!(store.write_body(1, b"x").is_err());
}

#[test]
fn f17_ac6_cookies_round_trip_without_expired_ones() {
    let fs = MemFs::new();
    let store = CookieStore::new(fs.clone(), &data());
    assert_eq!(store.load(NOW), Restored::Missing);
    let mut jar = Jar::default();
    jar.store("keep=1", "h.test", "/", NOW).unwrap();
    jar.store("brief=1; Max-Age=1", "h.test", "/", NOW).unwrap();
    store.save(&jar).unwrap();
    assert!(fs.text("/data/cookies.toml").is_some());
    assert_eq!(store.load(NOW), Restored::Ready(jar.clone()));
    let later = store.load(NOW + 5_000).or_default();
    assert_eq!(later.cookies.len(), 1);
}

#[test]
fn f17_ac6_unreadable_cookies_are_moved_aside() {
    let fs = MemFs::new();
    fs.put("/data/cookies.toml", "cookies = \"nope\"");
    let store = CookieStore::new(fs.clone(), &data());
    assert!(matches!(store.load(NOW), Restored::Unreadable { .. }));
    assert!(fs.text("/data/cookies.toml.bak").is_some());
    assert_eq!(store.load(NOW).or_default(), Jar::default());
    let fs = MemFs::new();
    fs.fail("/data", io::ErrorKind::PermissionDenied);
    assert!(CookieStore::new(fs, &data()).save(&Jar::default()).is_err());
}

#[test]
fn f10_ac6_restored_or_default() {
    assert_eq!(Restored::Ready(5).or_default(), 5);
    assert_eq!(Restored::<u8>::Missing.or_default(), 0);
    assert_eq!(
        Restored::<u8>::Unreadable {
            reason: String::new(),
            backup: None
        }
        .or_default(),
        0
    );
}
