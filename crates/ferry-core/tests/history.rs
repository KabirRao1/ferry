//! History: F10-AC1..AC3, F10-AC6 format.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::draft::RequestDraft;
use ferry_core::history::{self, Entry, History, MAX_ENTRIES, Outcome, groups};
use ferry_core::response::Header;
use proptest::prelude::*;

const DAY: i64 = 86_400_000;
/// 2026-10-17T19:07:20Z.
const NOW: i64 = 1_792_264_040_000;

fn entry(method: &str, url: &str, at: i64) -> Entry {
    Entry {
        id: 0,
        at,
        method: method.into(),
        url: url.into(),
        request: RequestDraft {
            url: url.into(),
            ..RequestDraft::default()
        },
        outcome: Outcome::Response {
            status: 200,
            reason: "OK".into(),
            elapsed_ms: 12,
            size: 5,
            headers: vec![],
            truncated: false,
        },
    }
}

#[test]
fn f10_ac1_record_assigns_ids_newest_first() {
    let mut h = History::default();
    let (a, dropped) = h.record(entry("GET", "https://a.test", NOW));
    assert!(dropped.is_empty());
    let (b, _) = h.record(entry("POST", "https://b.test", NOW + 1));
    assert_eq!((a, b), (0, 1));
    assert_eq!(h.next_id, 2);
    let urls: Vec<&str> = h.entries.iter().map(|e| e.url.as_str()).collect();
    assert_eq!(urls, ["https://b.test", "https://a.test"]);
    assert_eq!(h.get(a).unwrap().url, "https://a.test");
    assert!(h.get(9).is_none());
}

#[test]
fn f10_ac2_only_the_newest_100_are_kept() {
    let mut h = History::default();
    for i in 0..MAX_ENTRIES {
        let (_, dropped) = h.record(entry("GET", &format!("https://x/{i}"), NOW));
        assert!(dropped.is_empty());
    }
    let (id, dropped) = h.record(entry("GET", "https://x/new", NOW));
    assert_eq!(id, 100);
    assert_eq!(dropped, [0]);
    assert_eq!(h.entries.len(), MAX_ENTRIES);
    assert_eq!(h.entries.last().unwrap().id, 1);
}

#[test]
fn f10_ac2_ids_keep_growing_after_removals() {
    let mut h = History::default();
    h.record(entry("GET", "a", NOW));
    let (b, _) = h.record(entry("GET", "b", NOW));
    assert!(h.remove(b));
    assert!(!h.remove(b));
    let (c, _) = h.record(entry("GET", "c", NOW));
    assert_eq!(c, 2, "ids are never reused, so bodies never mix");
    assert_eq!(h.clear(), [2, 0]);
    assert!(h.entries.is_empty());
    assert_eq!(h.next_id, 3);
}

#[test]
fn f10_ac3_search_by_url_or_method() {
    let mut h = History::default();
    h.record(entry("GET", "https://api.test/Users", NOW));
    h.record(entry("POST", "https://api.test/orders", NOW));
    h.record(entry("DELETE", "https://other.test/x", NOW));
    let urls = |q: &str| -> Vec<String> { h.search(q).iter().map(|e| e.url.clone()).collect() };
    assert_eq!(urls("users"), ["https://api.test/Users"]);
    assert_eq!(urls("post"), ["https://api.test/orders"]);
    assert_eq!(urls("API.TEST").len(), 2);
    assert_eq!(urls("").len(), 3);
    assert!(urls("nothing").is_empty());
}

#[test]
fn f10_ac3_groups_by_local_day() {
    let at = [
        NOW,                  // today
        NOW - 19 * 3_600_000, // today, 00:07
        NOW - 20 * 3_600_000, // yesterday 23:07
        NOW - 2 * DAY,        // October 15
        NOW - 365 * DAY,      // 2025
    ];
    let entries: Vec<Entry> = at.iter().map(|t| entry("GET", "u", *t)).collect();
    let refs: Vec<&Entry> = entries.iter().collect();
    let labels: Vec<(String, usize)> = groups(&refs, NOW, 0)
        .into_iter()
        .map(|(label, items)| (label, items.len()))
        .collect();
    assert_eq!(
        labels,
        [
            ("Today".to_owned(), 2),
            ("Yesterday".to_owned(), 1),
            ("October 15".to_owned(), 1),
            ("October 17, 2025".to_owned(), 1),
        ]
    );
    // Four hours east it is 23:07 on October 17, and the entries at 00:07
    // and 23:07 UTC the night before fall on that same local day.
    let east: Vec<usize> = groups(&refs, NOW, 4 * 3600)
        .into_iter()
        .map(|(_, items)| items.len())
        .collect();
    assert_eq!(east[0], 3);
    assert!(groups(&[], NOW, 0).is_empty());
}

#[test]
fn f10_ac6_format_round_trips() {
    let mut h = History::default();
    let mut e = entry("PATCH", "https://a.test/{{id}}", NOW);
    e.outcome = Outcome::Response {
        status: 404,
        reason: "Not Found".into(),
        elapsed_ms: 1_234,
        size: 2_000_000,
        headers: vec![Header {
            name: "content-type".into(),
            value: "text/plain".into(),
        }],
        truncated: true,
    };
    h.record(e);
    let mut failed = entry("GET", "https://down.test", NOW + 5);
    failed.outcome = Outcome::Failed {
        error: "Connection refused by down.test:443".into(),
    };
    h.record(failed);
    let text = history::write(&h);
    assert!(text.contains("kind = \"failed\""), "{text}");
    assert_eq!(history::parse(&text).unwrap(), h);
    assert_eq!(history::parse("").unwrap(), History::default());
    assert!(history::parse("entries = 3").is_err());
    assert!(history::parse("[[entries]]\nid = 1").is_err());
}

proptest! {
    #[test]
    fn f10_ac2_prop_never_more_than_100_and_dropped_ids_are_gone(n in 0usize..250) {
        let mut h = History::default();
        let mut alive = std::collections::BTreeSet::new();
        for i in 0..n {
            let (id, dropped) = h.record(entry("GET", &i.to_string(), NOW));
            alive.insert(id);
            for d in dropped {
                prop_assert!(alive.remove(&d));
            }
        }
        prop_assert!(h.entries.len() <= MAX_ENTRIES);
        let ids: std::collections::BTreeSet<u64> = h.entries.iter().map(|e| e.id).collect();
        prop_assert_eq!(ids, alive);
    }
}
