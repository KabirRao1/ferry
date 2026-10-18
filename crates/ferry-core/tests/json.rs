//! F03-AC3 (validation, Beautify) and F06-AC6 (pretty view).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::json::{JsonError, pretty, validate};
use proptest::prelude::*;

fn pretty_str(s: &str) -> Option<String> {
    pretty(s.as_bytes()).map(|b| String::from_utf8(b).unwrap())
}

#[test]
fn f03_ac3_valid_json_passes() {
    for ok in [
        "{}",
        "[]",
        " 1 ",
        "\"s\"",
        "null",
        r#"{"a":[1,2,{"b":null}]}"#,
    ] {
        assert_eq!(validate(ok), Ok(()), "{ok}");
    }
}

#[test]
fn f03_ac3_invalid_json_names_line_column_and_reason() {
    let e = validate("[\n  1,\n]").unwrap_err();
    assert_eq!((e.line, e.column), (3, 1));
    assert_eq!(e.reason, "expected value");
    assert_eq!(
        e.to_string(),
        "Invalid JSON at line 3, column 1: expected value"
    );
    let e = validate("").unwrap_err();
    assert_eq!(
        e,
        JsonError {
            line: 1,
            column: 0,
            reason: "EOF while parsing a value".into()
        }
    );
}

#[test]
fn f06_ac6_pretty_prints_with_two_spaces() {
    let out = pretty_str(r#"{"name":"ferry","tags":["a","b"],"n":{"x":1}}"#).unwrap();
    assert_eq!(
        out,
        "{\n  \"name\": \"ferry\",\n  \"tags\": [\n    \"a\",\n    \"b\"\n  ],\n  \"n\": {\n    \"x\": 1\n  }\n}"
    );
}

#[test]
fn f06_ac6_keeps_key_order_numbers_and_escapes_verbatim() {
    let out = pretty_str(r#"{"z":1.50,"a":1e3,"m":-0.0E+2,"s":"\u00e9\n\"q\"","big":123456789012345678901234567890}"#)
        .unwrap();
    assert_eq!(
        out,
        "{\n  \"z\": 1.50,\n  \"a\": 1e3,\n  \"m\": -0.0E+2,\n  \"s\": \"\\u00e9\\n\\\"q\\\"\",\n  \"big\": 123456789012345678901234567890\n}"
    );
}

#[test]
fn f06_ac6_empty_containers_and_scalars() {
    assert_eq!(pretty_str("{ }").unwrap(), "{}");
    assert_eq!(pretty_str("[\n]").unwrap(), "[]");
    assert_eq!(
        pretty_str(r#"{"a":{},"b":[[]]}"#).unwrap(),
        "{\n  \"a\": {},\n  \"b\": [\n    []\n  ]\n}"
    );
    assert_eq!(pretty_str("  true ").unwrap(), "true");
    assert_eq!(pretty_str("\"é\"").unwrap(), "\"é\"");
}

#[test]
fn f06_ac6_rejects_invalid_json() {
    for bad in [
        "",
        " ",
        "{",
        "}",
        "[1,]",
        "{\"a\"}",
        "{\"a\":}",
        "{a:1}",
        "[1 2]",
        "01",
        "1.",
        ".5",
        "-",
        "1e",
        "+1",
        "tru",
        "nul",
        "\"unterminated",
        "\"bad \\x escape\"",
        "\"\\u12G4\"",
        "\"ctrl \u{1}\"",
        "[1]]",
        "{} {}",
        "{\"a\":1,}",
        "[,1]",
        "{,}",
        "{\"a\" 1}",
        "[}",
        "{]",
        "1 2",
    ] {
        assert_eq!(pretty_str(bad), None, "{bad:?} should be rejected");
    }
    assert_eq!(pretty(b"\"\xff\""), None);
}

#[test]
fn f06_ac6_deep_nesting_does_not_overflow_the_stack() {
    let depth = 100;
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let out = pretty(text.as_bytes()).unwrap();
    assert!(out.starts_with(b"[\n  [\n    ["));
}

#[test]
fn f06_ac6_pathological_nesting_is_refused_not_exploded() {
    // 200 KB of brackets would pretty-print to about 10 GB of indentation.
    let depth = 100_000;
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    assert_eq!(pretty(text.as_bytes()), None);
    // The limit is 8x the input plus 64 KiB: wide, shallow JSON fits easily.
    let wide = format!("[{}1]", "1,".repeat(100_000));
    assert!(pretty(wide.as_bytes()).is_some());
}

fn arb_json() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| serde_json::json!(n)),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(|f| serde_json::json!(f)),
        "\\PC{0,8}".prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(4, 64, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
            prop::collection::vec(("[a-z]{0,4}", inner), 0..6)
                .prop_map(|kv| serde_json::Value::Object(kv.into_iter().collect())),
        ]
    })
}

proptest! {
    /// Pretty output parses to the same value and matches serde_json's own
    /// pretty-printer for its compact output.
    #[test]
    fn f06_ac6_prop_matches_serde_json(value in arb_json()) {
        let compact = serde_json::to_vec(&value).unwrap();
        let ours = pretty(&compact).unwrap();
        let theirs = serde_json::to_vec_pretty(&value).unwrap();
        prop_assert_eq!(String::from_utf8(ours).unwrap(), String::from_utf8(theirs).unwrap());
    }

    /// Pretty succeeds exactly when serde_json accepts the input.
    #[test]
    fn f06_ac6_prop_accepts_exactly_valid_json(s in "[\\[\\]{}\",:0-9a-z .eE+-]{0,24}") {
        // serde_json rejects numbers beyond f64; the JSON grammar does not.
        prop_assume!(validate(&s).err().is_none_or(|e| !e.reason.contains("out of range")));
        prop_assert_eq!(pretty(s.as_bytes()).is_some(), validate(&s).is_ok(), "{:?}", s);
    }

    /// Pretty is idempotent.
    #[test]
    fn f06_ac6_prop_idempotent(value in arb_json()) {
        let once = pretty(&serde_json::to_vec(&value).unwrap()).unwrap();
        let twice = pretty(&once).unwrap();
        prop_assert_eq!(once, twice);
    }
}

#[test]
fn f06_ac6_output_limit_is_exact() {
    use ferry_core::json::pretty_within;
    // "[\n  [\n    1\n  ]\n]": the last line break brings the output to 16 bytes.
    let input = b"[[1]]";
    assert_eq!(
        pretty_within(input, 16).as_deref(),
        Some(&b"[\n  [\n    1\n  ]\n]"[..])
    );
    assert_eq!(pretty_within(input, 15), None);
    // An earlier, deeper line break can be the one that crosses the limit.
    assert_eq!(pretty_within(b"[[[1,2,3]]]", 10), None);
    // Scalars have no line breaks and ignore the limit.
    assert_eq!(pretty_within(b"12345", 0).as_deref(), Some(&b"12345"[..]));
}

// --- F22-AC4 JSONPath ----------------------------------------------------------------

#[test]
fn f22_ac4_jsonpath_results_as_a_pretty_array() {
    use ferry_core::json::filter;
    let body = br#"{"store":{"book":[{"author":"Ada","price":8},{"author":"Grace","price":22}]}}"#;
    let out = filter(body, "$.store.book[*].author").unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "[\n  \"Ada\",\n  \"Grace\"\n]"
    );
    let out = filter(body, "$..book[?@.price > 10]").unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "[\n  {\n    \"author\": \"Grace\",\n    \"price\": 22\n  }\n]"
    );
    assert_eq!(filter(body, "$.nothing").unwrap(), b"[]");
}

#[test]
fn f22_ac4_invalid_expressions_and_bodies() {
    use ferry_core::json::filter;
    let error = filter(b"{}", "store.book").unwrap_err();
    assert!(error.starts_with("Invalid JSONPath: "), "{error}");
    let error = filter(b"{not json", "$").unwrap_err();
    assert!(error.starts_with("Invalid JSON: "), "{error}");
}
