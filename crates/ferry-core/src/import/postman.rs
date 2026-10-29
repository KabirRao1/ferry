//! Postman Collection v2.0 and v2.1 files and Postman environments
//! (F11-AC3, AC5, AC6).
//!
//! Export writes every part of a request Ferry keeps, including body and
//! auth content of modes that are not selected, so importing an exported
//! file gives back the same collection.

use std::path::PathBuf;

use bytes::Bytes;
use serde_json::{Map, Value, json};

use super::{Collection, Entry, Import, Imported, array, or, text};
use crate::auth::{ApiKeyLocation, AuthDraft, AuthKind};
use crate::body::{BodyDraft, BodyMode, FormField, RawLanguage};
use crate::collection::{CollectionFile, Example, FolderFile, RequestFile};
use crate::draft::RequestDraft;
use crate::environment::VariablesFile;
use crate::kv::KeyValue;
use crate::params::{self, Param};
use crate::request::{Method, SendOptions};
use crate::response::Header;
use crate::scripts::Scripts;
use crate::variables::Variable;

/// The schema Ferry writes.
pub const SCHEMA_V21: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

pub(crate) fn is_collection(value: &Value) -> bool {
    let info = &value["info"];
    info.is_object()
        && (info["schema"]
            .as_str()
            .is_some_and(|s| s.contains("getpostman.com"))
            || value["item"].is_array())
}

pub(crate) fn is_environment(value: &Value) -> bool {
    value["values"].is_array()
        && (value.get("_postman_variable_scope").is_some() || value["name"].is_string())
}

// --- import ----------------------------------------------------------------------------

fn description(value: &Value) -> String {
    match value {
        Value::Object(_) => text(&value["content"]),
        other => text(other),
    }
}

fn disabled(value: &Value) -> bool {
    value["disabled"].as_bool().unwrap_or(false)
}

fn key_value(value: &Value) -> KeyValue {
    KeyValue {
        key: text(&value["key"]),
        value: text(&value["value"]),
        description: description(&value["description"]),
        enabled: !disabled(value),
    }
}

pub(crate) fn import_collection(value: &Value) -> Imported {
    let mut skipped = Vec::new();
    let info = &value["info"];
    let name = or(text(&info["name"]), "Imported Collection");
    let label = format!("Collection {name}");
    let file = CollectionFile {
        auth: auth(&value["auth"], AuthKind::None, &label, &mut skipped),
        description: description(&info["description"]),
        variables: variables(&value["variable"]),
        scripts: scripts(&value["event"]),
        name,
    };
    let items = entries(&value["item"], &mut skipped);
    // Examples repeat their request's notes.
    let mut seen = std::collections::HashSet::new();
    skipped.retain(|line| seen.insert(line.clone()));
    Imported {
        import: Import::Collection(Collection { file, items }),
        skipped,
    }
}

fn entries(value: &Value, skipped: &mut Vec<String>) -> Vec<Entry> {
    array(value)
        .iter()
        .map(|item| {
            if item["item"].is_array() {
                folder(item, skipped)
            } else {
                Entry::Request(request_file(item, skipped))
            }
        })
        .collect()
}

fn folder(item: &Value, skipped: &mut Vec<String>) -> Entry {
    let name = or(text(&item["name"]), "New Folder");
    let label = format!("Folder {name}");
    if !array(&item["variable"]).is_empty() {
        skipped.push(format!("{label}: folder variables are not supported"));
    }
    let file = FolderFile {
        auth: auth(&item["auth"], AuthKind::Inherit, &label, skipped),
        description: description(&item["description"]),
        scripts: scripts(&item["event"]),
        name,
    };
    Entry::Folder {
        file,
        items: entries(&item["item"], skipped),
    }
}

fn request_file(item: &Value, skipped: &mut Vec<String>) -> RequestFile {
    let req = &item["request"];
    // The method the request ends up with, which is GET when it is invalid.
    let method = Method::parse(&text(&req["method"])).unwrap_or(Method::Get);
    let name = or(text(&item["name"]), "New Request");
    let label = format!("{} {name}", method.as_str());
    let mut request = draft(req, &label, skipped);
    request.options = options(&item["protocolProfileBehavior"]);
    request.scripts = scripts(&item["event"]);
    let examples = array(&item["response"])
        .iter()
        .map(|r| example(r, &request, &label, skipped))
        .collect();
    RequestFile {
        name,
        description: description(&req["description"]),
        request,
        examples,
    }
}

/// A request without its scripts and options, which Postman keeps on the
/// item.
fn draft(req: &Value, label: &str, skipped: &mut Vec<String>) -> RequestDraft {
    let inherit = AuthDraft {
        kind: AuthKind::Inherit,
        ..AuthDraft::default()
    };
    if let Value::String(url) = req {
        return RequestDraft {
            url: url.clone(),
            params: params::rows_from_url(url, &[]),
            auth: inherit,
            ..RequestDraft::default()
        };
    }
    let method_text = text(&req["method"]);
    let method = if method_text.trim().is_empty() {
        Method::Get
    } else {
        Method::parse(&method_text).unwrap_or_else(|e| {
            skipped.push(format!(
                "{label}: method \"{method_text}\" is not valid ({e}), set to GET"
            ));
            Method::Get
        })
    };
    let (url, params) = url(&req["url"]);
    let path_variables: Vec<String> = array(&req["url"]["variable"])
        .iter()
        .map(|v| format!(":{}", text(&v["key"])))
        .collect();
    if !path_variables.is_empty() {
        skipped.push(format!(
            "{label}: path variables are not supported ({})",
            path_variables.join(", ")
        ));
    }
    RequestDraft {
        method,
        url,
        params,
        headers: headers(&req["header"]),
        body: body(&req["body"], label, skipped),
        auth: auth(&req["auth"], AuthKind::Inherit, label, skipped),
        ..RequestDraft::default()
    }
}

/// The URL text and its params table: rows from the URL, with the disabled
/// rows and descriptions of Postman's `query` list.
fn url(value: &Value) -> (String, Vec<Param>) {
    let raw = match value {
        Value::String(s) => s.clone(),
        Value::Object(_) if value["raw"].is_string() => text(&value["raw"]),
        Value::Object(_) => assemble_url(value),
        _ => String::new(),
    };
    let listed: Vec<Param> = array(&value["query"])
        .iter()
        .map(|q| Param {
            row: key_value(q),
            original: None,
        })
        .collect();
    let params = params::rows_from_url(&raw, &listed);
    (raw, params)
}

/// A URL from Postman's parts, for files without `raw`.
fn assemble_url(value: &Value) -> String {
    let join = |part: &Value, sep: &str| match part {
        Value::Array(items) => items.iter().map(text).collect::<Vec<_>>().join(sep),
        other => text(other),
    };
    let mut out = String::new();
    let protocol = text(&value["protocol"]);
    if !protocol.is_empty() {
        out.push_str(&protocol);
        out.push_str("://");
    }
    out.push_str(&join(&value["host"], "."));
    let port = text(&value["port"]);
    if !port.is_empty() {
        out.push(':');
        out.push_str(&port);
    }
    let path = join(&value["path"], "/");
    if !path.is_empty() {
        if !path.starts_with('/') {
            out.push('/');
        }
        out.push_str(&path);
    }
    let query: Vec<String> = array(&value["query"])
        .iter()
        .filter(|q| !disabled(q))
        .map(|q| {
            let (key, value) = (text(&q["key"]), text(&q["value"]));
            if q["value"].is_null() {
                key
            } else {
                format!("{key}={value}")
            }
        })
        .collect();
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    out
}

fn headers(value: &Value) -> Vec<KeyValue> {
    match value {
        Value::String(lines) => lines
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| KeyValue::new(k.trim(), v.trim()))
            .collect(),
        other => array(other).iter().map(key_value).collect(),
    }
}

fn body(value: &Value, label: &str, skipped: &mut Vec<String>) -> BodyDraft {
    if !value.is_object() {
        return BodyDraft::default();
    }
    let mut body = BodyDraft {
        raw: text(&value["raw"]),
        language: match value["options"]["raw"]["language"].as_str() {
            Some("json") => RawLanguage::Json,
            Some("xml") => RawLanguage::Xml,
            Some("html") => RawLanguage::Html,
            Some("javascript") => RawLanguage::JavaScript,
            _ => RawLanguage::Text,
        },
        urlencoded: array(&value["urlencoded"]).iter().map(key_value).collect(),
        form: array(&value["formdata"])
            .iter()
            .map(|f| form_field(f, label, skipped))
            .collect(),
        binary: value["file"]["src"]
            .as_str()
            .filter(|src| !src.is_empty())
            .map(PathBuf::from),
        ..BodyDraft::default()
    };
    body.mode = match value["mode"].as_str() {
        Some("raw") => BodyMode::Raw,
        Some("urlencoded") => BodyMode::UrlEncoded,
        Some("formdata") => BodyMode::FormData,
        Some("file") => BodyMode::Binary,
        Some("graphql") => {
            body.raw = graphql(&value["graphql"]);
            body.language = RawLanguage::Json;
            BodyMode::Raw
        }
        Some(other) => {
            skipped.push(format!(
                "{label}: body mode \"{other}\" is not supported, set to none"
            ));
            BodyMode::None
        }
        None => BodyMode::None,
    };
    if disabled(value) {
        body.mode = BodyMode::None;
    }
    body
}

/// A GraphQL body as the JSON a server receives.
fn graphql(value: &Value) -> String {
    let variables = match &value["variables"] {
        Value::String(s) if !s.trim().is_empty() => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        Value::Object(_) => value["variables"].clone(),
        _ => json!({}),
    };
    let body = json!({ "query": text(&value["query"]), "variables": variables });
    serde_json::to_string_pretty(&body).unwrap_or_default()
}

fn form_field(value: &Value, label: &str, skipped: &mut Vec<String>) -> FormField {
    let row = key_value(value);
    if value["type"].as_str() != Some("file") {
        return FormField { row, file: None };
    }
    let sources: Vec<String> = match &value["src"] {
        Value::Array(items) => items.iter().map(text).collect(),
        other => vec![text(other)],
    }
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();
    let file = match sources.as_slice() {
        [] => {
            skipped.push(format!(
                "{label}: form field \"{}\" names no file, imported as text",
                row.key
            ));
            None
        }
        [first, rest @ ..] => {
            if !rest.is_empty() {
                skipped.push(format!(
                    "{label}: form field \"{}\" has {} files, only the first is kept",
                    row.key,
                    sources.len()
                ));
            }
            Some(PathBuf::from(first))
        }
    };
    FormField { row, file }
}

/// One parameter of an auth type: v2.1 lists `{key, value}` pairs, v2.0
/// has an object.
fn auth_param(group: &Value, key: &str) -> String {
    match group {
        Value::Array(pairs) => pairs
            .iter()
            .find(|p| p["key"].as_str() == Some(key))
            .map(|p| text(&p["value"]))
            .unwrap_or_default(),
        other => text(&other[key]),
    }
}

/// `absent` is what a missing auth means: Inherit for folders and
/// requests, No Auth for collections.
fn auth(value: &Value, absent: AuthKind, label: &str, skipped: &mut Vec<String>) -> AuthDraft {
    let mut auth = AuthDraft {
        kind: absent,
        ..AuthDraft::default()
    };
    if !value.is_object() {
        return auth;
    }
    let (basic, bearer, apikey) = (&value["basic"], &value["bearer"], &value["apikey"]);
    auth.username = auth_param(basic, "username");
    auth.password = auth_param(basic, "password");
    auth.token = auth_param(bearer, "token");
    auth.key = auth_param(apikey, "key");
    auth.value = auth_param(apikey, "value");
    if auth_param(apikey, "in") == "query" {
        auth.location = ApiKeyLocation::Query;
    }
    auth.kind = match value["type"].as_str().unwrap_or("noauth") {
        "noauth" => AuthKind::None,
        "basic" => AuthKind::Basic,
        "bearer" => AuthKind::Bearer,
        "apikey" => AuthKind::ApiKey,
        "inherit" => absent,
        other => {
            skipped.push(format!(
                "{label}: auth type \"{other}\" is not supported, set to No Auth"
            ));
            AuthKind::None
        }
    };
    auth
}

fn options(value: &Value) -> SendOptions {
    SendOptions {
        follow_redirects: value["followRedirects"].as_bool().unwrap_or(true),
        verify_tls: value["strictSSL"].as_bool().unwrap_or(true),
        cookies: !value["disableCookies"].as_bool().unwrap_or(false),
    }
}

fn scripts(value: &Value) -> Scripts {
    let mut scripts = Scripts::default();
    for event in array(value) {
        let exec = &event["script"]["exec"];
        let source = match exec {
            Value::Array(lines) => lines.iter().map(text).collect::<Vec<_>>().join("\n"),
            other => text(other),
        };
        let slot = match event["listen"].as_str() {
            Some("prerequest") => &mut scripts.pre_request,
            Some("test") => &mut scripts.post_response,
            _ => continue,
        };
        if !slot.is_empty() {
            slot.push('\n');
        }
        slot.push_str(&source);
    }
    scripts
}

fn variables(value: &Value) -> Vec<Variable> {
    array(value)
        .iter()
        .map(|v| Variable {
            key: text(&v["key"]),
            value: text(&v["value"]),
            enabled: !disabled(v),
            secret: v["type"].as_str() == Some("secret"),
            ..Variable::default()
        })
        .collect()
}

fn example(
    value: &Value,
    parent: &RequestDraft,
    label: &str,
    skipped: &mut Vec<String>,
) -> Example {
    let original = &value["originalRequest"];
    let request = if original.is_null() {
        parent.clone()
    } else {
        let mut request = draft(original, label, skipped);
        request.options = options(&original["protocolProfileBehavior"]);
        request
    };
    Example {
        name: or(text(&value["name"]), "Example"),
        request,
        status: value["code"]
            .as_u64()
            .and_then(|code| u16::try_from(code).ok())
            .unwrap_or(0),
        headers: array(&value["header"])
            .iter()
            .map(|h| Header {
                name: text(&h["key"]),
                value: text(&h["value"]),
            })
            .collect(),
        body: Bytes::from(text(&value["body"])),
    }
}

pub(crate) fn import_environment(value: &Value) -> Imported {
    let file = VariablesFile {
        name: Some(or(text(&value["name"]), "Imported Environment")),
        variables: array(&value["values"])
            .iter()
            .map(|v| Variable {
                key: text(&v["key"]),
                value: text(&v["value"]),
                enabled: v["enabled"].as_bool().unwrap_or(true),
                secret: v["type"].as_str() == Some("secret"),
                ..Variable::default()
            })
            .collect(),
    };
    Imported {
        import: Import::Environment(file),
        skipped: Vec::new(),
    }
}

// --- export ----------------------------------------------------------------------------

fn object(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<Map<_, _>>(),
    )
}

/// A `{key, value}` row with `disabled` and `description` when set.
fn row_json(row: &KeyValue) -> Value {
    let mut pairs = vec![("key", json!(row.key)), ("value", json!(row.value))];
    if !row.enabled {
        pairs.push(("disabled", json!(true)));
    }
    if !row.description.is_empty() {
        pairs.push(("description", json!(row.description)));
    }
    object(pairs)
}

// CONTRACT
/// A Postman Collection v2.1 file for `collection` (F11-AC6). Secret
/// variable values are written empty.
#[must_use]
pub fn export_collection(collection: &Collection) -> String {
    let file = &collection.file;
    let mut info = vec![("name", json!(file.name)), ("schema", json!(SCHEMA_V21))];
    if !file.description.is_empty() {
        info.push(("description", json!(file.description)));
    }
    let mut pairs = vec![
        ("info", object(info)),
        ("item", items_json(&collection.items)),
    ];
    if file.auth != AuthDraft::default() {
        pairs.push(("auth", auth_json(&file.auth)));
    }
    if let Some(events) = events_json(&file.scripts) {
        pairs.push(("event", events));
    }
    if !file.variables.is_empty() {
        let vars = file
            .variables
            .iter()
            .map(|v| {
                let mut pairs = vec![
                    ("key", json!(v.key)),
                    ("value", json!(if v.secret { "" } else { v.value.as_str() })),
                ];
                if v.secret {
                    pairs.push(("type", json!("secret")));
                }
                if !v.enabled {
                    pairs.push(("disabled", json!(true)));
                }
                object(pairs)
            })
            .collect();
        pairs.push(("variable", Value::Array(vars)));
    }
    serde_json::to_string_pretty(&object(pairs)).unwrap_or_default()
}

fn items_json(items: &[Entry]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| match item {
                Entry::Folder { file, items } => {
                    let mut pairs = vec![("name", json!(file.name)), ("item", items_json(items))];
                    if !file.description.is_empty() {
                        pairs.push(("description", json!(file.description)));
                    }
                    if file.auth.kind != AuthKind::Inherit {
                        pairs.push(("auth", auth_json(&file.auth)));
                    }
                    if let Some(events) = events_json(&file.scripts) {
                        pairs.push(("event", events));
                    }
                    object(pairs)
                }
                Entry::Request(file) => request_item_json(file),
            })
            .collect(),
    )
}

fn request_item_json(file: &RequestFile) -> Value {
    let mut pairs = vec![
        ("name", json!(file.name)),
        ("request", request_json(&file.request, &file.description)),
    ];
    if let Some(behavior) = options_json(file.request.options) {
        pairs.push(("protocolProfileBehavior", behavior));
    }
    if let Some(events) = events_json(&file.request.scripts) {
        pairs.push(("event", events));
    }
    let responses = file
        .examples
        .iter()
        .map(|e| {
            let mut original = request_json(&e.request, "");
            if let (Some(behavior), Value::Object(map)) =
                (options_json(e.request.options), &mut original)
            {
                map.insert("protocolProfileBehavior".to_owned(), behavior);
            }
            object(vec![
                ("name", json!(e.name)),
                ("originalRequest", original),
                ("code", json!(e.status)),
                (
                    "header",
                    Value::Array(
                        e.headers
                            .iter()
                            .map(|h| json!({ "key": h.name, "value": h.value }))
                            .collect(),
                    ),
                ),
                ("body", json!(String::from_utf8_lossy(&e.body))),
            ])
        })
        .collect();
    pairs.push(("response", Value::Array(responses)));
    object(pairs)
}

/// A request's method, URL, headers, body and auth.
fn request_json(d: &RequestDraft, description: &str) -> Value {
    let mut pairs = vec![
        ("method", json!(d.method.as_str())),
        (
            "header",
            Value::Array(d.headers.iter().map(row_json).collect()),
        ),
        ("url", url_json(&d.url, &d.params)),
    ];
    if let Some(body) = body_json(&d.body) {
        pairs.push(("body", body));
    }
    if d.auth.kind != AuthKind::Inherit {
        pairs.push(("auth", auth_json(&d.auth)));
    }
    if !description.is_empty() {
        pairs.push(("description", json!(description)));
    }
    object(pairs)
}

/// Postman's URL object: the text plus the parts Postman shows.
fn url_json(raw: &str, params: &[Param]) -> Value {
    let (protocol, rest) = raw
        .split_once("://")
        .map_or((None, raw), |(p, r)| (Some(p), r));
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, None), |(a, p)| (a, Some(p)));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };
    let mut pairs = vec![("raw", json!(raw))];
    if let Some(protocol) = protocol {
        pairs.push(("protocol", json!(protocol)));
    }
    if !host.is_empty() {
        pairs.push(("host", json!(host.split('.').collect::<Vec<_>>())));
    }
    if let Some(port) = port {
        pairs.push(("port", json!(port)));
    }
    if let Some(path) = path {
        pairs.push(("path", json!(path.split('/').collect::<Vec<_>>())));
    }
    if !params.is_empty() {
        pairs.push((
            "query",
            Value::Array(params.iter().map(|p| row_json(&p.row)).collect()),
        ));
    }
    object(pairs)
}

/// Every part of the body Ferry keeps; a body with no mode is written
/// disabled.
fn body_json(b: &BodyDraft) -> Option<Value> {
    if *b == BodyDraft::default() {
        return None;
    }
    let mode = match b.mode {
        BodyMode::None | BodyMode::Raw => "raw",
        BodyMode::UrlEncoded => "urlencoded",
        BodyMode::FormData => "formdata",
        BodyMode::Binary => "file",
    };
    let mut pairs = vec![("mode", json!(mode))];
    if b.mode == BodyMode::None {
        pairs.push(("disabled", json!(true)));
    }
    if !b.raw.is_empty() {
        pairs.push(("raw", json!(b.raw)));
    }
    let language = match b.language {
        RawLanguage::Text => None,
        RawLanguage::Json => Some("json"),
        RawLanguage::Xml => Some("xml"),
        RawLanguage::Html => Some("html"),
        RawLanguage::JavaScript => Some("javascript"),
    };
    if let Some(language) = language {
        pairs.push(("options", json!({ "raw": { "language": language } })));
    }
    if !b.urlencoded.is_empty() {
        pairs.push((
            "urlencoded",
            Value::Array(b.urlencoded.iter().map(row_json).collect()),
        ));
    }
    if !b.form.is_empty() {
        let fields = b
            .form
            .iter()
            .map(|f| {
                let Value::Object(mut map) = row_json(&f.row) else {
                    unreachable!("row_json makes an object")
                };
                match &f.file {
                    Some(path) => {
                        map.insert("type".to_owned(), json!("file"));
                        map.insert("src".to_owned(), json!(path.to_string_lossy()));
                    }
                    None => {
                        map.insert("type".to_owned(), json!("text"));
                    }
                }
                Value::Object(map)
            })
            .collect();
        pairs.push(("formdata", Value::Array(fields)));
    }
    if let Some(path) = &b.binary {
        pairs.push(("file", json!({ "src": path.to_string_lossy() })));
    }
    Some(object(pairs))
}

/// The selected type and every type's values Ferry keeps.
fn auth_json(a: &AuthDraft) -> Value {
    let kind = match a.kind {
        AuthKind::Inherit | AuthKind::None => "noauth",
        AuthKind::Basic => "basic",
        AuthKind::Bearer => "bearer",
        AuthKind::ApiKey => "apikey",
    };
    let param = |key: &str, value: &str| json!({ "key": key, "value": value, "type": "string" });
    let mut pairs = vec![("type", json!(kind))];
    if !a.username.is_empty() || !a.password.is_empty() {
        pairs.push((
            "basic",
            json!([
                param("username", &a.username),
                param("password", &a.password)
            ]),
        ));
    }
    if !a.token.is_empty() {
        pairs.push(("bearer", json!([param("token", &a.token)])));
    }
    if !a.key.is_empty() || !a.value.is_empty() || a.location != ApiKeyLocation::Header {
        let location = match a.location {
            ApiKeyLocation::Header => "header",
            ApiKeyLocation::Query => "query",
        };
        pairs.push((
            "apikey",
            json!([
                param("key", &a.key),
                param("value", &a.value),
                param("in", location)
            ]),
        ));
    }
    object(pairs)
}

fn options_json(o: SendOptions) -> Option<Value> {
    let mut pairs = Vec::new();
    if !o.follow_redirects {
        pairs.push(("followRedirects", json!(false)));
    }
    if !o.verify_tls {
        pairs.push(("strictSSL", json!(false)));
    }
    if !o.cookies {
        pairs.push(("disableCookies", json!(true)));
    }
    (!pairs.is_empty()).then(|| object(pairs))
}

fn events_json(s: &Scripts) -> Option<Value> {
    let events: Vec<Value> = [("prerequest", &s.pre_request), ("test", &s.post_response)]
        .into_iter()
        .filter(|(_, source)| !source.is_empty())
        .map(|(listen, source)| {
            json!({
                "listen": listen,
                "script": { "type": "text/javascript", "exec": source.split('\n').collect::<Vec<_>>() }
            })
        })
        .collect();
    (!events.is_empty()).then_some(Value::Array(events))
}

// CONTRACT
/// A Postman environment file (F11-AC6). Secret values are written empty.
#[must_use]
pub fn export_environment(name: &str, variables: &[Variable]) -> String {
    let values = variables
        .iter()
        .map(|v| {
            json!({
                "key": v.key,
                "value": if v.secret { "" } else { v.value.as_str() },
                "type": if v.secret { "secret" } else { "default" },
                "enabled": v.enabled,
            })
        })
        .collect();
    let file = object(vec![
        ("name", json!(name)),
        ("values", Value::Array(values)),
        ("_postman_variable_scope", json!("environment")),
    ]);
    serde_json::to_string_pretty(&file).unwrap_or_default()
}
