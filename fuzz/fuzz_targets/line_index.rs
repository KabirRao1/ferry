//! F06-AC9: indexing arbitrary bytes never panics, and every line range is
//! in bounds, ordered and free of newlines.
#![no_main]

use std::num::NonZeroU32;

use ferry_core::text::LineIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (u8, &[u8])| {
    let (wrap, body) = input;
    let Some(wrap) = NonZeroU32::new(u32::from(wrap)) else { return };
    let Ok(index) = LineIndex::build(body, wrap) else { return };
    let mut cursor = 0;
    for i in 0..index.line_count() {
        let Some(r) = index.line(body, i) else { panic!("line {i} missing") };
        assert!(r.start >= cursor && r.end <= body.len());
        assert!(!body[r.clone()].contains(&b'\n'));
        cursor = r.end;
    }
    assert!(index.line(body, index.line_count()).is_none());
});
