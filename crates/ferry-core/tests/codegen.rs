//! Code snippets (F19-AC2): every language for four kinds of request.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::auth::{AuthDraft, AuthKind};
use ferry_core::body::{BodyMode, FormField, RawLanguage};
use ferry_core::codegen::{Language, Snippet, SnippetBody, snippet};
use ferry_core::draft::RequestDraft;
use ferry_core::kv::KeyValue;
use ferry_core::request::Method;

/// POST JSON with a header and body that need escaping, and bearer auth.
fn json() -> RequestDraft {
    let mut d = RequestDraft {
        method: Method::Post,
        url: "https://api.test/users?tag=a b".into(),
        headers: vec![KeyValue::new("X-Note", "it's \"quoted\" $HOME #{x} \\ ok")],
        auth: AuthDraft {
            kind: AuthKind::Bearer,
            token: "t0k".into(),
            ..AuthDraft::default()
        },
        ..RequestDraft::default()
    };
    d.body.mode = BodyMode::Raw;
    d.body.language = RawLanguage::Json;
    d.body.raw = "{\n  \"name\": \"Ada \\\"L\\\"\",\n  \"tab\": \"\t\"\n}".into();
    d
}

fn urlencoded() -> RequestDraft {
    let mut d = RequestDraft {
        method: Method::Put,
        url: "https://api.test/form".into(),
        ..RequestDraft::default()
    };
    d.body.mode = BodyMode::UrlEncoded;
    d.body.urlencoded = vec![KeyValue::new("q", "a b&c"), KeyValue::new("n", "1")];
    d
}

fn multipart() -> RequestDraft {
    let mut d = RequestDraft {
        method: Method::Post,
        url: "https://api.test/upload".into(),
        ..RequestDraft::default()
    };
    d.body.mode = BodyMode::FormData;
    d.body.form = vec![
        FormField {
            row: KeyValue::new("title", "Holiday"),
            file: None,
        },
        FormField {
            row: KeyValue::new("photo", ""),
            file: Some("/home/ada/pics/beach.jpg".into()),
        },
    ];
    d
}

/// A binary file body, redirects not followed, verification off.
fn binary() -> RequestDraft {
    let mut d = RequestDraft {
        method: Method::parse("PURGE").unwrap(),
        url: "https://api.test/cache".into(),
        ..RequestDraft::default()
    };
    d.body.mode = BodyMode::Binary;
    d.body.binary = Some("data.bin".into());
    d.options.follow_redirects = false;
    d.options.verify_tls = false;
    d
}

fn snip(d: &RequestDraft) -> Snippet {
    snippet(&d.build().unwrap(), d)
}

#[test]
fn f19_ac2_snippets() {
    for (name, d) in [
        ("json", json()),
        ("urlencoded", urlencoded()),
        ("multipart", multipart()),
        ("binary", binary()),
    ] {
        let s = snip(&d);
        for lang in Language::ALL {
            let slug = lang
                .label()
                .to_lowercase()
                .replace([' ', '#', '/', ':', '.'], "_")
                .replace("__", "_");
            insta::assert_snapshot!(format!("{slug}-{name}"), lang.generate(&s));
        }
    }
}

#[test]
fn f19_ac2_snippet_input() {
    let s = snip(&json());
    assert_eq!(s.method, "POST");
    assert_eq!(s.url, "https://api.test/users?tag=a%20b");
    let names: Vec<_> = s.headers.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["X-Note", "Content-Type", "Authorization"]);
    assert!(matches!(s.body, SnippetBody::Raw(_)));
    assert!(matches!(snip(&urlencoded()).body, SnippetBody::UrlEncoded(ref f) if f.len() == 2));
    assert!(matches!(snip(&multipart()).body, SnippetBody::FormData(ref p) if p.len() == 2));
    let b = snip(&binary());
    assert!(matches!(b.body, SnippetBody::File(ref p) if p == "data.bin"));
    assert!(!b.follow_redirects && !b.verify_tls);
    assert_eq!(Language::ALL.len(), 12);
    assert_eq!(Language::default(), Language::Curl);
    assert_eq!(Language::Swift.to_string(), "Swift - URLSession");
}
