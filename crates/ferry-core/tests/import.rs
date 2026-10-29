//! Importing and exporting Postman collections and environments
//! (F11-AC2..AC6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;

use ferry_core::auth::{ApiKeyLocation, AuthDraft, AuthKind};
use ferry_core::body::{BodyDraft, BodyMode, FormField, RawLanguage};
use ferry_core::collection::{CollectionFile, Example, FolderFile, RequestFile};
use ferry_core::draft::RequestDraft;
use ferry_core::import::postman::{export_collection, export_environment};
use ferry_core::import::{Collection, Entry, Import, NOT_IMPORTABLE, import};
use ferry_core::kv::KeyValue;
use ferry_core::params;
use ferry_core::request::{Method, SendOptions};
use ferry_core::response::Header;
use ferry_core::scripts::Scripts;
use ferry_core::variables::Variable;
use proptest::prelude::*;

const PAYMENTS: &str = include_str!("fixtures/postman/payments.postman_collection.json");

fn collection(text: &str) -> (Collection, Vec<String>) {
    let imported = import(text).unwrap();
    match imported.import {
        Import::Collection(c) => (c, imported.skipped),
        other => panic!("not a collection: {other:?}"),
    }
}

fn folder(entry: &Entry) -> (&FolderFile, &[Entry]) {
    match entry {
        Entry::Folder { file, items } => (file, items),
        Entry::Request(r) => panic!("not a folder: {}", r.name),
    }
}

fn request(entry: &Entry) -> &RequestFile {
    match entry {
        Entry::Request(r) => r,
        Entry::Folder { file, .. } => panic!("not a request: {}", file.name),
    }
}

fn row(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

fn off(mut kv: KeyValue) -> KeyValue {
    kv.enabled = false;
    kv
}

fn described(mut kv: KeyValue, description: &str) -> KeyValue {
    kv.description = description.into();
    kv
}

#[test]
fn f11_ac3_postman_collection() {
    let (c, _) = collection(PAYMENTS);
    assert_eq!(c.file.name, "Payments API");
    assert_eq!(c.file.description, "Charges, refunds and customers.");
    assert_eq!(c.file.auth.kind, AuthKind::Bearer);
    assert_eq!(c.file.auth.token, "{{token}}");
    assert_eq!(
        c.file.scripts,
        Scripts {
            pre_request: "console.log(\"start\");".into(),
            post_response: String::new(),
        }
    );
    let vars: Vec<(&str, &str, bool, bool)> = c
        .file
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled, v.secret))
        .collect();
    assert_eq!(
        vars,
        [
            ("baseUrl", "https://api.pay.test", true, false),
            ("apiKey", "sk_live", true, true),
            ("old", "x", false, false),
        ]
    );
    assert_eq!(c.counts(), (7, 2));
    let names: Vec<&str> = c
        .items
        .iter()
        .map(|e| match e {
            Entry::Folder { file, .. } => file.name.as_str(),
            Entry::Request(r) => r.name.as_str(),
        })
        .collect();
    assert_eq!(names, ["Auth", "Charges", "Charge graph", "Health"]);

    // A folder with its own auth, and a request with a disabled header,
    // a JSON body, a description, a test script and an example.
    let (auth, items) = folder(&c.items[0]);
    assert_eq!(
        (
            auth.auth.kind,
            auth.auth.username.as_str(),
            auth.auth.password.as_str()
        ),
        (AuthKind::Basic, "ada", "s3cret")
    );
    let login = request(&items[0]);
    assert_eq!(login.name, "Log in");
    assert_eq!(login.description, "Returns a token.");
    assert_eq!(login.request.method, Method::Post);
    assert_eq!(login.request.url, "{{baseUrl}}/login");
    assert_eq!(login.request.headers, [off(row("X-Trace", "on"))]);
    assert_eq!(login.request.body.mode, BodyMode::Raw);
    assert_eq!(login.request.body.language, RawLanguage::Json);
    assert!(login.request.body.raw.contains("\"user\": \"ada\""));
    assert_eq!(login.request.auth.kind, AuthKind::None, "noauth");
    assert_eq!(
        login.request.scripts.post_response,
        "pm.test(\"status is 200\", () => pm.response.to.have.status(200));\npm.environment.set(\"token\", pm.response.json().token);"
    );
    let [success] = login.examples.as_slice() else {
        panic!("{:?}", login.examples)
    };
    assert_eq!(success.name, "Success");
    assert_eq!(success.status, 200);
    assert_eq!(
        success.headers,
        [Header {
            name: "Content-Type".into(),
            value: "application/json".into()
        }]
    );
    assert_eq!(&success.body[..], b"{\"token\":\"t-1\"}");
    assert_eq!(success.request.body.raw, "{\"user\":\"ada\"}");
    assert_eq!(success.request.auth.kind, AuthKind::Inherit);

    let (charges, items) = folder(&c.items[1]);
    assert_eq!(charges.auth.kind, AuthKind::Inherit, "no auth inherits");
    // Query params with a description and a disabled row; options.
    let list = &request(&items[0]).request;
    assert_eq!(list.url, "{{baseUrl}}/charges?limit=10&cursor={{cursor}}");
    let rows: Vec<&KeyValue> = list.params.iter().map(|p| &p.row).collect();
    assert_eq!(
        rows,
        [
            &described(row("limit", "10"), "At most 100."),
            &off(row("status", "paid")),
            &row("cursor", "{{cursor}}"),
        ]
    );
    assert_eq!(
        list.options,
        SendOptions {
            follow_redirects: false,
            verify_tls: false,
            cookies: false,
        }
    );
    assert_eq!(list.auth.kind, AuthKind::Inherit);
    // urlencoded, with an unsupported auth type.
    let create = &request(&items[1]).request;
    assert_eq!(create.url, "{{baseUrl}}/charges");
    assert_eq!(create.body.mode, BodyMode::UrlEncoded);
    assert_eq!(
        create.body.urlencoded,
        [
            row("amount", "500"),
            described(off(row("currency", "usd")), "Defaults to usd."),
        ]
    );
    assert_eq!(create.auth.kind, AuthKind::None);
    // form-data with text and file fields.
    let upload = &request(&items[2]).request;
    assert_eq!(upload.method, Method::Put);
    assert_eq!(upload.body.mode, BodyMode::FormData);
    assert_eq!(
        upload.body.form,
        [
            FormField {
                row: row("note", "March"),
                file: None,
            },
            FormField {
                row: row("receipt", ""),
                file: Some(PathBuf::from("/home/ada/receipt.pdf")),
            },
            FormField {
                row: row("empty", ""),
                file: None,
            },
        ]
    );
    // A binary file, and a URL given only as parts.
    let import_charges = &request(&items[3]).request;
    assert_eq!(import_charges.body.mode, BodyMode::Binary);
    assert_eq!(
        import_charges.body.binary.as_deref(),
        Some(std::path::Path::new("/home/ada/charges.csv"))
    );
    assert_eq!(
        import_charges.url,
        "https://api.pay.test:8443/v1/import?dry=1"
    );
    assert_eq!(import_charges.params[0].row, row("dry", "1"));

    // GraphQL as the JSON a server receives.
    let graph = &request(&c.items[2]).request;
    assert_eq!(
        (graph.body.mode, graph.body.language),
        (BodyMode::Raw, RawLanguage::Json)
    );
    assert_eq!(
        graph.body.raw,
        "{\n  \"query\": \"query { charges { id } }\",\n  \"variables\": {\n    \"first\": 2\n  }\n}"
    );
    // A request that is only a URL.
    let health = &request(&c.items[3]).request;
    assert_eq!(health.url, "https://api.pay.test/health");
    assert_eq!(health.method, Method::Get);
    assert_eq!(health.auth.kind, AuthKind::Inherit);
}

#[test]
fn f11_ac4_what_was_not_carried_over() {
    let (_, skipped) = collection(PAYMENTS);
    assert_eq!(
        skipped,
        [
            "Folder Auth: folder variables are not supported",
            "POST Create charge: auth type \"ntlm\" is not supported, set to No Auth",
            "PUT Upload receipt: path variables are not supported (:id)",
            "PUT Upload receipt: form field \"receipt\" has 2 files, only the first is kept",
            "PUT Upload receipt: form field \"empty\" names no file, imported as text",
        ]
    );
}

#[test]
fn f11_ac3_postman_v2_0_and_odd_shapes() {
    let text = r#"{
        "info": { "name": "Old", "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json" },
        "item": [
            { "name": "Get", "request": {
                "url": "https://a.test/x?y=1",
                "method": "get",
                "header": "Accept: text/plain\nX-A: b",
                "auth": { "type": "basic", "basic": { "username": "u", "password": "p" } }
            } },
            { "name": "", "request": { "url": "https://a.test", "method": "BAD METHOD", "body": { "mode": "magic" } } },
            { "request": { "url": "https://a.test/2", "method": "POST", "body": { "mode": "raw", "raw": "x", "disabled": true } } }
        ],
        "auth": { "type": "apikey", "apikey": { "key": "X-Key", "value": "v", "in": "query" } }
    }"#;
    let (c, skipped) = collection(text);
    assert_eq!(
        c.file.auth,
        AuthDraft {
            kind: AuthKind::ApiKey,
            key: "X-Key".into(),
            value: "v".into(),
            location: ApiKeyLocation::Query,
            ..AuthDraft::default()
        }
    );
    let get = request(&c.items[0]);
    assert_eq!(get.request.method, Method::Get, "methods ignore case");
    assert_eq!(
        get.request.headers,
        [row("Accept", "text/plain"), row("X-A", "b")]
    );
    assert_eq!(get.request.params[0].row, row("y", "1"));
    assert_eq!(
        (
            get.request.auth.kind,
            get.request.auth.username.as_str(),
            get.request.auth.password.as_str()
        ),
        (AuthKind::Basic, "u", "p")
    );
    let odd = request(&c.items[1]);
    assert_eq!(odd.name, "New Request", "a blank name gets one");
    assert_eq!(odd.request.method, Method::Get);
    assert_eq!(odd.request.body.mode, BodyMode::None);
    let disabled = request(&c.items[2]);
    assert_eq!(
        disabled.request.body.mode,
        BodyMode::None,
        "a disabled body"
    );
    assert_eq!(disabled.request.body.raw, "x", "keeps its text");
    assert_eq!(
        skipped,
        [
            "GET New Request: method \"BAD METHOD\" is not valid (Method contains invalid character ' '), set to GET",
            "GET New Request: body mode \"magic\" is not supported, set to none",
        ]
    );
}

#[test]
fn f11_ac5_postman_environment() {
    let text = r#"{
        "id": "e1", "name": "Staging",
        "values": [
            { "key": "base", "value": "https://staging", "type": "default", "enabled": true },
            { "key": "token", "value": "abc", "type": "secret", "enabled": true },
            { "key": "off", "value": 1, "enabled": false }
        ],
        "_postman_variable_scope": "environment"
    }"#;
    let Import::Environment(env) = import(text).unwrap().import else {
        panic!("not an environment")
    };
    assert_eq!(env.name.as_deref(), Some("Staging"));
    let vars: Vec<(&str, &str, bool, bool)> = env
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled, v.secret))
        .collect();
    assert_eq!(
        vars,
        [
            ("base", "https://staging", true, false),
            ("token", "abc", true, true),
            ("off", "1", false, false),
        ]
    );
    // Exported, secret values are empty, and it reads back.
    let exported = export_environment("Staging", &env.variables);
    assert!(!exported.contains("abc"), "{exported}");
    let Import::Environment(back) = import(&exported).unwrap().import else {
        panic!("not an environment")
    };
    assert_eq!(back.name.as_deref(), Some("Staging"));
    assert_eq!(back.variables[1].value, "");
    assert_eq!(back.variables[0], env.variables[0]);
    assert_eq!(back.variables[2], env.variables[2]);
}

#[test]
fn f11_ac2_detects_the_format() {
    assert!(matches!(
        import("  curl https://a.test -X DELETE").unwrap().import,
        Import::Request(RequestDraft {
            method: Method::Delete,
            ..
        })
    ));
    assert!(matches!(
        import(&format!("\u{feff}{PAYMENTS}")).unwrap().import,
        Import::Collection(_)
    ));
    let globals = r#"{ "name": "Globals", "values": [], "_postman_variable_scope": "globals" }"#;
    assert!(matches!(
        import(globals).unwrap().import,
        Import::Environment(_)
    ));
    for text in ["", "hello", "{\"a\": 1}", "[1, 2]", "{\"info\": 1}"] {
        assert_eq!(
            import(text).unwrap_err().to_string(),
            NOT_IMPORTABLE,
            "{text}"
        );
    }
    assert_eq!(
        import("{ \"info\": ").unwrap_err().to_string(),
        "Ferry cannot import this: it is not valid JSON (line 1, column 9)"
    );
    assert_eq!(
        import("curl 'https://a").unwrap_err().to_string(),
        "Could not read the cURL command: a quote is not closed"
    );
}

#[test]
fn f11_ac6_export_format() {
    let (c, _) = collection(PAYMENTS);
    insta::assert_snapshot!(export_collection(&c));
}

/// Secret values are exported empty.
fn without_secrets(mut c: Collection) -> Collection {
    for v in &mut c.file.variables {
        if v.secret {
            v.value.clear();
        }
    }
    c
}

#[test]
fn f11_ac6_export_reads_back_the_same() {
    let (c, _) = collection(PAYMENTS);
    let (back, skipped) = collection(&export_collection(&c));
    assert_eq!(skipped, Vec::<String>::new());
    assert_eq!(back, without_secrets(c));
}

// --- property: any collection survives export and import ---------------------------

fn text_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 _.-]{0,8}",
        "\\PC{0,8}",
        "[a-z{}\"'\\\\\n\t ]{0,8}",
    ]
}

fn name_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9 ]{0,8}"
}

fn kv_strategy() -> impl Strategy<Value = KeyValue> {
    (
        text_strategy(),
        text_strategy(),
        text_strategy(),
        any::<bool>(),
    )
        .prop_map(|(key, value, description, enabled)| KeyValue {
            key,
            value,
            description,
            enabled,
        })
}

fn path_strategy() -> impl Strategy<Value = PathBuf> {
    "[a-z/._ ]{1,10}".prop_map(PathBuf::from)
}

fn body_strategy() -> impl Strategy<Value = BodyDraft> {
    (
        prop::sample::select(BodyMode::ALL.to_vec()),
        text_strategy(),
        prop::sample::select(RawLanguage::ALL.to_vec()),
        prop::collection::vec(kv_strategy(), 0..3),
        prop::collection::vec((kv_strategy(), prop::option::of(path_strategy())), 0..3),
        prop::option::of(path_strategy()),
    )
        .prop_map(
            |(mode, raw, language, urlencoded, form, binary)| BodyDraft {
                mode,
                raw,
                language,
                urlencoded,
                form: form
                    .into_iter()
                    .map(|(row, file)| FormField { row, file })
                    .collect(),
                binary,
            },
        )
}

fn auth_strategy(kinds: Vec<AuthKind>) -> impl Strategy<Value = AuthDraft> {
    (
        prop::sample::select(kinds),
        prop::collection::vec(text_strategy(), 5),
        prop::sample::select(ApiKeyLocation::ALL.to_vec()),
    )
        .prop_map(|(kind, f, location)| AuthDraft {
            kind,
            username: f[0].clone(),
            password: f[1].clone(),
            token: f[2].clone(),
            key: f[3].clone(),
            value: f[4].clone(),
            location,
        })
}

fn scripts_strategy() -> impl Strategy<Value = Scripts> {
    (text_strategy(), text_strategy()).prop_map(|(pre_request, post_response)| Scripts {
        pre_request,
        post_response,
    })
}

fn draft_strategy() -> impl Strategy<Value = RequestDraft> {
    (
        prop::sample::select(vec![
            Method::Get,
            Method::Post,
            Method::Patch,
            Method::Custom("PROPFIND".into()),
        ]),
        prop::sample::select(vec![
            "https://a.test/p",
            "{{base}}/x/y",
            "http://h:8080",
            "",
        ]),
        prop::collection::vec(
            (kv_strategy(), "[a-z]{1,4}").prop_map(|(mut kv, key)| {
                kv.key = key;
                ferry_core::params::Param {
                    row: kv,
                    original: None,
                }
            }),
            0..3,
        ),
        prop::collection::vec(kv_strategy(), 0..3),
        body_strategy(),
        auth_strategy(AuthKind::ALL.to_vec()),
        (any::<bool>(), any::<bool>(), any::<bool>()),
        scripts_strategy(),
    )
        .prop_map(
            |(method, base, rows, headers, body, auth, (follow, verify, cookies), scripts)| {
                let url = params::url_with_rows(base, &rows);
                RequestDraft {
                    method,
                    params: params::rows_from_url(&url, &rows),
                    url,
                    headers,
                    body,
                    auth,
                    options: SendOptions {
                        follow_redirects: follow,
                        verify_tls: verify,
                        cookies,
                    },
                    scripts,
                }
            },
        )
}

fn example_strategy() -> impl Strategy<Value = Example> {
    (
        name_strategy(),
        draft_strategy(),
        any::<u16>(),
        prop::collection::vec((text_strategy(), text_strategy()), 0..2),
        text_strategy(),
    )
        .prop_map(|(name, request, status, headers, body)| Example {
            name,
            request,
            status,
            headers: headers
                .into_iter()
                .map(|(name, value)| Header { name, value })
                .collect(),
            body: body.into(),
        })
}

fn request_strategy() -> impl Strategy<Value = Entry> {
    (
        name_strategy(),
        text_strategy(),
        draft_strategy(),
        prop::collection::vec(example_strategy(), 0..2),
    )
        .prop_map(|(name, description, request, examples)| {
            Entry::Request(RequestFile {
                name,
                description,
                request,
                examples,
            })
        })
}

fn entries_strategy() -> impl Strategy<Value = Vec<Entry>> {
    let leaf = prop::collection::vec(request_strategy(), 0..3);
    leaf.prop_recursive(2, 8, 3, |inner| {
        prop::collection::vec(
            prop_oneof![
                request_strategy(),
                (
                    name_strategy(),
                    text_strategy(),
                    auth_strategy(AuthKind::ALL.to_vec()),
                    scripts_strategy(),
                    inner
                )
                    .prop_map(|(name, description, auth, scripts, items)| {
                        Entry::Folder {
                            file: FolderFile {
                                name,
                                description,
                                auth,
                                scripts,
                            },
                            items,
                        }
                    }),
            ],
            0..3,
        )
    })
}

fn collection_strategy() -> impl Strategy<Value = Collection> {
    (
        name_strategy(),
        text_strategy(),
        auth_strategy(vec![
            AuthKind::None,
            AuthKind::Basic,
            AuthKind::Bearer,
            AuthKind::ApiKey,
        ]),
        prop::collection::vec(
            (
                text_strategy(),
                text_strategy(),
                any::<bool>(),
                any::<bool>(),
            )
                .prop_map(|(key, value, enabled, secret)| Variable {
                    key,
                    value,
                    enabled,
                    secret,
                    ..Variable::default()
                }),
            0..3,
        ),
        scripts_strategy(),
        entries_strategy(),
    )
        .prop_map(
            |(name, description, auth, variables, scripts, items)| Collection {
                file: CollectionFile {
                    name,
                    description,
                    auth,
                    variables,
                    scripts,
                },
                items,
            },
        )
}

/// What Postman has no place for: values under Inherit, which Postman
/// writes as no auth at all, and the scripts of an example's request.
fn normalized(c: Collection) -> Collection {
    fn auth(a: &mut AuthDraft) {
        if a.kind == AuthKind::Inherit {
            *a = AuthDraft {
                kind: AuthKind::Inherit,
                ..AuthDraft::default()
            };
        }
    }
    fn walk(items: &mut [Entry]) {
        for item in items {
            match item {
                Entry::Folder { file, items } => {
                    auth(&mut file.auth);
                    walk(items);
                }
                Entry::Request(r) => {
                    auth(&mut r.request.auth);
                    for e in &mut r.examples {
                        auth(&mut e.request.auth);
                        e.request.scripts = Scripts::default();
                    }
                }
            }
        }
    }
    let mut c = without_secrets(c);
    walk(&mut c.items);
    c
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn f11_ac6_prop_export_then_import_is_the_same(c in collection_strategy()) {
        let text = export_collection(&c);
        let (back, skipped) = collection(&text);
        prop_assert_eq!(skipped, Vec::<String>::new());
        prop_assert_eq!(back, normalized(c), "{}", text);
    }

    #[test]
    fn f11_ac2_prop_import_never_panics(text in "\\PC{0,200}") {
        let _ = import(&text);
        let _ = import(&format!("{{\"info\":{{}},\"item\":[{text}]}}"));
    }
}
