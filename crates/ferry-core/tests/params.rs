//! F01-AC2..AC4, F01-AC6, F01-AC8: URL and params table in two-way sync.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::kv::KeyValue;
use ferry_core::params::{Param, escape, parse_query, rows_from_url, url_with_rows};
use proptest::prelude::*;

fn pairs(rows: &[Param]) -> Vec<(&str, &str, bool)> {
    rows.iter()
        .map(|p| (p.row.key.as_str(), p.row.value.as_str(), p.row.enabled))
        .collect()
}

fn disabled(key: &str, value: &str) -> Param {
    let mut p = Param::new(key, value);
    p.row.enabled = false;
    p
}

#[test]
fn f01_ac2_table_lists_pairs_in_order_as_written_with_duplicates() {
    let rows = rows_from_url("https://a.test/x?b=2&a=1&b=3&q=a%20b&flag#frag", &[]);
    assert_eq!(
        pairs(&rows),
        [
            ("b", "2", true),
            ("a", "1", true),
            ("b", "3", true),
            ("q", "a%20b", true),
            ("flag", "", true),
        ]
    );
}

#[test]
fn f01_ac2_no_query_or_empty_segments_give_no_rows() {
    assert!(rows_from_url("https://a.test/x", &[]).is_empty());
    assert!(rows_from_url("https://a.test/x?", &[]).is_empty());
    assert_eq!(
        parse_query("https://a.test/?&&a=1&"),
        [("a".to_owned(), "1".to_owned())]
    );
    // A `?` inside the fragment is not a query.
    assert!(parse_query("https://a.test/#x?a=1").is_empty());
}

#[test]
fn f01_ac2_value_may_contain_equals() {
    let rows = rows_from_url("http://a.test/?token=ab==&x==y", &[]);
    assert_eq!(pairs(&rows), [("token", "ab==", true), ("x", "=y", true)]);
}

#[test]
fn f01_ac3_editing_a_row_updates_the_query() {
    let url = "https://a.test/x?a=1&b=2#top";
    let mut rows = rows_from_url(url, &[]);
    rows[1].row.value = "two words".into();
    assert_eq!(
        url_with_rows(url, &rows),
        "https://a.test/x?a=1&b=two words#top"
    );
}

#[test]
fn f01_ac3_adding_removing_and_disabling_rows() {
    let url = "https://a.test/x?a=1&b=2";
    let mut rows = rows_from_url(url, &[]);
    rows.push(Param::new("c", "3"));
    assert_eq!(url_with_rows(url, &rows), "https://a.test/x?a=1&b=2&c=3");
    rows[0].row.enabled = false;
    assert_eq!(url_with_rows(url, &rows), "https://a.test/x?b=2&c=3");
    rows.remove(1);
    assert_eq!(url_with_rows(url, &rows), "https://a.test/x?c=3");
}

#[test]
fn f01_ac3_empty_table_drops_question_mark_keeps_fragment() {
    assert_eq!(
        url_with_rows("https://a.test/x?a=1#f", &[]),
        "https://a.test/x#f"
    );
    assert_eq!(
        url_with_rows("https://a.test/x?a=1", &[disabled("a", "1")]),
        "https://a.test/x"
    );
}

#[test]
fn f01_ac3_rows_without_key_are_left_out_and_empty_value_is_key_only() {
    let rows = [Param::new("", "orphan"), Param::new("flag", "")];
    assert_eq!(
        url_with_rows("http://a.test/", &rows),
        "http://a.test/?flag"
    );
}

#[test]
fn f01_ac3_structure_characters_in_edited_rows_are_escaped() {
    let rows = [Param::new("a=b&c#d", "x&y=z#w")];
    assert_eq!(
        url_with_rows("http://a.test/", &rows),
        "http://a.test/?a%3Db%26c%23d=x%26y=z%23w"
    );
    assert_eq!(escape("plain {{v}} %41", false), "plain {{v}} %41");
}

#[test]
fn f01_ac4_untouched_rows_keep_their_original_encoding() {
    let url = "http://a.test/?q=a+b%20c&e=&f&g=%7B";
    let mut rows = rows_from_url(url, &[]);
    rows.push(Param::new("new", "1"));
    assert_eq!(
        url_with_rows(url, &rows),
        "http://a.test/?q=a+b%20c&e=&f&g=%7B&new=1"
    );
}

#[test]
fn f01_ac4_disabled_rows_keep_their_position_when_url_is_edited() {
    let previous = vec![
        Param::new("a", "1"),
        disabled("off", "x"),
        Param::new("b", "2"),
    ];
    let rows = rows_from_url("http://a.test/?a=9&b=8&c=7", &previous);
    assert_eq!(
        pairs(&rows),
        [
            ("a", "9", true),
            ("off", "x", false),
            ("b", "8", true),
            ("c", "7", true),
        ]
    );
    // Fewer pairs than before: the extra enabled rows go, disabled ones stay.
    let rows = rows_from_url("http://a.test/", &previous);
    assert_eq!(pairs(&rows), [("off", "x", false)]);
}

#[test]
fn f01_ac8_descriptions_follow_unchanged_keys() {
    let mut previous = rows_from_url("http://a.test/?a=1&b=2", &[]);
    previous[0].row.description = "first".into();
    previous[1].row.description = "second".into();
    let rows = rows_from_url("http://a.test/?a=5&z=2", &previous);
    assert_eq!(rows[0].row.description, "first");
    assert_eq!(rows[1].row.description, "");
}

#[test]
fn f01_ac6_placeholders_survive_both_directions() {
    let url = "{{base}}/users?id={{user_id}}&page={{page}}";
    let mut rows = rows_from_url(url, &[]);
    assert_eq!(
        pairs(&rows),
        [("id", "{{user_id}}", true), ("page", "{{page}}", true)]
    );
    rows[1].row.value = "{{next}}".into();
    rows.push(Param::new("{{k}}", "{{v}}"));
    assert_eq!(
        url_with_rows(url, &rows),
        "{{base}}/users?id={{user_id}}&page={{next}}&{{k}}={{v}}"
    );
}

#[test]
fn f01_ac8_param_rows_have_description_and_enabled() {
    let p = Param::new("k", "v");
    assert_eq!(
        p.row,
        KeyValue {
            key: "k".into(),
            value: "v".into(),
            description: String::new(),
            enabled: true,
        }
    );
}

fn query_text() -> impl Strategy<Value = String> {
    // Keys and values without structure characters, plus some encodings.
    prop::collection::vec(
        (
            "[a-z{}%0-9+ ]{0,6}",
            prop::option::of("[a-z{}%0-9=+ ]{0,6}"),
        ),
        0..6,
    )
    .prop_map(|pairs| {
        pairs
            .into_iter()
            .map(|(k, v)| match v {
                Some(v) => format!("{k}={v}"),
                None => k,
            })
            .collect::<Vec<_>>()
            .join("&")
    })
}

proptest! {
    /// URL → table → URL is the identity, apart from empty segments.
    #[test]
    fn f01_ac4_prop_url_roundtrips_through_table(
        base in "https?://[a-z]{1,8}(/[a-z]{0,5})?",
        query in query_text(),
        fragment in prop::option::of("[a-z?=&]{0,6}"),
    ) {
        let mut url = base.clone();
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query);
        }
        if let Some(f) = &fragment {
            url.push('#');
            url.push_str(f);
        }
        let rows = rows_from_url(&url, &[]);
        let rebuilt = url_with_rows(&url, &rows);
        let segments: Vec<&str> = query.split('&').filter(|s| !s.is_empty() && !s.starts_with('=')).collect();
        let mut expected = base;
        if !segments.is_empty() {
            expected.push('?');
            expected.push_str(&segments.join("&"));
        }
        if let Some(f) = &fragment {
            expected.push('#');
            expected.push_str(f);
        }
        prop_assert_eq!(rebuilt, expected);
    }

    /// Table → URL → table gives back the enabled rows, escaped.
    #[test]
    fn f01_ac4_prop_table_roundtrips_through_url(
        rows in prop::collection::vec(("[a-z&#={}% ]{1,6}", "[a-z&#={}% ]{0,6}", any::<bool>()), 0..6),
    ) {
        let params: Vec<Param> = rows
            .iter()
            .map(|(k, v, on)| {
                let mut p = Param::new(k.clone(), v.clone());
                p.row.enabled = *on;
                p
            })
            .collect();
        let url = url_with_rows("http://a.test/p", &params);
        let back = rows_from_url(&url, &[]);
        let expected: Vec<(String, String)> = params
            .iter()
            .filter(|p| p.row.enabled)
            .map(|p| (escape(&p.row.key, true), escape(&p.row.value, false)))
            .collect();
        let got: Vec<(String, String)> = back.into_iter().map(|p| (p.row.key, p.row.value)).collect();
        prop_assert_eq!(got, expected);
    }
}
