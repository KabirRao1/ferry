//! F02-AC4: bulk edit of key-value rows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::kv::{KeyValue, from_bulk, to_bulk};
use proptest::prelude::*;

fn off(key: &str, value: &str) -> KeyValue {
    KeyValue {
        enabled: false,
        ..KeyValue::new(key, value)
    }
}

#[test]
fn f02_ac4_rows_to_bulk_text() {
    let rows = [
        KeyValue::new("Accept", "application/json"),
        off("X-Debug", "1"),
        KeyValue::default(),
        KeyValue::new("X-Empty", ""),
    ];
    assert_eq!(
        to_bulk(&rows),
        "Accept: application/json\n//X-Debug: 1\nX-Empty: \n"
    );
}

#[test]
fn f02_ac4_bulk_text_to_rows() {
    let rows = from_bulk(
        "Accept: application/json\n\n  //X-Debug: 1\nNoColon\nTime: 12:30\nSpaced:  two\n",
        &[],
    );
    assert_eq!(
        rows,
        [
            KeyValue::new("Accept", "application/json"),
            off("X-Debug", "1"),
            KeyValue::new("NoColon", ""),
            KeyValue::new("Time", "12:30"),
            KeyValue::new("Spaced", " two"),
        ]
    );
}

#[test]
fn f02_ac4_descriptions_survive_unchanged_keys() {
    let previous = [
        KeyValue {
            description: "first a".into(),
            ..KeyValue::new("a", "1")
        },
        KeyValue {
            description: "b".into(),
            ..KeyValue::new("b", "2")
        },
        KeyValue {
            description: "second a".into(),
            ..KeyValue::new("a", "3")
        },
    ];
    let rows = from_bulk("a: x\nc: y\na: z\n", &previous);
    let descriptions: Vec<&str> = rows.iter().map(|r| r.description.as_str()).collect();
    assert_eq!(descriptions, ["first a", "", "second a"]);
}

#[test]
fn f02_ac1_active_rows_need_enabled_and_non_blank_key() {
    assert!(KeyValue::new("a", "").is_active());
    assert!(!KeyValue::new("  ", "v").is_active());
    assert!(!off("a", "v").is_active());
}

proptest! {
    #[test]
    fn f02_ac4_prop_bulk_roundtrip(
        rows in prop::collection::vec(("[A-Za-z0-9-]{1,8}", "[ -~]{0,10}", any::<bool>()), 0..8),
    ) {
        let rows: Vec<KeyValue> = rows
            .into_iter()
            .map(|(k, v, enabled)| KeyValue { enabled, ..KeyValue::new(k, v.trim_end()) })
            .collect();
        prop_assert_eq!(from_bulk(&to_bulk(&rows), &rows), rows);
    }
}
