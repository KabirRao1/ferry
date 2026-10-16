//! F06-AC9 / F06-AC11: display-line indexing of response bodies.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::num::NonZeroU32;

use ferry_core::text::{IndexError, LineIndex, hex_line, hex_line_count, looks_binary};
use proptest::prelude::*;

fn wrap(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

fn lines(body: &[u8], w: u32) -> Vec<&[u8]> {
    let idx = LineIndex::build(body, wrap(w)).unwrap();
    (0..idx.line_count())
        .map(|i| &body[idx.line(body, i).unwrap()])
        .collect()
}

#[test]
fn f06_ac9_empty_body_has_one_empty_line() {
    assert_eq!(lines(b"", 80), vec![b"" as &[u8]]);
}

#[test]
fn f06_ac9_splits_on_newline() {
    assert_eq!(lines(b"a\nbc\nd", 80), vec![&b"a"[..], b"bc", b"d"]);
}

#[test]
fn f06_ac9_trailing_newline_yields_final_empty_line() {
    assert_eq!(lines(b"a\n", 80), vec![&b"a"[..], b""]);
}

#[test]
fn f06_ac9_leading_empty_lines() {
    assert_eq!(lines(b"\na", 80), vec![&b""[..], b"a"]);
    assert_eq!(lines(b"\r\n\r\n", 80), vec![&b""[..], b"", b""]);
}

#[test]
fn f06_ac9_crlf_excludes_carriage_return() {
    assert_eq!(lines(b"a\r\nb\r\n", 80), vec![&b"a"[..], b"b", b""]);
}

#[test]
fn f06_ac9_lone_carriage_return_is_content() {
    assert_eq!(lines(b"a\rb", 80), vec![&b"a\rb"[..]]);
}

#[test]
fn f06_ac9_crlf_at_wrap_boundary_adds_no_line() {
    // The `\r` of CRLF is a terminator, so "ab" fits width 2 exactly.
    assert_eq!(lines(b"ab\r\nc", 2), vec![&b"ab"[..], b"c"]);
}

#[test]
fn f06_ac9_lone_carriage_return_at_end_wraps_as_content() {
    // No `\n` follows, so the final `\r` is content and counts toward the width.
    assert_eq!(lines(b"ab\r", 2), vec![&b"ab"[..], b"\r"]);
}

#[test]
fn f06_ac9_long_line_wraps_at_width() {
    assert_eq!(
        lines(b"abcdefg\nhi", 3),
        vec![&b"abc"[..], b"def", b"g", b"hi"]
    );
}

#[test]
fn f06_ac9_line_exactly_width_does_not_add_empty_line() {
    assert_eq!(lines(b"abc\nd", 3), vec![&b"abc"[..], b"d"]);
}

#[test]
fn f06_ac9_wrap_never_splits_utf8_char() {
    // "é" is 2 bytes, "€" is 3 bytes.
    let body = "aé€b".as_bytes();
    let got = lines(body, 2);
    for l in &got {
        assert!(
            std::str::from_utf8(l).is_ok(),
            "split inside a char: {got:?}"
        );
        assert!(l.len() <= 3);
    }
    assert_eq!(got.concat(), body);
}

#[test]
fn f06_ac9_wide_char_wider_than_wrap_still_progresses() {
    // A 3-byte char with wrap=1 must occupy its own line rather than loop forever.
    assert_eq!(
        lines("€€".as_bytes(), 1),
        vec!["€".as_bytes(), "€".as_bytes()]
    );
}

#[test]
fn f06_ac9_invalid_utf8_wraps_bytewise() {
    let body = [0xff, 0xfe, 0xfd, 0xfc, 0xfb];
    assert_eq!(lines(&body, 2), vec![&body[0..2], &body[2..4], &body[4..5]]);
}

#[test]
fn f06_ac9_out_of_bounds_line_is_none() {
    let body = b"a\nb";
    let idx = LineIndex::build(body, wrap(80)).unwrap();
    assert_eq!(idx.line(body, 2), None);
    assert_eq!(idx.line(body, usize::MAX), None);
}

#[test]
fn f06_ac11_error_message_names_sizes() {
    let err = IndexError::TooLarge {
        len: 5_000_000_000,
        max: LineIndex::MAX_LEN,
    };
    assert_eq!(
        err.to_string(),
        "body is 5000000000 bytes; the viewer supports bodies up to 4294967295 bytes"
    );
}

proptest! {
    /// Concatenating display lines plus their stripped terminators reproduces the body.
    #[test]
    fn f06_ac9_prop_lines_cover_body_in_order(body in proptest::collection::vec(any::<u8>(), 0..512), w in 1u32..16) {
        let idx = LineIndex::build(&body, wrap(w)).unwrap();
        let mut cursor = 0usize;
        for i in 0..idx.line_count() {
            let r = idx.line(&body, i).unwrap();
            prop_assert!(r.start >= cursor);
            // Anything skipped between lines is a line terminator.
            let gap = &body[cursor..r.start];
            prop_assert!(gap == b"\n" || gap == b"\r\n" || gap.is_empty(), "gap {:?}", gap);
            prop_assert!(!body[r.clone()].contains(&b'\n'));
            // Wrapped lines respect the width unless a single (up to 4-byte) char is wider.
            prop_assert!(r.len() <= (w as usize).max(4));
            cursor = r.end;
        }
        let tail = &body[cursor..];
        prop_assert!(tail.is_empty());
    }

    #[test]
    fn f06_ac9_prop_utf8_lines_stay_valid(s in r"\PC{0,200}", w in 1u32..12) {
        let body = s.as_bytes();
        let idx = LineIndex::build(body, wrap(w)).unwrap();
        for i in 0..idx.line_count() {
            let r = idx.line(body, i).unwrap();
            prop_assert!(std::str::from_utf8(&body[r]).is_ok());
        }
    }
}

// --- F06-AC7 hex view -------------------------------------------------------

#[test]
fn f06_ac7_hex_lines_show_offset_bytes_and_ascii() {
    let body: Vec<u8> = (0u8..20).chain(*b"Hi!").collect();
    assert_eq!(hex_line_count(body.len()), 2);
    assert_eq!(
        hex_line(&body, 0).unwrap(),
        "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|"
    );
    assert_eq!(
        hex_line(&body, 1).unwrap(),
        "00000010  10 11 12 13 48 69 21                              |....Hi!|"
    );
    assert_eq!(hex_line(&body, 2), None);
}

#[test]
fn f06_ac7_empty_body_has_one_empty_hex_line() {
    assert_eq!(hex_line_count(0), 1);
    assert_eq!(
        hex_line(b"", 0).unwrap(),
        format!("00000000 {}  ||", " ".repeat(49))
    );
    assert_eq!(hex_line(b"", 1), None);
}

#[test]
fn f06_ac7_binary_detection() {
    assert!(!looks_binary(b""));
    assert!(!looks_binary("plain text, é ✓\n".as_bytes()));
    assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0"));
    assert!(looks_binary(b"caf\xe9"));
    // A multi-byte character cut by the 8 KiB window is still text.
    let mut long = vec![b'a'; 8191];
    long.extend_from_slice("é tail".as_bytes());
    assert!(!looks_binary(&long));
    // But a body that itself ends mid-character is not.
    assert!(looks_binary(&"é".as_bytes()[..1]));
}

proptest! {
    #[test]
    fn f06_ac7_prop_hex_lines_cover_body(body in proptest::collection::vec(any::<u8>(), 0..100)) {
        let n = hex_line_count(body.len());
        let mut decoded = Vec::new();
        for i in 0..n {
            let line = hex_line(&body, i).unwrap();
            prop_assert_eq!(line.len(), 62 + body.len().saturating_sub(i * 16).min(16));
            let hex = &line[10..58];
            decoded.extend(hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()));
        }
        prop_assert_eq!(decoded, body);
    }
}
