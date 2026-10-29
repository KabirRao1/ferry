//! F11-AC6: exporting an imported collection gives text that imports and
//! exports the same way again, for Postman-shaped files where any field may
//! hold the wrong kind of value. The fuzz target `import` checks the same on
//! arbitrary text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::import::postman::export_collection;
use ferry_core::import::{Collection, Import, import};
use proptest::prelude::*;
use serde_json::{Value, json};

fn collection(text: &str) -> (Collection, Vec<String>) {
    let imported = import(text).unwrap();
    match imported.import {
        Import::Collection(c) => (c, imported.skipped),
        other => panic!("not a collection: {other:?}"),
    }
}

/// Any JSON, made of Postman's field names and values.
fn junk() -> impl Strategy<Value = Value> {
    const KEYS: &[&str] = &[
        "name",
        "item",
        "request",
        "url",
        "raw",
        "query",
        "key",
        "value",
        "disabled",
        "description",
        "content",
        "method",
        "header",
        "body",
        "mode",
        "type",
        "src",
        "auth",
    ];
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (0u32..70000).prop_map(Value::from),
        word(),
    ];
    leaf.prop_recursive(2, 12, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(Value::Array),
            prop::collection::vec((prop::sample::select(KEYS), inner), 0..3).prop_map(|pairs| {
                Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
            }),
        ]
    })
}

fn word() -> impl Strategy<Value = Value> {
    const WORDS: &[&str] = &[
        "raw",
        "urlencoded",
        "formdata",
        "file",
        "graphql",
        "json",
        "xml",
        "text",
        "basic",
        "bearer",
        "apikey",
        "noauth",
        "inherit",
        "ntlm",
        "query",
        "header",
        "prerequest",
        "test",
        "secret",
        "GET",
        "post",
        "PROP FIND",
        "https://a.test/x?a=1&b",
        "{{base}}/p?q",
        "http://h:80/a/b",
        "",
    ];
    prop_oneof![
        prop::sample::select(WORDS).prop_map(Value::from),
        "\\PC{0,6}".prop_map(Value::from),
    ]
}

/// A value that is usually the right shape and sometimes anything.
fn or_junk(good: impl Strategy<Value = Value>) -> impl Strategy<Value = Value> {
    prop_oneof![4 => good, 1 => junk()]
}

/// An object with the fields that are present.
fn obj(fields: Vec<(&'static str, Option<Value>)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v)))
            .collect(),
    )
}

fn row_json() -> impl Strategy<Value = Value> {
    or_junk(
        (
            word(),
            prop::option::of(word()),
            prop::option::of(any::<bool>()),
            prop::option::of(or_junk(word())),
            prop::option::of(prop::sample::select(vec!["text", "file"])),
            prop::option::of(or_junk(
                prop::collection::vec(word(), 0..3).prop_map(Value::Array),
            )),
        )
            .prop_map(|(key, value, disabled, description, kind, src)| {
                obj(vec![
                    ("key", Some(key)),
                    ("value", value),
                    ("disabled", disabled.map(Value::Bool)),
                    ("description", description),
                    ("type", kind.map(Value::from)),
                    ("src", src),
                ])
            }),
    )
}

fn rows_json() -> impl Strategy<Value = Value> {
    prop::collection::vec(row_json(), 0..3).prop_map(Value::Array)
}

fn url_value() -> impl Strategy<Value = Value> {
    or_junk(prop_oneof![
        word(),
        (
            prop::option::of(word()),
            prop::option::of(rows_json()),
            prop::option::of(word()),
            prop::option::of(junk()),
            prop::option::of(rows_json()),
        )
            .prop_map(|(raw, query, protocol, host, variable)| {
                obj(vec![
                    ("raw", raw),
                    ("query", query),
                    ("protocol", protocol),
                    ("host", host),
                    ("variable", variable),
                ])
            }),
    ])
}

fn body_value() -> impl Strategy<Value = Value> {
    or_junk(
        (
            prop::option::of(word()),
            prop::option::of(word()),
            prop::option::of(rows_json()),
            prop::option::of(rows_json()),
            prop::option::of(word()),
            prop::option::of(word()),
            prop::option::of(any::<bool>()),
        )
            .prop_map(
                |(mode, raw, urlencoded, formdata, src, language, disabled)| {
                    obj(vec![
                        ("mode", mode),
                        ("raw", raw),
                        ("urlencoded", urlencoded),
                        ("formdata", formdata),
                        ("file", src.map(|s| json!({ "src": s }))),
                        (
                            "options",
                            language.map(|l| json!({ "raw": { "language": l } })),
                        ),
                        (
                            "graphql",
                            Some(json!({ "query": "{ a }", "variables": "{\"n\": 1}" })),
                        ),
                        ("disabled", disabled.map(Value::Bool)),
                    ])
                },
            ),
    )
}

fn auth_value() -> impl Strategy<Value = Value> {
    or_junk(
        (word(), rows_json(), rows_json(), prop::option::of(junk())).prop_map(
            |(kind, basic, apikey, bearer)| {
                obj(vec![
                    ("type", Some(kind)),
                    ("basic", Some(basic)),
                    ("apikey", Some(apikey)),
                    ("bearer", bearer),
                ])
            },
        ),
    )
}

fn request_value() -> impl Strategy<Value = Value> {
    or_junk(prop_oneof![
        word(),
        (
            prop::option::of(word()),
            prop::option::of(url_value()),
            prop::option::of(prop_oneof![rows_json(), word()]),
            prop::option::of(body_value()),
            prop::option::of(auth_value()),
            prop::option::of(word()),
        )
            .prop_map(|(method, url, header, body, auth, description)| {
                obj(vec![
                    ("method", method),
                    ("url", url),
                    ("header", header),
                    ("body", body),
                    ("auth", auth),
                    ("description", description),
                ])
            }),
    ])
}

fn events_value() -> impl Strategy<Value = Value> {
    prop::collection::vec(
        (
            word(),
            prop_oneof![
                word(),
                prop::collection::vec(word(), 0..3).prop_map(Value::Array)
            ],
        )
            .prop_map(|(listen, exec)| json!({ "listen": listen, "script": { "exec": exec } })),
        0..3,
    )
    .prop_map(Value::Array)
}

fn example_value() -> impl Strategy<Value = Value> {
    (
        word(),
        prop::option::of(request_value()),
        junk(),
        rows_json(),
        word(),
    )
        .prop_map(|(name, original, code, header, body)| {
            obj(vec![
                ("name", Some(name)),
                ("originalRequest", original),
                ("code", Some(code)),
                ("header", Some(header)),
                ("body", Some(body)),
            ])
        })
}

fn item_value() -> impl Strategy<Value = Value> {
    let request = (
        word(),
        request_value(),
        prop::option::of(prop::collection::vec(example_value(), 0..2).prop_map(Value::Array)),
        prop::option::of(events_value()),
        prop::option::of(or_junk(
            (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
                |(f, s, c)| json!({ "followRedirects": f, "strictSSL": s, "disableCookies": c }),
            ),
        )),
    )
        .prop_map(|(name, request, response, event, behavior)| {
            obj(vec![
                ("name", Some(name)),
                ("request", Some(request)),
                ("response", response),
                ("event", event),
                ("protocolProfileBehavior", behavior),
            ])
        });
    request.prop_recursive(2, 10, 3, |inner| {
        (
            word(),
            prop::collection::vec(inner, 0..3),
            prop::option::of(auth_value()),
            prop::option::of(events_value()),
            prop::option::of(word()),
        )
            .prop_map(|(name, items, auth, event, description)| {
                obj(vec![
                    ("name", Some(name)),
                    ("item", Some(Value::Array(items))),
                    ("auth", auth),
                    ("event", event),
                    ("description", description),
                ])
            })
    })
}

fn collection_value() -> impl Strategy<Value = Value> {
    (
        word(),
        prop::collection::vec(item_value(), 0..4),
        prop::option::of(auth_value()),
        prop::option::of(events_value()),
        prop::option::of(rows_json()),
    )
        .prop_map(|(name, items, auth, event, variable)| {
            obj(vec![
                ("info", Some(json!({ "name": name }))),
                ("item", Some(Value::Array(items))),
                ("auth", auth),
                ("event", event),
                ("variable", variable),
            ])
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn f11_ac6_prop_export_is_a_fixed_point(value in collection_value()) {
        let text = value.to_string();
        let (c, _) = collection(&text);
        let written = export_collection(&c);
        let (again, skipped) = collection(&written);
        prop_assert_eq!(skipped, Vec::<String>::new(), "{}", text);
        prop_assert_eq!(export_collection(&again), written, "{}", text);
    }
}
