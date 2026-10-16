//! F06-AC6: pretty-printing arbitrary bytes never panics or explodes; when it
//! succeeds the output is valid JSON, bounded in size, and a fixed point.
#![no_main]

use ferry_core::json::{pretty, validate};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|body: &[u8]| {
    let Some(out) = pretty(body) else { return };
    assert!(out.len() <= body.len() * 8 + 64 * 1024 + 1);
    let text = std::str::from_utf8(&out).expect("pretty output is UTF-8");
    if let Err(e) = validate(text) {
        // serde_json rejects numbers beyond f64 and nesting beyond 128 levels;
        // the JSON grammar does not.
        assert!(
            e.reason.contains("out of range") || e.reason.contains("recursion"),
            "pretty output is not JSON: {e}"
        );
    }
    assert_eq!(pretty(&out).as_deref(), Some(&out[..]), "not idempotent");
});
