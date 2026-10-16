//! F06-AC9 / F06-AC10: body viewer scroll state.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::num::NonZeroU32;

use bytes::Bytes;

use ferry_app::body_view::{BodyView, Message};

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
