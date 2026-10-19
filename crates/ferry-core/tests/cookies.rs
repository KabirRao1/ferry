//! The cookie jar: F17-AC1, AC2, AC5, AC6.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::cookies::{self, Cookie, CookieError, Jar, parse_date, parse_set_cookie};
use proptest::prelude::*;

/// 2026-10-17T19:07:20Z.
const NOW: i64 = 1_792_264_040_000;
const HOUR: i64 = 3_600_000;

fn stored(jar: &mut Jar, header: &str, host: &str, path: &str) {
    jar.store(header, host, path, NOW).unwrap();
}

// --- F17-AC1 parsing and storing ---------------------------------------------------

#[test]
fn f17_ac1_host_only_cookie_with_default_path() {
    let c = parse_set_cookie("sid=abc123", "API.test", "/v1/users/7", NOW).unwrap();
    assert_eq!(c.name, "sid");
    assert_eq!(c.value, "abc123");
    assert_eq!(c.domain, "api.test");
    assert!(c.host_only);
    assert_eq!(c.path, "/v1/users");
    assert_eq!(c.expires, None);
    assert!(!c.secure && !c.http_only);
    assert_eq!(c.created, NOW);
    for (request_path, default) in [
        ("/", "/"),
        ("", "/"),
        ("/x", "/"),
        ("noslash", "/"),
        ("/a/b/", "/a/b"),
    ] {
        let c = parse_set_cookie("a=1", "h.test", request_path, NOW).unwrap();
        assert_eq!(c.path, default, "{request_path:?}");
    }
}

#[test]
fn f17_ac1_attributes() {
    let c = parse_set_cookie(
        " token = v=1;Path=/api ; DOMAIN=.Example.TEST; Secure; httponly; SameSite=Lax; Unknown=x",
        "www.example.test",
        "/",
        NOW,
    )
    .unwrap();
    assert_eq!((c.name.as_str(), c.value.as_str()), ("token", "v=1"));
    assert_eq!(c.domain, "example.test");
    assert!(!c.host_only);
    assert_eq!(c.path, "/api");
    assert!(c.secure && c.http_only);
    assert_eq!(c.same_site.as_deref(), Some("Lax"));
    // A Path that does not start with / falls back to the default.
    let c = parse_set_cookie("a=1; Path=relative", "h.test", "/d/x", NOW).unwrap();
    assert_eq!(c.path, "/d");
    // An empty Domain or SameSite is ignored.
    let c = parse_set_cookie("a=1; Domain=; SameSite=", "h.test", "/", NOW).unwrap();
    assert!(c.host_only);
    assert_eq!(c.same_site, None);
}

#[test]
fn f17_ac1_max_age_beats_expires() {
    let c = parse_set_cookie(
        "a=1; Expires=Wed, 21 Oct 2026 07:28:00 GMT; Max-Age=60",
        "h.test",
        "/",
        NOW,
    )
    .unwrap();
    assert_eq!(c.expires, Some(NOW + 60_000));
    let c = parse_set_cookie(
        "a=1; Max-Age=60; Expires=Wed, 21 Oct 2026 07:28:00 GMT",
        "h.test",
        "/",
        NOW,
    )
    .unwrap();
    assert_eq!(c.expires, Some(NOW + 60_000), "order does not matter");
    let c = parse_set_cookie(
        "a=1; Expires=Wed, 21 Oct 2026 07:28:00 GMT",
        "h.test",
        "/",
        NOW,
    )
    .unwrap();
    assert_eq!(c.expires, parse_date("Wed, 21 Oct 2026 07:28:00 GMT"));
    let c = parse_set_cookie("a=1; Max-Age=nope", "h.test", "/", NOW).unwrap();
    assert_eq!(c.expires, None, "an invalid Max-Age is ignored");
    let c = parse_set_cookie("a=1; Max-Age=-5", "h.test", "/", NOW).unwrap();
    assert!(c.is_expired(NOW));
    let c = parse_set_cookie("a=1; Max-Age=0", "h.test", "/", NOW).unwrap();
    assert!(c.is_expired(NOW));
}

#[test]
fn f17_ac1_domain_must_match_and_have_two_labels() {
    for (domain, host) in [
        ("other.test", "api.example.test"),
        ("test", "api.test"),
        ("com", "example.com"),
        ("ample.test", "example.test"),
        ("sub.example.test", "example.test"),
    ] {
        let header = format!("a=1; Domain={domain}");
        assert_eq!(
            parse_set_cookie(&header, host, "/", NOW),
            Err(CookieError::Domain {
                domain: domain.into()
            }),
            "{domain} from {host}"
        );
    }
    // A single label is fine when it is the host itself.
    let c = parse_set_cookie("a=1; Domain=localhost", "localhost", "/", NOW).unwrap();
    assert_eq!(c.domain, "localhost");
    assert!(!c.host_only);
    // IP addresses only match themselves.
    assert!(parse_set_cookie("a=1; Domain=0.0.1", "127.0.0.1", "/", NOW).is_err());
    assert!(parse_set_cookie("a=1; Domain=127.0.0.1", "127.0.0.1", "/", NOW).is_ok());
}

#[test]
fn f17_ac1_headers_without_a_name_are_ignored() {
    for header in ["", "novalue", "=v", "  =v; Path=/", "; a=1"] {
        assert_eq!(
            parse_set_cookie(header, "h.test", "/", NOW),
            Err(CookieError::Syntax),
            "{header:?}"
        );
    }
    let c = parse_set_cookie("a=", "h.test", "/", NOW).unwrap();
    assert_eq!(c.value, "");
    assert_eq!(
        CookieError::Syntax.to_string(),
        "Enter a cookie as name=value; attributes"
    );
    assert_eq!(
        CookieError::Domain {
            domain: "x.test".into()
        }
        .to_string(),
        "The cookie's domain x.test does not match this host"
    );
}

#[test]
fn f17_ac1_same_cookie_replaces_and_keeps_creation_time() {
    let mut jar = Jar::default();
    stored(&mut jar, "a=1", "h.test", "/");
    jar.store("a=2", "h.test", "/", NOW + 10).unwrap();
    assert_eq!(jar.cookies.len(), 1);
    assert_eq!(jar.cookies[0].value, "2");
    assert_eq!(jar.cookies[0].created, NOW);
    // Different path or domain: a separate cookie.
    stored(&mut jar, "a=3; Path=/x", "h.test", "/");
    stored(&mut jar, "a=4", "other.test", "/");
    assert_eq!(jar.cookies.len(), 3);
}

#[test]
fn f17_ac1_expired_cookies_delete() {
    let mut jar = Jar::default();
    stored(&mut jar, "a=1", "h.test", "/");
    stored(
        &mut jar,
        "a=gone; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
        "h.test",
        "/",
    );
    assert!(jar.cookies.is_empty());
    stored(&mut jar, "b=never; Max-Age=0", "h.test", "/");
    assert!(
        jar.cookies.is_empty(),
        "an already expired cookie is not stored"
    );
}

#[test]
fn f17_ac1_store_reports_ignored_cookies() {
    let mut jar = Jar::default();
    assert_eq!(
        jar.store("bad", "h.test", "/", NOW),
        Err(CookieError::Syntax)
    );
    assert!(jar.cookies.is_empty());
}

// --- dates ----------------------------------------------------------------------------

#[test]
fn f17_ac1_cookie_dates() {
    let expected = 1_792_567_680_000; // 2026-10-21T07:28:00Z
    for text in [
        "Wed, 21 Oct 2026 07:28:00 GMT",
        "Wednesday, 21-Oct-26 07:28:00 GMT",
        "21 oct 2026 07:28:00",
        "Oct 21 07:28:00 2026",
        "Wed Oct 21 07:28:00 2026",
    ] {
        assert_eq!(parse_date(text), Some(expected), "{text}");
    }
    assert_eq!(parse_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(parse_date("01 Jan 69 00:00:00"), Some(3_124_224_000_000)); // 2069
    assert_eq!(parse_date("01 Jan 70 00:00:00"), Some(0)); // 1970
    for bad in [
        "",
        "tomorrow",
        "32 Oct 2026 00:00:00",
        "21 Oct 2026 24:00:00",
        "21 Oct 2026 07:60:00",
        "21 Oct 2026 07:28:60",
        "21 Oct 1600 00:00:00",
        "21 Foo 2026 00:00:00",
        "21 Oct 2026",
    ] {
        assert_eq!(parse_date(bad), None, "{bad}");
    }
}

// --- F17-AC2 sending ---------------------------------------------------------------

#[test]
fn f17_ac2_matching_by_domain_path_secure_and_expiry() {
    let mut jar = Jar::default();
    stored(&mut jar, "host=1", "example.test", "/");
    stored(&mut jar, "wide=2; Domain=example.test", "example.test", "/");
    stored(&mut jar, "api=3; Path=/api", "example.test", "/");
    stored(&mut jar, "sec=4; Secure", "example.test", "/");
    stored(&mut jar, "soon=5; Max-Age=3600", "example.test", "/");
    let header =
        |host: &str, path: &str, secure: bool, now: i64| jar.header_for(host, path, secure, now);
    assert_eq!(
        header("example.test", "/api/users", true, NOW).as_deref(),
        Some("api=3; host=1; wide=2; sec=4; soon=5")
    );
    assert_eq!(
        header("example.test", "/", false, NOW).as_deref(),
        Some("host=1; wide=2; soon=5")
    );
    assert_eq!(
        header("www.example.test", "/", false, NOW).as_deref(),
        Some("wide=2"),
        "host-only cookies stay on their host"
    );
    assert_eq!(
        header("example.test", "/apiary", false, NOW + 2 * HOUR).as_deref(),
        Some("host=1; wide=2"),
        "/api does not match /apiary, and soon has expired"
    );
    assert_eq!(header("other.test", "/", true, NOW), None);
    assert_eq!(header("notexample.test", "/", true, NOW), None);
}

#[test]
fn f17_ac2_path_matching() {
    let c = parse_set_cookie("a=1; Path=/docs/", "h.test", "/", NOW).unwrap();
    assert!(c.matches("h.test", "/docs/", false, NOW));
    assert!(c.matches("h.test", "/docs/x", false, NOW));
    assert!(!c.matches("h.test", "/docs", false, NOW));
    let c = parse_set_cookie("a=1; Path=/docs", "h.test", "/", NOW).unwrap();
    assert!(c.matches("h.test", "/docs", false, NOW));
    assert!(c.matches("h.test", "/docs/x", false, NOW));
    assert!(!c.matches("h.test", "/docsx", false, NOW));
}

#[test]
fn f17_ac2_order_is_longer_paths_then_older() {
    let mut jar = Jar::default();
    jar.store("late=1", "h.test", "/", NOW + 5).unwrap();
    jar.store("early=1", "h.test", "/", NOW).unwrap();
    jar.store("deep=1; Path=/a/b", "h.test", "/", NOW + 9)
        .unwrap();
    assert_eq!(
        jar.header_for("h.test", "/a/b/c", false, NOW + 10)
            .as_deref(),
        Some("deep=1; early=1; late=1")
    );
}

// --- F17-AC5 manager operations ----------------------------------------------------------

#[test]
fn f17_ac5_domains_listing_and_removal() {
    let mut jar = Jar::default();
    stored(&mut jar, "b=1", "zeta.test", "/");
    stored(&mut jar, "z=1", "alpha.test", "/");
    stored(&mut jar, "a=1", "alpha.test", "/");
    stored(&mut jar, "a=2; Path=/p", "alpha.test", "/");
    assert_eq!(jar.domains(), ["alpha.test", "zeta.test"]);
    let names: Vec<(&str, &str)> = jar
        .for_domain("alpha.test")
        .iter()
        .map(|c| (c.name.as_str(), c.path.as_str()))
        .collect();
    assert_eq!(names, [("a", "/"), ("a", "/p"), ("z", "/")]);
    assert!(jar.remove("alpha.test", "a", "/p"));
    assert!(!jar.remove("alpha.test", "a", "/p"));
    jar.remove_domain("alpha.test");
    assert_eq!(jar.domains(), ["zeta.test"]);
}

#[test]
fn f17_ac5_set_cookie_text_round_trips() {
    let c = parse_set_cookie(
        "sid=x y; Path=/app; Domain=example.test; Max-Age=3600; Secure; HttpOnly; SameSite=Strict",
        "example.test",
        "/",
        NOW,
    )
    .unwrap();
    let text = c.to_set_cookie();
    assert_eq!(
        text,
        "sid=x y; Path=/app; Domain=example.test; Expires=Sat, 17 Oct 2026 20:07:20 GMT; Secure; HttpOnly; SameSite=Strict"
    );
    let back = parse_set_cookie(&text, "example.test", "/", NOW).unwrap();
    assert_eq!(back, c);
    let host_only = parse_set_cookie("a=1", "h.test", "/", NOW).unwrap();
    assert_eq!(host_only.to_set_cookie(), "a=1; Path=/");
}

// --- F17-AC6 persistence ------------------------------------------------------------------

#[test]
fn f17_ac6_format_drops_expired_cookies() {
    let mut jar = Jar::default();
    stored(&mut jar, "keep=1", "h.test", "/");
    stored(&mut jar, "brief=1; Max-Age=60", "h.test", "/");
    let text = cookies::write(&jar);
    assert_eq!(cookies::parse(&text, NOW).unwrap(), jar);
    let later = cookies::parse(&text, NOW + 61_000).unwrap();
    assert_eq!(later.cookies.len(), 1);
    assert_eq!(later.cookies[0].name, "keep");
    let mut purged = jar.clone();
    purged.purge(NOW + 61_000);
    assert_eq!(purged, later);
    assert_eq!(cookies::parse("", NOW).unwrap(), Jar::default());
    assert!(cookies::parse("cookies = 1", NOW).is_err());
}

proptest! {
    #[test]
    fn f17_ac1_prop_parsing_never_panics(header in "\\PC{0,60}", host in "[a-z.]{0,12}", path in "[a-z/]{0,10}") {
        let _ = parse_set_cookie(&header, &host, &path, NOW);
        let _ = parse_date(&header);
    }

    #[test]
    fn f17_ac2_prop_stored_cookies_match_their_own_host_and_path(name in "[a-z]{1,6}", value in "[a-z0-9]{0,6}", path in "(/[a-z]{1,4}){0,3}") {
        let mut jar = Jar::default();
        let header = format!("{name}={value}; Path={}", if path.is_empty() { "/" } else { &path });
        jar.store(&header, "h.test", "/", NOW).unwrap();
        let request_path = if path.is_empty() { "/".to_owned() } else { path.clone() };
        let sent = jar.header_for("h.test", &request_path, false, NOW).unwrap();
        prop_assert_eq!(sent, format!("{name}={value}"));
    }
}

#[test]
fn f17_ac1_cookie_struct_is_plain_data() {
    let c = Cookie {
        name: "n".into(),
        value: "v".into(),
        domain: "d.test".into(),
        host_only: true,
        path: "/".into(),
        expires: Some(NOW),
        secure: false,
        http_only: false,
        same_site: None,
        created: NOW,
    };
    assert!(c.is_expired(NOW), "expiring now means expired");
    assert!(!c.is_expired(NOW - 1));
}
