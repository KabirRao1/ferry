//! F17-AC1: parsing any `Set-Cookie` header for any host and path never
//! panics; a stored cookie is well-formed; the jar survives being written
//! and read back.
#![no_main]

use ferry_core::cookies::{self, Jar, parse_date, parse_set_cookie};
use libfuzzer_sys::fuzz_target;

/// 2026-10-17T19:07:20Z.
const NOW: i64 = 1_792_264_040_000;

fuzz_target!(|data: &[u8]| {
    // header \0 host \0 path
    let mut parts = data.split(|b| *b == 0).map(std::str::from_utf8);
    let (Some(Ok(header)), host, path) = (parts.next(), parts.next(), parts.next()) else {
        return;
    };
    let host = host.and_then(Result::ok).unwrap_or("api.test");
    let path = path.and_then(Result::ok).unwrap_or("/");
    let _ = parse_date(header);
    if let Ok(cookie) = parse_set_cookie(header, host, path, NOW) {
        assert!(!cookie.name.is_empty());
        assert!(cookie.path.starts_with('/'), "{cookie:?}");
        if cookie.host_only {
            // The request's own host, whatever it is.
            assert_eq!(cookie.domain, host.to_ascii_lowercase(), "{cookie:?}");
        } else {
            assert!(!cookie.domain.starts_with('.'), "{cookie:?}");
            assert_eq!(cookie.domain, cookie.domain.to_ascii_lowercase());
        }
    }
    let mut jar = Jar::default();
    let _ = jar.store(header, host, path, NOW);
    let _ = jar.header_for(host, path, true, NOW);
    let text = cookies::write(&jar);
    let back = cookies::parse(&text, NOW).expect("a written jar parses");
    assert_eq!(back, jar, "{text}");
});
