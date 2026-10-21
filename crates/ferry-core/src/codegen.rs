//! Code snippets: the request as code in twelve languages (F19).
//!
//! Generators take a [`Snippet`], made from a built request, and return
//! source text. Every string goes through its language's escaper, so any
//! header or body text survives.

use std::fmt::Write as _;

use crate::body::BodyMode;
use crate::draft::RequestDraft;
use crate::request::{PartValue, Request, RequestBody};

// CONTRACT
/// A request as the generators see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub method: String,
    pub url: String,
    /// The user's enabled headers, the auth header and `Content-Type` from
    /// the body (none for form-data, whose boundary the code makes).
    pub headers: Vec<(String, String)>,
    pub body: SnippetBody,
    pub follow_redirects: bool,
    pub verify_tls: bool,
}

// CONTRACT
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetBody {
    None,
    Raw(String),
    UrlEncoded(Vec<(String, String)>),
    FormData(Vec<(String, FormValue)>),
    /// A file sent as the whole body.
    File(String),
}

// CONTRACT
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormValue {
    Text(String),
    File(String),
}

// CONTRACT
/// The snippet for `request`, built from `draft` (which gives the
/// urlencoded fields as typed).
#[must_use]
pub fn snippet(request: &Request, draft: &RequestDraft) -> Snippet {
    let body = match &request.body {
        RequestBody::Empty => SnippetBody::None,
        RequestBody::Bytes(_) if draft.body.mode == BodyMode::UrlEncoded => {
            SnippetBody::UrlEncoded(
                draft
                    .body
                    .urlencoded
                    .iter()
                    .filter(|r| r.is_active())
                    .map(|r| (r.key.clone(), r.value.clone()))
                    .collect(),
            )
        }
        RequestBody::Bytes(bytes) => SnippetBody::Raw(String::from_utf8_lossy(bytes).into_owned()),
        RequestBody::Multipart(parts) => SnippetBody::FormData(
            parts
                .iter()
                .map(|p| {
                    let value = match &p.value {
                        PartValue::Text(t) => FormValue::Text(t.clone()),
                        PartValue::File(path) => FormValue::File(path.display().to_string()),
                    };
                    (p.name.clone(), value)
                })
                .collect(),
        ),
        RequestBody::File(path) => SnippetBody::File(path.display().to_string()),
    };
    Snippet {
        method: request.method.to_string(),
        url: request.url.to_string(),
        headers: request
            .headers
            .iter()
            .map(|h| (h.name.clone(), h.value.clone()))
            .collect(),
        body,
        follow_redirects: request.options.follow_redirects,
        verify_tls: request.options.verify_tls,
    }
}

// CONTRACT
/// The languages, in menu order (F19-AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    Curl,
    Httpie,
    PythonRequests,
    JavaScriptFetch,
    NodeAxios,
    Go,
    RustReqwest,
    Java,
    CSharp,
    Php,
    Ruby,
    Swift,
}

impl Language {
    pub const ALL: [Language; 12] = [
        Self::Curl,
        Self::Httpie,
        Self::PythonRequests,
        Self::JavaScriptFetch,
        Self::NodeAxios,
        Self::Go,
        Self::RustReqwest,
        Self::Java,
        Self::CSharp,
        Self::Php,
        Self::Ruby,
        Self::Swift,
    ];

    // CONTRACT
    /// The menu label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Curl => "cURL",
            Self::Httpie => "HTTPie",
            Self::PythonRequests => "Python - Requests",
            Self::JavaScriptFetch => "JavaScript - Fetch",
            Self::NodeAxios => "Node.js - Axios",
            Self::Go => "Go - net/http",
            Self::RustReqwest => "Rust - reqwest",
            Self::Java => "Java - HttpClient",
            Self::CSharp => "C# - HttpClient",
            Self::Php => "PHP - cURL",
            Self::Ruby => "Ruby - Net::HTTP",
            Self::Swift => "Swift - URLSession",
        }
    }

    // CONTRACT
    /// `snippet` as code in this language.
    #[must_use]
    pub fn generate(self, s: &Snippet) -> String {
        match self {
            Self::Curl => curl(s),
            Self::Httpie => httpie(s),
            Self::PythonRequests => python(s),
            Self::JavaScriptFetch => fetch(s),
            Self::NodeAxios => axios(s),
            Self::Go => go(s),
            Self::RustReqwest => rust(s),
            Self::Java => java(s),
            Self::CSharp => csharp(s),
            Self::Php => php(s),
            Self::Ruby => ruby(s),
            Self::Swift => swift(s),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// --- escaping --------------------------------------------------------------------------

/// POSIX shell single quotes: `'` becomes `'\''`.
#[must_use]
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// A double-quoted string with C-style escapes; `unicode` formats a
/// control character's code point.
fn c_string(text: &str, unicode: fn(u32) -> String, extra: &[(char, &str)]) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if let Some((_, escaped)) = extra.iter().find(|(e, _)| *e == c) {
            out.push_str(escaped);
            continue;
        }
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&unicode(c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn u4(code: u32) -> String {
    format!("\\u{code:04x}")
}

fn x2(code: u32) -> String {
    format!("\\x{code:02x}")
}

fn braced(code: u32) -> String {
    format!("\\u{{{code:x}}}")
}

/// JavaScript, Java, C# and JSON-compatible strings.
fn js(text: &str) -> String {
    c_string(
        text,
        u4,
        &[('\u{2028}', "\\u2028"), ('\u{2029}', "\\u2029")],
    )
}

fn python_str(text: &str) -> String {
    c_string(text, x2, &[])
}

fn go_str(text: &str) -> String {
    c_string(text, x2, &[])
}

fn rust_str(text: &str) -> String {
    c_string(text, braced, &[])
}

fn swift_str(text: &str) -> String {
    c_string(text, braced, &[])
}

fn ruby_str(text: &str) -> String {
    c_string(text, u4, &[('#', "\\#")])
}

/// PHP single quotes: only `\` and `'` are special.
fn php_str(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// `application/x-www-form-urlencoded` text of `fields`.
fn form_urlencoded(fields: &[(String, String)]) -> String {
    let enc = |s: &str| {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                    out.push(b as char);
                }
                b' ' => out.push('+'),
                b => {
                    let _ = write!(out, "%{b:02X}");
                }
            }
        }
        out
    };
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The last path segment, for a file part's file name.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

const BOUNDARY: &str = "----FerryFormBoundary";

// --- generators ------------------------------------------------------------------------

fn curl(s: &Snippet) -> String {
    let mut args = vec![format!("curl {}", shell_quote(&s.url))];
    // cURL turns a GET with data into a POST unless told otherwise.
    if s.method != "GET" || !matches!(s.body, SnippetBody::None) {
        args.push(format!("--request {}", s.method));
    }
    if s.follow_redirects {
        args.push("--location".to_owned());
    }
    if !s.verify_tls {
        args.push("--insecure".to_owned());
    }
    for (k, v) in &s.headers {
        // `Name;` is how cURL sends an empty header; `Name:` removes one.
        let header = if v.is_empty() {
            format!("{k};")
        } else {
            format!("{k}: {v}")
        };
        args.push(format!("--header {}", shell_quote(&header)));
    }
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => args.push(format!("--data-raw {}", shell_quote(text))),
        SnippetBody::UrlEncoded(fields) => {
            for (k, v) in fields {
                args.push(format!(
                    "--data-urlencode {}",
                    shell_quote(&format!("{k}={v}"))
                ));
            }
        }
        SnippetBody::FormData(parts) => {
            for (name, value) in parts {
                args.push(match value {
                    FormValue::Text(t) => {
                        format!("--form-string {}", shell_quote(&format!("{name}={t}")))
                    }
                    FormValue::File(p) => {
                        format!("--form {}", shell_quote(&format!("{name}=@{p}")))
                    }
                });
            }
        }
        SnippetBody::File(path) => args.push(format!(
            "--data-binary {}",
            shell_quote(&format!("@{path}"))
        )),
    }
    args.join(" \\\n  ")
}

fn httpie(s: &Snippet) -> String {
    let mut first = vec!["http".to_owned()];
    if s.follow_redirects {
        first.push("--follow".to_owned());
    }
    if !s.verify_tls {
        first.push("--verify=no".to_owned());
    }
    match &s.body {
        SnippetBody::UrlEncoded(_) => first.push("--form".to_owned()),
        SnippetBody::FormData(_) => first.push("--multipart".to_owned()),
        SnippetBody::Raw(text) => first.push(format!("--raw {}", shell_quote(text))),
        SnippetBody::None | SnippetBody::File(_) => {}
    }
    first.push(s.method.clone());
    first.push(shell_quote(&s.url));
    let mut lines = vec![first.join(" ")];
    for (k, v) in &s.headers {
        lines.push(shell_quote(&format!("{k}:{v}")));
    }
    match &s.body {
        SnippetBody::UrlEncoded(fields) => {
            for (k, v) in fields {
                lines.push(shell_quote(&format!("{k}={v}")));
            }
        }
        SnippetBody::FormData(parts) => {
            for (name, value) in parts {
                lines.push(match value {
                    FormValue::Text(t) => shell_quote(&format!("{name}={t}")),
                    FormValue::File(p) => shell_quote(&format!("{name}@{p}")),
                });
            }
        }
        SnippetBody::File(path) => lines.push(format!("< {}", shell_quote(path))),
        SnippetBody::None | SnippetBody::Raw(_) => {}
    }
    lines.join(" \\\n  ")
}

fn python(s: &Snippet) -> String {
    let mut out = String::from("import requests\n\n");
    let _ = writeln!(out, "url = {}", python_str(&s.url));
    out.push_str("headers = {\n");
    for (k, v) in &s.headers {
        let _ = writeln!(out, "    {}: {},", python_str(k), python_str(v));
    }
    out.push_str("}\n");
    let mut args = vec![
        python_str(&s.method),
        "url".to_owned(),
        "headers=headers".to_owned(),
    ];
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "data = {}", python_str(text));
            args.push("data=data.encode(\"utf-8\")".to_owned());
        }
        SnippetBody::UrlEncoded(fields) => {
            out.push_str("data = [\n");
            for (k, v) in fields {
                let _ = writeln!(out, "    ({}, {}),", python_str(k), python_str(v));
            }
            out.push_str("]\n");
            args.push("data=data".to_owned());
        }
        SnippetBody::FormData(parts) => {
            out.push_str("files = [\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => {
                        writeln!(
                            out,
                            "    ({}, (None, {})),",
                            python_str(name),
                            python_str(t)
                        )
                    }
                    FormValue::File(p) => writeln!(
                        out,
                        "    ({}, ({}, open({}, \"rb\"))),",
                        python_str(name),
                        python_str(file_name(p)),
                        python_str(p)
                    ),
                };
            }
            out.push_str("]\n");
            args.push("files=files".to_owned());
        }
        SnippetBody::File(path) => {
            let _ = writeln!(out, "data = open({}, \"rb\")", python_str(path));
            args.push("data=data".to_owned());
        }
    }
    if !s.follow_redirects {
        args.push("allow_redirects=False".to_owned());
    }
    if !s.verify_tls {
        args.push("verify=False".to_owned());
    }
    let _ = write!(
        out,
        "\nresponse = requests.request({})\n\nprint(response.text)\n",
        args.join(", ")
    );
    out
}

fn fetch(s: &Snippet) -> String {
    let mut out = String::from("const headers = new Headers();\n");
    for (k, v) in &s.headers {
        let _ = writeln!(out, "headers.append({}, {});", js(k), js(v));
    }
    let body = match &s.body {
        SnippetBody::None => None,
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "\nconst body = {};", js(text));
            Some("body")
        }
        SnippetBody::UrlEncoded(fields) => {
            out.push_str("\nconst body = new URLSearchParams();\n");
            for (k, v) in fields {
                let _ = writeln!(out, "body.append({}, {});", js(k), js(v));
            }
            Some("body")
        }
        SnippetBody::FormData(parts) => {
            out.push_str("\nconst body = new FormData();\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => writeln!(out, "body.append({}, {});", js(name), js(t)),
                    FormValue::File(p) => writeln!(
                        out,
                        "body.append({}, fileInput.files[0], {}); // {}",
                        js(name),
                        js(file_name(p)),
                        p.replace('\n', " ")
                    ),
                };
            }
            Some("body")
        }
        SnippetBody::File(path) => {
            let _ = writeln!(
                out,
                "\nconst body = fileInput.files[0]; // {}",
                path.replace('\n', " ")
            );
            Some("body")
        }
    };
    out.push_str("\nconst response = await fetch(");
    out.push_str(&js(&s.url));
    let _ = write!(out, ", {{\n  method: {},\n  headers,\n", js(&s.method));
    if let Some(body) = body {
        let _ = writeln!(out, "  {body},");
    }
    let _ = writeln!(
        out,
        "  redirect: {},",
        if s.follow_redirects {
            "\"follow\""
        } else {
            "\"manual\""
        }
    );
    out.push_str("});\nconsole.log(await response.text());\n");
    if !s.verify_tls {
        out.push_str("// fetch cannot turn off certificate verification.\n");
    }
    out
}

fn axios(s: &Snippet) -> String {
    let mut out = String::from("const axios = require(\"axios\");\n");
    if matches!(s.body, SnippetBody::FormData(_) | SnippetBody::File(_)) {
        out.push_str("const fs = require(\"fs\");\n");
    }
    if matches!(s.body, SnippetBody::FormData(_)) {
        out.push_str("const FormData = require(\"form-data\");\n");
    }
    if !s.verify_tls {
        out.push_str("const https = require(\"https\");\n");
    }
    let data = match &s.body {
        SnippetBody::None => None,
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "\nconst data = {};", js(text));
            Some("data")
        }
        SnippetBody::UrlEncoded(fields) => {
            out.push_str("\nconst data = new URLSearchParams();\n");
            for (k, v) in fields {
                let _ = writeln!(out, "data.append({}, {});", js(k), js(v));
            }
            Some("data")
        }
        SnippetBody::FormData(parts) => {
            out.push_str("\nconst data = new FormData();\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => writeln!(out, "data.append({}, {});", js(name), js(t)),
                    FormValue::File(p) => {
                        writeln!(
                            out,
                            "data.append({}, fs.createReadStream({}));",
                            js(name),
                            js(p)
                        )
                    }
                };
            }
            Some("data")
        }
        SnippetBody::File(path) => {
            let _ = writeln!(out, "\nconst data = fs.readFileSync({});", js(path));
            Some("data")
        }
    };
    let _ = write!(
        out,
        "\nconst config = {{\n  method: {},\n  url: {},\n  headers: {{\n",
        js(&s.method.to_lowercase()),
        js(&s.url)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(out, "    {}: {},", js(k), js(v));
    }
    if matches!(s.body, SnippetBody::FormData(_)) {
        out.push_str("    ...data.getHeaders(),\n");
    }
    out.push_str("  },\n");
    if let Some(data) = data {
        let _ = writeln!(out, "  {data},");
    }
    if !s.follow_redirects {
        out.push_str("  maxRedirects: 0,\n");
    }
    if !s.verify_tls {
        out.push_str("  httpsAgent: new https.Agent({ rejectUnauthorized: false }),\n");
    }
    out.push_str("};\n\naxios\n  .request(config)\n  .then((response) => console.log(JSON.stringify(response.data)))\n  .catch((error) => console.log(error));\n");
    out
}

fn go(s: &Snippet) -> String {
    let mut imports = vec!["fmt", "io", "net/http"];
    match &s.body {
        SnippetBody::Raw(_) | SnippetBody::UrlEncoded(_) => imports.push("strings"),
        SnippetBody::FormData(_) => {
            imports.extend(["bytes", "mime/multipart", "os", "path/filepath"]);
        }
        SnippetBody::File(_) => imports.push("os"),
        SnippetBody::None => {}
    }
    if !s.verify_tls {
        imports.push("crypto/tls");
    }
    imports.sort_unstable();
    let mut out = String::from("package main\n\nimport (\n");
    for i in &imports {
        let _ = writeln!(out, "\t\"{i}\"");
    }
    out.push_str(")\n\nfunc main() {\n");
    let _ = writeln!(out, "\turl := {}", go_str(&s.url));
    let mut content_type = None;
    let payload = match &s.body {
        SnippetBody::None => "nil".to_owned(),
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "\tpayload := strings.NewReader({})", go_str(text));
            "payload".to_owned()
        }
        SnippetBody::UrlEncoded(fields) => {
            let _ = writeln!(
                out,
                "\tpayload := strings.NewReader({})",
                go_str(&form_urlencoded(fields))
            );
            "payload".to_owned()
        }
        SnippetBody::FormData(parts) => {
            out.push_str(
                "\tpayload := &bytes.Buffer{}\n\twriter := multipart.NewWriter(payload)\n",
            );
            for (name, value) in parts {
                match value {
                    FormValue::Text(t) => {
                        let _ = writeln!(
                            out,
                            "\t_ = writer.WriteField({}, {})",
                            go_str(name),
                            go_str(t)
                        );
                    }
                    FormValue::File(p) => {
                        let _ = write!(
                            out,
                            // A block per file, so several files can each
                            // declare `file` and `part`.
                            "\t{{\n\t\tfile, err := os.Open({p})\n\t\tif err != nil {{\n\t\t\tfmt.Println(err)\n\t\t\treturn\n\t\t}}\n\t\tpart, _ := writer.CreateFormFile({n}, filepath.Base({p}))\n\t\t_, _ = io.Copy(part, file)\n\t\tfile.Close()\n\t}}\n",
                            p = go_str(p),
                            n = go_str(name)
                        );
                    }
                }
            }
            out.push_str("\t_ = writer.Close()\n");
            content_type = Some("writer.FormDataContentType()");
            "payload".to_owned()
        }
        SnippetBody::File(path) => {
            let _ = write!(
                out,
                "\tpayload, err := os.Open({})\n\tif err != nil {{\n\t\tfmt.Println(err)\n\t\treturn\n\t}}\n\tdefer payload.Close()\n",
                go_str(path)
            );
            "payload".to_owned()
        }
    };
    out.push_str("\n\tclient := &http.Client{}\n");
    if !s.follow_redirects {
        out.push_str("\tclient.CheckRedirect = func(req *http.Request, via []*http.Request) error {\n\t\treturn http.ErrUseLastResponse\n\t}\n");
    }
    if !s.verify_tls {
        out.push_str("\tclient.Transport = &http.Transport{TLSClientConfig: &tls.Config{InsecureSkipVerify: true}}\n");
    }
    let _ = write!(
        out,
        "\treq, err := http.NewRequest({}, url, {payload})\n\tif err != nil {{\n\t\tfmt.Println(err)\n\t\treturn\n\t}}\n",
        go_str(&s.method)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(out, "\treq.Header.Add({}, {})", go_str(k), go_str(v));
    }
    if let Some(ct) = content_type {
        let _ = writeln!(out, "\treq.Header.Set(\"Content-Type\", {ct})");
    }
    out.push_str("\n\tres, err := client.Do(req)\n\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n\tdefer res.Body.Close()\n\n\tbody, err := io.ReadAll(res.Body)\n\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n\tfmt.Println(string(body))\n}\n");
    out
}

fn rust(s: &Snippet) -> String {
    let mut out = String::from(
        "#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n    let client = reqwest::Client::builder()\n",
    );
    if !s.follow_redirects {
        out.push_str("        .redirect(reqwest::redirect::Policy::none())\n");
    }
    if !s.verify_tls {
        out.push_str("        .danger_accept_invalid_certs(true)\n");
    }
    out.push_str("        .build()?;\n\n");
    let _ = write!(
        out,
        "    let request = client\n        .request(reqwest::Method::from_bytes({}.as_bytes())?, {})\n",
        rust_str(&s.method),
        rust_str(&s.url)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(out, "        .header({}, {})", rust_str(k), rust_str(v));
    }
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "        .body({})", rust_str(text));
        }
        SnippetBody::UrlEncoded(fields) => {
            out.push_str("        .form(&[\n");
            for (k, v) in fields {
                let _ = writeln!(out, "            ({}, {}),", rust_str(k), rust_str(v));
            }
            out.push_str("        ])\n");
        }
        SnippetBody::FormData(parts) => {
            out.push_str("        .multipart(\n            reqwest::multipart::Form::new()\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => {
                        writeln!(
                            out,
                            "                .text({}, {})",
                            rust_str(name),
                            rust_str(t)
                        )
                    }
                    FormValue::File(p) => writeln!(
                        out,
                        "                .part(\n                    {},\n                    reqwest::multipart::Part::bytes(std::fs::read({})?)\n                        .file_name({}),\n                )",
                        rust_str(name),
                        rust_str(p),
                        rust_str(file_name(p))
                    ),
                };
            }
            out.push_str("        )\n");
        }
        SnippetBody::File(path) => {
            let _ = writeln!(out, "        .body(std::fs::read({})?)", rust_str(path));
        }
    }
    out.push_str("        ;\n\n    let response = request.send().await?;\n    println!(\"{}\", response.text().await?);\n    Ok(())\n}\n");
    out.replace("\n        ;\n", ";\n")
}

/// The multipart body built in code by the languages that have no helper:
/// lines of `(text, is_file)`, each to be appended.
fn multipart_pieces(parts: &[(String, FormValue)]) -> Vec<(String, Option<String>)> {
    let mut pieces = Vec::new();
    for (name, value) in parts {
        match value {
            FormValue::Text(t) => pieces.push((
                format!(
                    "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{t}\r\n"
                ),
                None,
            )),
            FormValue::File(p) => {
                pieces.push((
                    format!(
                        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\r\n\r\n",
                        file_name(p)
                    ),
                    None,
                ));
                pieces.push((String::new(), Some(p.clone())));
                pieces.push(("\r\n".to_owned(), None));
            }
        }
    }
    pieces.push((format!("--{BOUNDARY}--\r\n"), None));
    pieces
}

fn java(s: &Snippet) -> String {
    let mut out = String::from(
        "import java.net.URI;\nimport java.net.http.HttpClient;\nimport java.net.http.HttpRequest;\nimport java.net.http.HttpResponse;\n",
    );
    if matches!(s.body, SnippetBody::FormData(_) | SnippetBody::File(_)) {
        out.push_str("import java.nio.file.Files;\nimport java.nio.file.Path;\n");
    }
    out.push_str(
        "\npublic class Main {\n    public static void main(String[] args) throws Exception {\n",
    );
    let _ = writeln!(
        out,
        "        HttpClient client = HttpClient.newBuilder()\n            .followRedirects(HttpClient.Redirect.{})\n            .build();",
        if s.follow_redirects {
            "NORMAL"
        } else {
            "NEVER"
        }
    );
    if !s.verify_tls {
        out.push_str("        // HttpClient needs an SSLContext that trusts all certificates to skip verification.\n");
    }
    let publisher = match &s.body {
        SnippetBody::None => "HttpRequest.BodyPublishers.noBody()".to_owned(),
        SnippetBody::Raw(text) => format!("HttpRequest.BodyPublishers.ofString({})", js(text)),
        SnippetBody::UrlEncoded(fields) => format!(
            "HttpRequest.BodyPublishers.ofString({})",
            js(&form_urlencoded(fields))
        ),
        SnippetBody::FormData(parts) => {
            out.push_str("        var body = new java.io.ByteArrayOutputStream();\n");
            for (text, file) in multipart_pieces(parts) {
                let _ = match file {
                    Some(p) => writeln!(
                        out,
                        "        body.writeBytes(Files.readAllBytes(Path.of({})));",
                        js(&p)
                    ),
                    None => writeln!(
                        out,
                        "        body.writeBytes({}.getBytes(java.nio.charset.StandardCharsets.UTF_8));",
                        js(&text)
                    ),
                };
            }
            "HttpRequest.BodyPublishers.ofByteArray(body.toByteArray())".to_owned()
        }
        SnippetBody::File(path) => {
            format!("HttpRequest.BodyPublishers.ofFile(Path.of({}))", js(path))
        }
    };
    let _ = writeln!(
        out,
        "        HttpRequest request = HttpRequest.newBuilder()\n            .uri(URI.create({}))",
        js(&s.url)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(out, "            .header({}, {})", js(k), js(v));
    }
    if matches!(s.body, SnippetBody::FormData(_)) {
        let _ = writeln!(
            out,
            "            .header(\"Content-Type\", \"multipart/form-data; boundary={BOUNDARY}\")"
        );
    }
    let _ = writeln!(
        out,
        "            .method({}, {publisher})\n            .build();",
        js(&s.method)
    );
    out.push_str("        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());\n        System.out.println(response.body());\n    }\n}\n");
    out
}

fn csharp(s: &Snippet) -> String {
    let mut out = String::from("var handler = new HttpClientHandler();\n");
    if !s.follow_redirects {
        out.push_str("handler.AllowAutoRedirect = false;\n");
    }
    if !s.verify_tls {
        out.push_str("handler.ServerCertificateCustomValidationCallback = HttpClientHandler.DangerousAcceptAnyServerCertificateValidator;\n");
    }
    let _ = writeln!(
        out,
        "var client = new HttpClient(handler);\nvar request = new HttpRequestMessage(new HttpMethod({}), {});",
        js(&s.method),
        js(&s.url)
    );
    let content_type = s
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone());
    for (k, v) in s
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type"))
    {
        let _ = writeln!(
            out,
            "request.Headers.TryAddWithoutValidation({}, {});",
            js(k),
            js(v)
        );
    }
    let content = match &s.body {
        SnippetBody::None => None,
        SnippetBody::Raw(text) => Some(format!("new StringContent({})", js(text))),
        SnippetBody::UrlEncoded(fields) => {
            out.push_str("var fields = new List<KeyValuePair<string, string>>\n{\n");
            for (k, v) in fields {
                let _ = writeln!(out, "    new({}, {}),", js(k), js(v));
            }
            out.push_str("};\n");
            Some("new FormUrlEncodedContent(fields)".to_owned())
        }
        SnippetBody::FormData(parts) => {
            out.push_str("var form = new MultipartFormDataContent();\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => {
                        writeln!(out, "form.Add(new StringContent({}), {});", js(t), js(name))
                    }
                    FormValue::File(p) => writeln!(
                        out,
                        "form.Add(new ByteArrayContent(File.ReadAllBytes({})), {}, {});",
                        js(p),
                        js(name),
                        js(file_name(p))
                    ),
                };
            }
            Some("form".to_owned())
        }
        SnippetBody::File(path) => Some(format!(
            "new ByteArrayContent(File.ReadAllBytes({}))",
            js(path)
        )),
    };
    if let Some(content) = content {
        let _ = writeln!(out, "request.Content = {content};");
        if let Some(ct) = content_type
            && !matches!(s.body, SnippetBody::FormData(_))
        {
            let _ = writeln!(
                out,
                "request.Content.Headers.ContentType = System.Net.Http.Headers.MediaTypeHeaderValue.Parse({});",
                js(&ct)
            );
        }
    }
    out.push_str("var response = await client.SendAsync(request);\nConsole.WriteLine(await response.Content.ReadAsStringAsync());\n");
    out
}

fn php(s: &Snippet) -> String {
    let mut out = String::from("<?php\n\n$curl = curl_init();\n\ncurl_setopt_array($curl, [\n");
    let _ = writeln!(out, "    CURLOPT_URL => {},", php_str(&s.url));
    out.push_str("    CURLOPT_RETURNTRANSFER => true,\n");
    let _ = writeln!(
        out,
        "    CURLOPT_FOLLOWLOCATION => {},",
        if s.follow_redirects { "true" } else { "false" }
    );
    let _ = writeln!(out, "    CURLOPT_CUSTOMREQUEST => {},", php_str(&s.method));
    if !s.verify_tls {
        out.push_str("    CURLOPT_SSL_VERIFYPEER => false,\n    CURLOPT_SSL_VERIFYHOST => 0,\n");
    }
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "    CURLOPT_POSTFIELDS => {},", php_str(text));
        }
        SnippetBody::UrlEncoded(fields) => {
            let _ = writeln!(
                out,
                "    CURLOPT_POSTFIELDS => {},",
                php_str(&form_urlencoded(fields))
            );
        }
        SnippetBody::FormData(parts) => {
            out.push_str("    CURLOPT_POSTFIELDS => [\n");
            for (name, value) in parts {
                let _ = match value {
                    FormValue::Text(t) => {
                        writeln!(out, "        {} => {},", php_str(name), php_str(t))
                    }
                    FormValue::File(p) => writeln!(
                        out,
                        "        {} => new CURLFile({}),",
                        php_str(name),
                        php_str(p)
                    ),
                };
            }
            out.push_str("    ],\n");
        }
        SnippetBody::File(path) => {
            let _ = writeln!(
                out,
                "    CURLOPT_POSTFIELDS => file_get_contents({}),",
                php_str(path)
            );
        }
    }
    if !s.headers.is_empty() {
        out.push_str("    CURLOPT_HTTPHEADER => [\n");
        for (k, v) in &s.headers {
            let _ = writeln!(out, "        {},", php_str(&format!("{k}: {v}")));
        }
        out.push_str("    ],\n");
    }
    out.push_str("]);\n\n$response = curl_exec($curl);\ncurl_close($curl);\necho $response;\n");
    out
}

fn ruby(s: &Snippet) -> String {
    let mut out = String::from("require \"uri\"\nrequire \"net/http\"\n");
    if !s.verify_tls {
        out.push_str("require \"openssl\"\n");
    }
    let _ = write!(
        out,
        "\nurl = URI({})\nhttp = Net::HTTP.new(url.host, url.port)\nhttp.use_ssl = url.scheme == \"https\"\n",
        ruby_str(&s.url)
    );
    if !s.verify_tls {
        out.push_str("http.verify_mode = OpenSSL::SSL::VERIFY_NONE\n");
    }
    let has_body = !matches!(s.body, SnippetBody::None);
    let _ = writeln!(
        out,
        "\nrequest = Net::HTTPGenericRequest.new({}, {has_body}, true, url)",
        ruby_str(&s.method)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(out, "request[{}] = {}", ruby_str(k), ruby_str(v));
    }
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => {
            let _ = writeln!(out, "request.body = {}", ruby_str(text));
        }
        SnippetBody::UrlEncoded(fields) => {
            let pairs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("[{}, {}]", ruby_str(k), ruby_str(v)))
                .collect();
            let _ = writeln!(
                out,
                "request.set_form([{}], \"application/x-www-form-urlencoded\")",
                pairs.join(", ")
            );
        }
        SnippetBody::FormData(parts) => {
            let pairs: Vec<String> = parts
                .iter()
                .map(|(name, value)| match value {
                    FormValue::Text(t) => format!("[{}, {}]", ruby_str(name), ruby_str(t)),
                    FormValue::File(p) => {
                        format!("[{}, File.open({})]", ruby_str(name), ruby_str(p))
                    }
                })
                .collect();
            let _ = writeln!(
                out,
                "request.set_form([{}], \"multipart/form-data\")",
                pairs.join(", ")
            );
        }
        SnippetBody::File(path) => {
            let _ = writeln!(out, "request.body = File.binread({})", ruby_str(path));
        }
    }
    out.push_str("\nresponse = http.request(request)\nputs response.read_body\n");
    if s.follow_redirects {
        out.push_str("# Net::HTTP does not follow redirects; check response[\"location\"].\n");
    }
    out
}

fn swift(s: &Snippet) -> String {
    let mut out = String::from(
        "import Foundation\n#if canImport(FoundationNetworking)\nimport FoundationNetworking\n#endif\n\n",
    );
    let _ = writeln!(
        out,
        "var request = URLRequest(url: URL(string: {})!, timeoutInterval: Double.infinity)",
        swift_str(&s.url)
    );
    for (k, v) in &s.headers {
        let _ = writeln!(
            out,
            "request.addValue({}, forHTTPHeaderField: {})",
            swift_str(v),
            swift_str(k)
        );
    }
    let _ = writeln!(out, "request.httpMethod = {}", swift_str(&s.method));
    match &s.body {
        SnippetBody::None => {}
        SnippetBody::Raw(text) => {
            let _ = writeln!(
                out,
                "request.httpBody = {}.data(using: .utf8)",
                swift_str(text)
            );
        }
        SnippetBody::UrlEncoded(fields) => {
            let _ = writeln!(
                out,
                "request.httpBody = {}.data(using: .utf8)",
                swift_str(&form_urlencoded(fields))
            );
        }
        SnippetBody::FormData(parts) => {
            out.push_str("var body = Data()\n");
            for (text, file) in multipart_pieces(parts) {
                let _ = match file {
                    Some(p) => writeln!(
                        out,
                        "body.append(try Data(contentsOf: URL(fileURLWithPath: {})))",
                        swift_str(&p)
                    ),
                    None => writeln!(out, "body.append({}.data(using: .utf8)!)", swift_str(&text)),
                };
            }
            let _ = writeln!(
                out,
                "request.addValue(\"multipart/form-data; boundary={BOUNDARY}\", forHTTPHeaderField: \"Content-Type\")\nrequest.httpBody = body"
            );
        }
        SnippetBody::File(path) => {
            let _ = writeln!(
                out,
                "request.httpBody = try Data(contentsOf: URL(fileURLWithPath: {}))",
                swift_str(path)
            );
        }
    }
    if !s.follow_redirects || !s.verify_tls {
        out.push_str("// Redirect and certificate handling need a URLSessionDelegate.\n");
    }
    out.push_str("\nlet task = URLSession.shared.dataTask(with: request) { data, response, error in\n    guard let data = data else {\n        print(String(describing: error))\n        return\n    }\n    print(String(data: data, encoding: .utf8)!)\n}\ntask.resume()\n");
    out
}
