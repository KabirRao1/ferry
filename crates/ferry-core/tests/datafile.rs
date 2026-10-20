//! Collection Runner data files (F16-AC3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::datafile::{INVALID, Row, parse};
use proptest::prelude::*;

fn row(pairs: &[(&str, &str)]) -> Row {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn f16_ac3_csv_with_a_header_row() {
    let rows = parse("user,pass\nada,1\r\n\"b, c\",\"say \"\"hi\"\"\nthere\"\n\nlast,\n").unwrap();
    assert_eq!(
        rows,
        [
            row(&[("user", "ada"), ("pass", "1")]),
            row(&[("user", "b, c"), ("pass", "say \"hi\"\nthere")]),
            row(&[("user", "last"), ("pass", "")]),
        ]
    );
    // Short rows get empty values; a byte order mark and no final newline are fine.
    assert_eq!(
        parse("\u{feff}a,b\n1").unwrap(),
        [row(&[("a", "1"), ("b", "")])]
    );
    // Columns without a name are left out.
    assert_eq!(
        parse("a,,b\n1,2,3").unwrap(),
        [row(&[("a", "1"), ("b", "3")])]
    );
}

#[test]
fn f16_ac3_json_array_of_objects() {
    let rows = parse(r#" [{"user": "ada", "n": 1, "ok": true, "o": {"x": null}}, {}] "#).unwrap();
    assert_eq!(
        rows,
        [
            row(&[
                ("n", "1"),
                ("o", r#"{"x":null}"#),
                ("ok", "true"),
                ("user", "ada")
            ]),
            row(&[]),
        ]
    );
}

#[test]
fn f16_ac3_anything_else_is_refused() {
    for bad in [
        "",
        "\n\n",
        ",,\n1,2",
        "a\n1,2",
        "a\n\"open",
        "[1, 2]",
        "[{\"a\": 1}, 3]",
        "[",
        "[]",
        "a,b",
    ] {
        let error = parse(bad).unwrap_err();
        assert_eq!(error.to_string(), INVALID, "{bad:?}");
    }
}

proptest! {
    #[test]
    fn f16_ac3_prop_quoted_csv_round_trips(values in proptest::collection::vec("\\PC{0,8}", 1..5)) {
        let quote = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
        let header: Vec<String> = (0..values.len()).map(|i| format!("c{i}")).collect();
        let text = format!(
            "{}\n{}\n",
            header.join(","),
            values.iter().map(|v| quote(v)).collect::<Vec<_>>().join(",")
        );
        let rows = parse(&text).unwrap();
        let expected: Row = header.into_iter().zip(values).collect();
        prop_assert_eq!(rows, vec![expected]);
    }

    #[test]
    fn f16_ac3_prop_never_panics(text in "\\PC{0,80}") {
        let _ = parse(&text);
    }
}
