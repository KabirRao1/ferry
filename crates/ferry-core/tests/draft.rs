//! Building a request from a draft: F02-AC1..AC3, F03-AC1..AC6, F04-AC1..AC5,
//! F05-AC4/AC5 options, F07-AC5 session format.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;

use bytes::Bytes;
use ferry_core::auth::{ApiKeyLocation, AuthDraft, AuthKind};
use ferry_core::body::{BodyDraft, BodyMode, FormField, RawLanguage, guess_content_type};
use ferry_core::draft::{AutoHeader, BuildError, CALCULATED, RequestDraft, USER_AGENT};
use ferry_core::kv::KeyValue;
use ferry_core::params::Param;
use ferry_core::request::{Method, Part, PartValue, RequestBody, SendOptions, UrlError};
use ferry_core::response::Header;
use ferry_core::session::Session;

fn draft(url: &str) -> RequestDraft {
    RequestDraft {
        url: url.into(),
        ..RequestDraft::default()
    }
}

fn headers(d: &RequestDraft) -> Vec<(String, String)> {
    d.build()
        .unwrap()
        .headers
        .into_iter()
        .map(|Header { name, value }| (name, value))
        .collect()
}

fn h(name: &str, value: &str) -> (String, String) {
    (name.to_owned(), value.to_owned())
}

fn off(key: &str, value: &str) -> KeyValue {
    KeyValue {
        enabled: false,
        ..KeyValue::new(key, value)
    }
}

// --- F02 headers ---------------------------------------------------------

#[test]
fn f02_ac1_enabled_rows_sent_in_order_with_duplicates() {
    let mut d = draft("http://a.test/");
    d.headers = vec![
        KeyValue::new("X-B", "2"),
        off("X-Off", "no"),
        KeyValue::new("  ", "blank key"),
        KeyValue::new("X-A", "1"),
        KeyValue::new(" X-B ", "3"),
    ];
    assert_eq!(headers(&d), [h("X-B", "2"), h("X-A", "1"), h("X-B", "3")]);
}

#[test]
fn f02_ac2_invalid_header_name_blocks_send() {
    let mut d = draft("http://a.test/");
    d.headers = vec![KeyValue::new("Bad Name", "v")];
    let e = d.build().unwrap_err();
    assert_eq!(
        e,
        BuildError::HeaderName {
            name: "Bad Name".into(),
            invalid: ' '
        }
    );
    assert_eq!(
        e.to_string(),
        "Header name \"Bad Name\" contains invalid character ' '"
    );
}

#[test]
fn f02_ac2_line_breaks_and_control_characters_in_values_block_send() {
    let mut d = draft("http://a.test/");
    d.headers = vec![KeyValue::new("X", "a\r\nInjected: 1")];
    assert_eq!(
        d.build().unwrap_err().to_string(),
        "Header \"X\" value cannot contain line breaks"
    );
    d.headers = vec![KeyValue::new("X", "a\u{0}b")];
    assert_eq!(
        d.build().unwrap_err().to_string(),
        "Header \"X\" value contains invalid character \\u{0}"
    );
    // Tabs and non-ASCII are fine.
    d.headers = vec![KeyValue::new("X", "a\tb é")];
    assert!(d.build().is_ok());
    // Disabled rows are not validated.
    d.headers = vec![off("Bad Name", "\n")];
    assert!(d.build().is_ok());
}

#[test]
fn f02_ac3_auto_headers_listed_in_order() {
    let mut d = draft("http://a.test/");
    let names = |d: &RequestDraft| -> Vec<String> {
        d.auto_headers().into_iter().map(|a| a.name).collect()
    };
    assert_eq!(
        names(&d),
        ["Host", "User-Agent", "Accept", "Accept-Encoding"]
    );
    d.body.mode = BodyMode::Raw;
    d.body.raw = "{}".into();
    d.body.language = RawLanguage::Json;
    d.auth.kind = AuthKind::Bearer;
    d.auth.token = "t".into();
    assert_eq!(
        d.auto_headers(),
        [
            auto("Content-Type", "application/json", false),
            auto("Authorization", "Bearer t", false),
            auto("Content-Length", CALCULATED, false),
            auto("Host", CALCULATED, false),
            auto("User-Agent", USER_AGENT, false),
            auto("Accept", "*/*", false),
            auto("Accept-Encoding", "gzip, deflate, br, zstd", false),
        ]
    );
    assert!(USER_AGENT.starts_with("ferry/"));
}

fn auto(name: &str, value: &str, overridden: bool) -> AutoHeader {
    AutoHeader {
        name: name.into(),
        value: value.into(),
        overridden,
    }
}

#[test]
fn f02_ac3_user_header_overrides_auto_header_case_insensitively() {
    let mut d = draft("http://a.test/");
    d.body.mode = BodyMode::Raw;
    d.body.raw = "x".into();
    d.headers = vec![
        KeyValue::new("content-type", "text/csv"),
        KeyValue::new("user-agent", "curl/8"),
        off("Accept", "never"),
    ];
    let autos = d.auto_headers();
    let overridden: Vec<&str> = autos
        .iter()
        .filter(|a| a.overridden)
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(overridden, ["Content-Type", "User-Agent"]);
    assert_eq!(
        headers(&d),
        [h("content-type", "text/csv"), h("user-agent", "curl/8")]
    );
}

// --- F03 body --------------------------------------------------------------

fn body_of(d: &RequestDraft) -> RequestBody {
    d.build().unwrap().body
}

#[test]
fn f03_ac1_each_mode_keeps_content_and_only_selected_is_sent() {
    let mut d = draft("http://a.test/");
    d.body = BodyDraft {
        mode: BodyMode::None,
        raw: "raw text".into(),
        language: RawLanguage::Text,
        urlencoded: vec![KeyValue::new("a", "1")],
        form: vec![FormField {
            row: KeyValue::new("f", "v"),
            file: None,
        }],
        binary: Some(PathBuf::from("data.bin")),
    };
    assert_eq!(body_of(&d), RequestBody::Empty);
    assert_eq!(headers(&d), []);
    d.body.mode = BodyMode::Raw;
    assert_eq!(body_of(&d), RequestBody::Bytes(Bytes::from("raw text")));
    d.body.mode = BodyMode::UrlEncoded;
    assert_eq!(body_of(&d), RequestBody::Bytes(Bytes::from("a=1")));
    d.body.mode = BodyMode::FormData;
    assert_eq!(
        body_of(&d),
        RequestBody::Multipart(vec![Part {
            name: "f".into(),
            value: PartValue::Text("v".into())
        }])
    );
    d.body.mode = BodyMode::Binary;
    assert_eq!(body_of(&d), RequestBody::File(PathBuf::from("data.bin")));
    assert_eq!(d.body.raw, "raw text");
}

#[test]
fn f03_ac2_raw_sent_verbatim_with_language_content_type() {
    let mut d = draft("http://a.test/");
    d.body.mode = BodyMode::Raw;
    d.body.raw = "  {\"é\": 1}\r\n".into();
    let cases = [
        (RawLanguage::Text, "text/plain"),
        (RawLanguage::Json, "application/json"),
        (RawLanguage::Xml, "application/xml"),
        (RawLanguage::Html, "text/html"),
        (RawLanguage::JavaScript, "application/javascript"),
    ];
    for (language, content_type) in cases {
        d.body.language = language;
        assert_eq!(headers(&d), [h("Content-Type", content_type)]);
        assert_eq!(
            body_of(&d),
            RequestBody::Bytes(Bytes::from("  {\"é\": 1}\r\n"))
        );
    }
    d.body.raw.clear();
    assert_eq!(body_of(&d), RequestBody::Empty);
    assert_eq!(headers(&d), []);
}

#[test]
fn f03_ac4_urlencoded_encodes_like_html_forms() {
    let mut d = draft("http://a.test/");
    d.body.mode = BodyMode::UrlEncoded;
    d.body.urlencoded = vec![
        KeyValue::new("name", "Ferry App"),
        off("skip", "me"),
        KeyValue::new("sym", "a&b=c+d/é"),
        KeyValue::new("name", "dup"),
        KeyValue::new("", "no key"),
    ];
    assert_eq!(
        body_of(&d),
        RequestBody::Bytes(Bytes::from(
            "name=Ferry+App&sym=a%26b%3Dc%2Bd%2F%C3%A9&name=dup"
        ))
    );
    assert_eq!(
        headers(&d),
        [h("Content-Type", "application/x-www-form-urlencoded")]
    );
}

#[test]
fn f03_ac5_form_data_text_and_file_parts_in_order() {
    let mut d = draft("http://a.test/");
    d.body.mode = BodyMode::FormData;
    d.body.form = vec![
        FormField {
            row: KeyValue::new("title", "hello"),
            file: None,
        },
        FormField {
            row: off("off", "x"),
            file: None,
        },
        FormField {
            row: KeyValue::new("upload", "ignored"),
            file: Some(PathBuf::from("photo.png")),
        },
    ];
    assert_eq!(
        body_of(&d),
        RequestBody::Multipart(vec![
            Part {
                name: "title".into(),
                value: PartValue::Text("hello".into())
            },
            Part {
                name: "upload".into(),
                value: PartValue::File(PathBuf::from("photo.png"))
            },
        ])
    );
    // The transport adds Content-Type with the boundary.
    assert_eq!(headers(&d), []);
    assert_eq!(
        d.body.content_type().unwrap(),
        "multipart/form-data; boundary=<calculated when request is sent>"
    );
}

#[test]
fn f03_ac6_binary_file_with_guessed_content_type() {
    let mut d = draft("http://a.test/");
    d.body.mode = BodyMode::Binary;
    assert_eq!(body_of(&d), RequestBody::Empty);
    d.body.binary = Some(PathBuf::from("report.pdf"));
    assert_eq!(body_of(&d), RequestBody::File(PathBuf::from("report.pdf")));
    assert_eq!(headers(&d), [h("Content-Type", "application/pdf")]);
    assert_eq!(guess_content_type("x.json".as_ref()), "application/json");
    assert_eq!(
        guess_content_type("noext".as_ref()),
        "application/octet-stream"
    );
}

// --- F04 auth ----------------------------------------------------------------

#[test]
fn f04_ac1_switching_type_keeps_fields_and_only_selected_applies() {
    let mut d = draft("http://a.test/");
    d.auth = AuthDraft {
        kind: AuthKind::None,
        username: "u".into(),
        password: "p".into(),
        token: "tok".into(),
        key: "X-Key".into(),
        value: "k".into(),
        location: ApiKeyLocation::Header,
    };
    assert_eq!(headers(&d), []);
    d.auth.kind = AuthKind::Bearer;
    assert_eq!(headers(&d), [h("Authorization", "Bearer tok")]);
    d.auth.kind = AuthKind::ApiKey;
    assert_eq!(headers(&d), [h("X-Key", "k")]);
    assert_eq!(d.auth.username, "u");
}

#[test]
fn f04_ac2_basic_auth_is_base64_of_utf8_credentials() {
    let mut d = draft("http://a.test/");
    d.auth.kind = AuthKind::Basic;
    d.auth.username = "Aladdin".into();
    d.auth.password = "open sesame".into();
    assert_eq!(
        headers(&d),
        [h("Authorization", "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==")]
    );
    d.auth.username = "josé".into();
    d.auth.password = String::new();
    assert_eq!(headers(&d), [h("Authorization", "Basic am9zw6k6")]);
}

#[test]
fn f04_ac3_empty_bearer_token_sends_nothing() {
    let mut d = draft("http://a.test/");
    d.auth.kind = AuthKind::Bearer;
    assert_eq!(headers(&d), []);
}

#[test]
fn f04_ac4_api_key_in_header_or_query() {
    let mut d = draft("http://a.test/p?x=1");
    d.auth.kind = AuthKind::ApiKey;
    d.auth.value = "v 1&2".into();
    assert_eq!(headers(&d), []);
    d.auth.key = "api key".into();
    d.auth.location = ApiKeyLocation::Query;
    let r = d.build().unwrap();
    assert_eq!(r.url.as_str(), "http://a.test/p?x=1&api+key=v+1%262");
    assert!(r.headers.is_empty());
    d.auth.location = ApiKeyLocation::Header;
    // A header needs a valid name.
    assert_eq!(
        d.build().unwrap_err().to_string(),
        "Header name \"api key\" contains invalid character ' '"
    );
    d.auth.key = "X-Api-Key".into();
    assert_eq!(headers(&d), [h("X-Api-Key", "v 1&2")]);
}

#[test]
fn f04_ac5_user_header_wins_over_auth() {
    let mut d = draft("http://a.test/");
    d.auth.kind = AuthKind::Bearer;
    d.auth.token = "from-auth".into();
    d.headers = vec![KeyValue::new("authorization", "Bearer manual")];
    assert_eq!(headers(&d), [h("authorization", "Bearer manual")]);
    assert!(
        d.auto_headers()
            .iter()
            .any(|a| a.name == "Authorization" && a.overridden)
    );
}

// --- URL, method, options ------------------------------------------------------

#[test]
fn f01_ac5_invalid_url_blocks_build() {
    assert_eq!(
        draft("example.com").build().unwrap_err(),
        BuildError::Url(UrlError)
    );
    assert_eq!(
        draft("").build().unwrap_err().to_string(),
        "Enter a URL starting with http:// or https://"
    );
}

#[test]
fn f05_ac4_ac5_options_and_method_carried_over() {
    let mut d = draft("http://a.test/");
    d.method = Method::Custom("PURGE".into());
    d.options = SendOptions {
        follow_redirects: false,
        verify_tls: false,
        cookies: true,
    };
    let r = d.build().unwrap();
    assert_eq!(r.method.as_str(), "PURGE");
    assert!(!r.options.follow_redirects && !r.options.verify_tls);
    assert_eq!(
        SendOptions::default(),
        SendOptions {
            follow_redirects: true,
            verify_tls: true,
            // Added by F17-AC3 (Phase 3).
            cookies: true
        }
    );
}

// --- F07-AC5 session format ---------------------------------------------------

#[test]
fn f07_ac5_session_roundtrips_through_toml() {
    let mut d = draft("http://a.test/?q=1");
    d.method = Method::Custom("PROPFIND".into());
    d.params = vec![Param::new("q", "1")];
    d.headers = vec![off("X", "y")];
    d.body.mode = BodyMode::FormData;
    d.body.form = vec![FormField {
        row: KeyValue::new("f", ""),
        file: Some(PathBuf::from("a b.txt")),
    }];
    d.auth.kind = AuthKind::ApiKey;
    d.auth.location = ApiKeyLocation::Query;
    let session = Session {
        tabs: vec![d.into(), RequestDraft::default().into()],
        active: 1,
        sidebar_open: false,
        split: 0.3,
        ..Session::default()
    };
    let text = toml::to_string(&session).unwrap();
    let back: Session = toml::from_str(&text).unwrap();
    assert_eq!(back, session);
    // Missing fields fall back to defaults.
    let minimal: Session = toml::from_str("[[tabs]]\nurl = \"http://x.test\"\n").unwrap();
    assert_eq!(minimal.tabs[0].draft.method, Method::Get);
    assert!(minimal.tabs[0].draft.options.follow_redirects);
    assert_eq!(minimal.tabs[0].link, None);
    assert_eq!(minimal.version, Session::VERSION);
    // An invalid method is an error, not a silent default.
    assert!(toml::from_str::<Session>("[[tabs]]\nmethod = \"BAD METHOD\"\n").is_err());
}

// --- labels shown in menus -------------------------------------------------------

#[test]
fn f03_f04_menu_labels_match_postman() {
    let modes: Vec<String> = BodyMode::ALL.iter().map(ToString::to_string).collect();
    assert_eq!(
        modes,
        [
            "none",
            "form-data",
            "x-www-form-urlencoded",
            "raw",
            "binary"
        ]
    );
    assert_eq!(BodyMode::Raw.label(), "raw");
    let languages: Vec<String> = RawLanguage::ALL.iter().map(ToString::to_string).collect();
    assert_eq!(languages, ["Text", "JavaScript", "JSON", "HTML", "XML"]);
    let kinds: Vec<String> = AuthKind::ALL.iter().map(ToString::to_string).collect();
    // Inherit was added first in the menu by F04-AC6 (Phase 3), as in Postman.
    assert_eq!(
        kinds,
        [
            "Inherit auth from parent",
            "No Auth",
            "Basic Auth",
            "Bearer Token",
            "API Key"
        ]
    );
    let places: Vec<String> = ApiKeyLocation::ALL
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(places, ["Header", "Query Params"]);
}

#[test]
fn f01_ac8_params_compare_by_visible_content() {
    let mut a = Param::new("k", "v");
    let mut b = Param::new("k", "v");
    a.original = Some("k=v".into());
    assert_eq!(a, b, "bookkeeping is ignored");
    b.row.value = "w".into();
    assert_ne!(a, b);
    b.row.value = "v".into();
    b.row.enabled = false;
    assert_ne!(a, b);
}
