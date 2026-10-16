//! F01-AC1 (methods), F01-AC5 (URL validation).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::request::{Method, MethodError, UrlError, parse_url};
use proptest::prelude::*;

#[test]
fn f01_ac1_default_method_is_get() {
    assert_eq!(Method::default(), Method::Get);
}

#[test]
fn f01_ac1_standard_methods_in_menu_order() {
    let names: Vec<&str> = Method::STANDARD.iter().map(Method::as_str).collect();
    assert_eq!(
        names,
        ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
    );
}

#[test]
fn f01_ac1_standard_names_parse_case_insensitively() {
    assert_eq!(Method::parse("get"), Ok(Method::Get));
    assert_eq!(Method::parse(" Post "), Ok(Method::Post));
    for m in Method::STANDARD {
        assert_eq!(Method::parse(m.as_str()).as_ref(), Ok(m));
        assert_eq!(Method::parse(&m.as_str().to_lowercase()).as_ref(), Ok(m));
    }
}

#[test]
fn f01_ac1_custom_token_kept_verbatim() {
    let m = Method::parse("PROPFIND").unwrap();
    assert_eq!(m, Method::Custom("PROPFIND".into()));
    assert_eq!(m.to_string(), "PROPFIND");
    assert_eq!(Method::parse("x-Purge!").unwrap().as_str(), "x-Purge!");
}

#[test]
fn f01_ac1_invalid_methods_rejected_with_reason() {
    assert_eq!(Method::parse(""), Err(MethodError::Empty));
    assert_eq!(Method::parse("   "), Err(MethodError::Empty));
    assert_eq!(Method::parse("GE T"), Err(MethodError::InvalidChar(' ')));
    assert_eq!(Method::parse("GET("), Err(MethodError::InvalidChar('(')));
    assert_eq!(Method::parse("GÉT"), Err(MethodError::InvalidChar('É')));
    assert_eq!(MethodError::Empty.to_string(), "Method cannot be empty");
    assert_eq!(
        MethodError::InvalidChar(' ').to_string(),
        "Method contains invalid character ' '"
    );
}

#[test]
fn f01_ac5_valid_urls_parse() {
    for ok in [
        "http://example.com",
        "https://example.com/a/b?c=d#e",
        "  https://api.example.com/users  ",
        "http://localhost:8080/x",
        "http://127.0.0.1:3000",
        "http://[::1]:8080/",
        "HTTPS://Example.COM/Path",
    ] {
        assert!(parse_url(ok).is_ok(), "{ok:?} should parse");
    }
    assert_eq!(
        parse_url(" http://a.test/x ").unwrap().as_str(),
        "http://a.test/x"
    );
}

#[test]
fn f01_ac5_invalid_urls_rejected_with_exact_message() {
    for bad in [
        "",
        "   ",
        "example.com",
        "example.com/users",
        "ftp://example.com",
        "file:///etc/passwd",
        "http://",
        "https:///path",
        "mailto:a@b.c",
        "localhost:8080",
        "http//example.com",
        "http:/example.com",
        r"http:\\example.com",
        r"https://\example.com",
    ] {
        assert_eq!(parse_url(bad), Err(UrlError), "{bad:?} should be rejected");
    }
    assert_eq!(
        UrlError.to_string(),
        "Enter a URL starting with http:// or https://"
    );
}

#[test]
fn f01_ac5_prefix_with_empty_host_rejected() {
    for bad in [
        "http://:80",
        "http://@/",
        "https://?q",
        "http://#f",
        "http:// ",
    ] {
        assert_eq!(parse_url(bad), Err(UrlError), "{bad:?} should be rejected");
    }
}

proptest! {
    /// Inputs that pass the prefix check: every accepted one is http(s) with a host.
    #[test]
    fn f01_ac5_prop_accepted_urls_are_http_with_host(
        scheme in "(?i)https?",
        rest in r"[^/\\]\PC{0,30}",
    ) {
        if let Ok(u) = parse_url(&format!("{scheme}://{rest}")) {
            prop_assert!(u.scheme() == "http" || u.scheme() == "https");
            prop_assert!(u.host_str().is_some_and(|h| !h.is_empty()), "{}", u);
        }
    }

    #[test]
    fn f01_ac1_prop_parse_never_panics_and_roundtrips(s in r"\PC{0,20}") {
        if let Ok(m) = Method::parse(&s) {
            prop_assert_eq!(Method::parse(m.as_str()), Ok(m.clone()));
        }
    }

    #[test]
    fn f01_ac5_prop_parse_url_never_panics(s in r"\PC{0,60}") {
        if let Ok(u) = parse_url(&s) {
            prop_assert!(u.scheme() == "http" || u.scheme() == "https");
            prop_assert!(u.host_str().is_some_and(|h| !h.is_empty()));
        }
    }
}
