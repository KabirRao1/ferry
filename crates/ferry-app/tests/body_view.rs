//! F06-AC9 / F06-AC10: body viewer scroll state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::num::NonZeroU32;

use bytes::Bytes;

use ferry_app::body_view::{BodyView, Mark, Message};

fn view(body: &str) -> BodyView {
    BodyView::new(
        Bytes::copy_from_slice(body.as_bytes()),
        NonZeroU32::new(80).unwrap(),
    )
    .unwrap()
}

fn texts(v: &BodyView, rows: usize) -> Vec<(usize, String)> {
    v.visible(rows)
        .into_iter()
        .map(|(n, t)| (n, t.into_owned()))
        .collect()
}

#[test]
fn f06_ac9_starts_at_top() {
    let v = view("a\nb\nc");
    assert_eq!(v.top(), 0);
    assert_eq!(v.line_count(), 3);
}

#[test]
fn f06_ac9_visible_returns_rows_from_top() {
    let mut v = view("a\nb\nc\nd");
    v.update(Message::ScrolledBy(1));
    assert_eq!(texts(&v, 2), vec![(1, "b".into()), (2, "c".into())]);
}

#[test]
fn f06_ac9_visible_truncates_at_end() {
    let mut v = view("a\nb\nc");
    v.update(Message::ScrolledTo(2));
    assert_eq!(texts(&v, 10), vec![(2, "c".into())]);
}

#[test]
fn f06_ac9_scroll_clamps_both_ends() {
    let mut v = view("a\nb\nc");
    v.update(Message::ScrolledBy(-5));
    assert_eq!(v.top(), 0);
    v.update(Message::ScrolledBy(isize::MAX));
    assert_eq!(v.top(), 2);
    v.update(Message::ScrolledBy(isize::MIN));
    assert_eq!(v.top(), 0);
    v.update(Message::ScrolledTo(usize::MAX));
    assert_eq!(v.top(), 2);
}

#[test]
fn f06_ac9_invalid_utf8_is_replaced() {
    let v = BodyView::new(
        Bytes::from_static(&[b'a', 0xff, b'b']),
        NonZeroU32::new(80).unwrap(),
    )
    .unwrap();
    assert_eq!(texts(&v, 1), vec![(0, "a\u{fffd}b".into())]);
}

#[test]
fn f06_ac10_scrolling_50mb_body_is_constant_time() {
    let line = "{\"id\": 1, \"name\": \"ferry\"},\n";
    let body = line.repeat(50 * 1024 * 1024 / line.len());
    let mut v = view(&body);
    let n = v.line_count();
    assert!(n > 1_000_000);
    let start = std::time::Instant::now();
    for i in 0..10_000 {
        v.update(Message::ScrolledTo((i * 7919) % n));
        let rows = v.visible(60);
        assert_eq!(rows.len(), 60.min(n - v.top()));
    }
    // 10k scroll+render-data steps must fit well inside one frame each.
    assert!(
        start.elapsed().as_millis() < 10_000 * 16 / 100,
        "too slow: {:?}",
        start.elapsed()
    );
}

// --- F06-AC8 search highlights -------------------------------------------------------

type Pieces = Vec<(usize, Vec<(String, Mark)>)>;

fn marked(
    v: &BodyView,
    rows: usize,
    matches: &[usize],
    len: usize,
    current: Option<usize>,
) -> Pieces {
    v.visible_marked(rows, matches, len, current)
        .into_iter()
        .map(|(n, pieces)| {
            (
                n,
                pieces
                    .into_iter()
                    .map(|(t, m)| (t.into_owned(), m))
                    .collect(),
            )
        })
        .collect()
}

fn plain(t: &str) -> (String, Mark) {
    (t.into(), Mark::Plain)
}

#[test]
fn f06_ac8_lines_split_around_matches() {
    let v = view("abcabc\nxbc\n\nbc");
    let got = marked(&v, 4, &[1, 4, 8, 12], 2, Some(1));
    assert_eq!(
        got,
        vec![
            (
                0,
                vec![
                    plain("a"),
                    ("bc".into(), Mark::Match),
                    plain("a"),
                    ("bc".into(), Mark::Current),
                ]
            ),
            (1, vec![plain("x"), ("bc".into(), Mark::Match)]),
            (2, vec![plain("")]),
            (3, vec![("bc".into(), Mark::Match)]),
        ]
    );
}

#[test]
fn f06_ac8_without_matches_lines_are_whole() {
    let v = view("ab\ncd");
    assert_eq!(
        marked(&v, 2, &[], 2, None),
        vec![(0, vec![plain("ab")]), (1, vec![plain("cd")])]
    );
    assert_eq!(
        marked(&v, 1, &[0], 0, Some(0)),
        vec![(0, vec![plain("ab")])],
        "empty needle"
    );
}

#[test]
fn f06_ac8_a_match_split_by_wrapping_is_marked_on_both_lines() {
    // Lines wrap at 4 bytes: "abcd", "efgh".
    let v = BodyView::new(Bytes::from_static(b"abcdefgh"), NonZeroU32::new(4).unwrap()).unwrap();
    assert_eq!(
        marked(&v, 2, &[2], 4, Some(0)),
        vec![
            (0, vec![plain("ab"), ("cd".into(), Mark::Current)]),
            (1, vec![("ef".into(), Mark::Current), plain("gh")]),
        ]
    );
}

#[test]
fn f06_ac8_only_matches_on_visible_lines_are_considered() {
    let mut v = view("a\nb\na\nb\na");
    v.update(Message::ScrolledTo(2));
    assert_eq!(
        marked(&v, 2, &[0, 4, 8], 1, Some(2)),
        vec![(2, vec![("a".into(), Mark::Match)]), (3, vec![plain("b")])]
    );
}

#[test]
fn f06_ac8_hex_has_no_marks() {
    let v = BodyView::hex(Bytes::from_static(b"abc"));
    let got = marked(&v, 1, &[0], 1, Some(0));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1.len(), 1);
    assert_eq!(got[0].1[0].1, Mark::Plain);
}

#[test]
fn f06_ac8_reveal_scrolls_only_when_needed() {
    let body: String = (0..100).map(|i| format!("{i}\n")).collect();
    let mut v = view(&body);
    let offset_of = |line: usize| body.lines().take(line).map(|l| l.len() + 1).sum::<usize>();
    v.reveal(offset_of(50));
    assert_eq!(v.top(), 41, "viewport unknown: 30 rows assumed, 50 - 29/3");
    v.update(Message::Resized(10));
    v.reveal(offset_of(49));
    assert_eq!(v.top(), 41, "fully visible");
    v.reveal(offset_of(50));
    assert_eq!(
        v.top(),
        47,
        "the last row may be cut off, so it scrolls: 50 - 9/3"
    );
    v.reveal(offset_of(1));
    assert_eq!(v.top(), 0);
    v.reveal(offset_of(99));
    assert_eq!(
        v.top(),
        v.line_count() - 9,
        "no blank space after the last line"
    );
    let mut short = view("a\nb\nmatch\nc");
    short.reveal(4);
    assert_eq!(short.top(), 0, "a body that fits never scrolls");
    let mut hex = BodyView::hex(Bytes::from(body.clone()));
    hex.reveal(offset_of(50));
    assert_eq!(hex.top(), 0, "hex does not move");
}

#[test]
fn f06_ac8_top_offset_and_body() {
    let mut v = view("ab\ncd\nef");
    assert_eq!(v.top_offset(), 0);
    v.update(Message::ScrolledTo(2));
    assert_eq!(v.top_offset(), 6);
    assert_eq!(v.body().as_ref(), b"ab\ncd\nef");
    assert!(v.is_text());
    assert_eq!(BodyView::hex(Bytes::new()).top_offset(), 0);
    assert!(!BodyView::hex(Bytes::new()).is_text());
}

#[test]
fn f06_ac8_resizing_keeps_the_top_and_records_rows() {
    let mut v = view("a\nb\nc");
    v.update(Message::ScrolledTo(2));
    v.update(Message::Resized(40));
    assert_eq!((v.top(), v.rows()), (2, 40));
}
