//! cURL commands in (F11-AC1) and out (F19-AC3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::auth::AuthKind;
use ferry_core::body::{BodyMode, FormField, RawLanguage};
use ferry_core::codegen::{Language, snippet};
use ferry_core::curl::{is_curl, parse};
use ferry_core::draft::RequestDraft;
use ferry_core::kv::KeyValue;
use ferry_core::request::Method;
use proptest::prelude::*;

fn header(d: &RequestDraft, name: &str) -> Option<String> {
    d.headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
}

#[test]
fn f11_ac1_chrome_style_command() {
    let d = parse(
        "curl 'https://api.test/items?page=2' \\\n  -H 'accept: application/json' \\\n  -H 'authorization: Bearer abc' \\\n  --data-raw '{\"name\":\"x\"}' \\\n  --compressed",
    )
    .unwrap();
    assert_eq!(d.method, Method::Post, "data implies POST");
    assert_eq!(d.url, "https://api.test/items?page=2");
    assert_eq!(d.params[0].row.key, "page");
    assert_eq!(header(&d, "authorization").as_deref(), Some("Bearer abc"));
    assert_eq!(d.body.mode, BodyMode::Raw);
    assert_eq!(d.body.raw, "{\"name\":\"x\"}");
    assert_eq!(d.body.language, RawLanguage::Json);
    assert!(!d.options.follow_redirects, "no -L");
}

#[test]
fn f11_ac1_options() {
    let d = parse(
        "curl -sSLk -X PUT --url https://a.test -u ada:pw -A agent -e https://r.test -b 'a=1; b=2'",
    )
    .unwrap();
    assert_eq!(d.method, Method::Put);
    assert_eq!(d.url, "https://a.test");
    assert!(d.options.follow_redirects);
    assert!(!d.options.verify_tls);
    assert_eq!(d.auth.kind, AuthKind::Basic);
    assert_eq!(
        (d.auth.username.as_str(), d.auth.password.as_str()),
        ("ada", "pw")
    );
    assert_eq!(header(&d, "User-Agent").as_deref(), Some("agent"));
    assert_eq!(header(&d, "Referer").as_deref(), Some("https://r.test"));
    assert_eq!(header(&d, "Cookie").as_deref(), Some("a=1; b=2"));
    // Attached values and -I.
    let d = parse("curl -XDELETE -HX-A:1 https://a.test").unwrap();
    assert_eq!(d.method, Method::Delete);
    assert_eq!(header(&d, "X-A").as_deref(), Some("1"));
    assert_eq!(
        parse("curl -I https://a.test").unwrap().method,
        Method::Head
    );
    // `Name;` is an empty header; `Name:` removes one.
    let d = parse("curl -H 'X-Empty;' -H 'X-Gone:' https://a.test").unwrap();
    assert_eq!(header(&d, "X-Empty").as_deref(), Some(""));
    assert_eq!(header(&d, "X-Gone"), None);
    // Options with values never become the URL.
    let d = parse("curl -o out.json --max-time 5 https://a.test").unwrap();
    assert_eq!(d.url, "https://a.test");
}

#[test]
fn f11_ac1_data_forms() {
    let d = parse("curl https://a.test -d 'a=1&b=two%20words' -d c=3").unwrap();
    assert_eq!(d.body.mode, BodyMode::UrlEncoded);
    let fields: Vec<_> = d
        .body
        .urlencoded
        .iter()
        .map(|f| (f.key.as_str(), f.value.as_str()))
        .collect();
    assert_eq!(fields, [("a", "1"), ("b", "two words"), ("c", "3")]);
    let d = parse("curl https://a.test --data-urlencode 'q=a b&c' --data-urlencode '=x'").unwrap();
    let fields: Vec<_> = d
        .body
        .urlencoded
        .iter()
        .map(|f| (f.key.as_str(), f.value.as_str()))
        .collect();
    assert_eq!(fields, [("q", "a b&c"), ("", "x")]);
    // A Content-Type that is not urlencoded keeps the text raw.
    let d = parse("curl https://a.test -H 'Content-Type: text/plain' -d 'a=b'").unwrap();
    assert_eq!((d.body.mode, d.body.raw.as_str()), (BodyMode::Raw, "a=b"));
    let d = parse("curl https://a.test -d 'not a form'").unwrap();
    assert_eq!(d.body.mode, BodyMode::Raw);
    // --json sets the JSON headers unless given.
    let d = parse("curl https://a.test --json '{\"a\":1}'").unwrap();
    assert_eq!(d.body.language, RawLanguage::Json);
    assert_eq!(
        header(&d, "Content-Type").as_deref(),
        Some("application/json")
    );
    assert_eq!(header(&d, "Accept").as_deref(), Some("application/json"));
    // -F text and file fields.
    let d = parse("curl https://a.test -F 'name=ada' -F 'pic=@/tmp/a b.png;type=image/png' --form-string 'raw=@not-a-file'").unwrap();
    assert_eq!(d.body.mode, BodyMode::FormData);
    assert_eq!(
        d.body.form,
        [
            FormField {
                row: KeyValue::new("name", "ada"),
                file: None
            },
            FormField {
                row: KeyValue::new("pic", ""),
                file: Some("/tmp/a b.png".into())
            },
            FormField {
                row: KeyValue::new("raw", "@not-a-file"),
                file: None
            },
        ]
    );
    // A file as the whole body.
    let d = parse("curl https://a.test --data-binary @data.bin").unwrap();
    assert_eq!(d.body.mode, BodyMode::Binary);
    assert_eq!(
        d.body.binary.as_deref(),
        Some(std::path::Path::new("data.bin"))
    );
    // -G moves the data into the query.
    let d = parse("curl -G https://a.test/s -d q=1 --data-urlencode 'w=a b'").unwrap();
    assert_eq!(d.method, Method::Get);
    assert_eq!(d.url, "https://a.test/s?q=1&w=a%20b");
    assert_eq!(d.body.mode, BodyMode::None);
}

#[test]
fn f11_ac1_quoting_styles() {
    let d = parse("curl $'https://a.test/\\x41' -H \"X-Q: \\\"q\\\" \\$HOME\" -d $'line1\\nline2'")
        .unwrap();
    assert_eq!(d.url, "https://a.test/A");
    assert_eq!(header(&d, "X-Q").as_deref(), Some("\"q\" $HOME"));
    assert_eq!(d.body.raw, "line1\nline2");
    // Windows cmd: carets escape and continue lines.
    let d = parse("curl ^\"https://a.test/x^\" ^\r\n  -H ^\"A: b^\" ^\r\n  --data-raw ^\"^{^\\^\"k^\\^\":1^}^\"").unwrap();
    assert_eq!(d.url, "https://a.test/x");
    assert_eq!(header(&d, "A").as_deref(), Some("b"));
    assert_eq!(d.body.raw, "{\"k\":1}");
}

#[test]
fn f11_ac1_errors() {
    assert!(is_curl("  curl https://a"));
    assert!(!is_curl("curly https://a"));
    for (text, reason) in [
        (
            "wget https://a",
            "Could not read the cURL command: it does not start with curl",
        ),
        (
            "curl 'https://a",
            "Could not read the cURL command: a quote is not closed",
        ),
        (
            "curl -H 'A: b'",
            "Could not read the cURL command: it names no URL",
        ),
        (
            "curl https://a -H",
            "Could not read the cURL command: -H needs a value",
        ),
    ] {
        assert_eq!(parse(text).unwrap_err().to_string(), reason, "{text}");
    }
}

// --- F19-AC3 round trip ------------------------------------------------------------

fn token() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9-]{0,8}"
}

fn text() -> impl Strategy<Value = String> {
    // Printable, no CR or LF in headers; no leading or trailing spaces.
    "[!-~]([ -~]{0,10}[!-~])?"
}

fn draft() -> impl Strategy<Value = RequestDraft> {
    let body = prop_oneof![Just(0u8), Just(1u8), Just(2u8), Just(3u8), Just(4u8),];
    (
        prop::sample::select(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "PURGE"]),
        "(/[a-z0-9]{1,5}){0,3}",
        prop::collection::vec((token(), prop_oneof![text(), Just(String::new())]), 0..4),
        body,
        "\\PC{0,30}",
        prop::collection::vec((token(), text()), 1..3),
        any::<(bool, bool)>(),
    )
        .prop_map(
            |(method, path, headers, kind, raw, fields, (follow, verify))| {
                let mut d = RequestDraft {
                    method: Method::parse(method).unwrap(),
                    url: format!("https://api.test{path}"),
                    headers: headers
                        .into_iter()
                        .map(|(k, v)| KeyValue::new(k, v))
                        .collect(),
                    ..RequestDraft::default()
                };
                d.options.follow_redirects = follow;
                d.options.verify_tls = verify;
                match kind {
                    1 => {
                        d.body.mode = BodyMode::Raw;
                        d.body.raw = raw;
                        d.body.language = RawLanguage::Json;
                    }
                    2 => {
                        d.body.mode = BodyMode::UrlEncoded;
                        d.body.urlencoded = fields
                            .into_iter()
                            .map(|(k, v)| KeyValue::new(k, v))
                            .collect();
                    }
                    3 => {
                        d.body.mode = BodyMode::FormData;
                        d.body.form = fields
                            .into_iter()
                            .enumerate()
                            .map(|(i, (k, v))| FormField {
                                row: KeyValue::new(
                                    k,
                                    if i == 0 { String::new() } else { v.clone() },
                                ),
                                file: (i == 0).then(|| {
                                    format!("/data/{v}.bin").replace([';', '"'], "_").into()
                                }),
                            })
                            .collect();
                    }
                    4 => {
                        d.body.mode = BodyMode::Binary;
                        d.body.binary = Some("/data/file.bin".into());
                    }
                    _ => {}
                }
                d
            },
        )
}

proptest! {
    #[test]
    fn f19_ac3_prop_curl_output_reads_back_as_the_same_request(d in draft()) {
        let Ok(built) = d.build() else { return Ok(()); };
        let command = Language::Curl.generate(&snippet(&built, &d));
        let back = parse(&command).map_err(|e| TestCaseError::fail(format!("{e}\n{command}")))?;
        let rebuilt = back.build().map_err(|e| TestCaseError::fail(format!("{e}\n{command}")))?;
        prop_assert_eq!(&rebuilt.method, &built.method, "{}", command);
        prop_assert_eq!(&rebuilt.url, &built.url, "{}", command);
        prop_assert_eq!(&rebuilt.headers, &built.headers, "{}", command);
        prop_assert_eq!(&rebuilt.body, &built.body, "{}", command);
        prop_assert_eq!(&rebuilt.options, &built.options, "{}", command);
    }

    #[test]
    fn f11_ac1_prop_parsing_never_panics(rest in "\\PC{0,80}") {
        let _ = parse(&format!("curl {rest}"));
    }
}
