//! Reading a cURL command into a request (F11-AC1).

use std::fmt;
use std::fmt::Write as _;

use crate::auth::AuthKind;
use crate::body::{BodyMode, FormField, RawLanguage};
use crate::draft::RequestDraft;
use crate::kv::KeyValue;
use crate::params;
use crate::request::Method;

// CONTRACT
/// Why a cURL command could not be read. `Display` is the text shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurlError(pub String);

impl fmt::Display for CurlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Could not read the cURL command: {}", self.0)
    }
}

impl std::error::Error for CurlError {}

fn error(reason: &str) -> CurlError {
    CurlError(reason.to_owned())
}

// CONTRACT
/// Whether `text` looks like a cURL command (starts with `curl `).
#[must_use]
pub fn is_curl(text: &str) -> bool {
    let t = text.trim_start();
    t.strip_prefix("curl")
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// Split a command into words: Windows `cmd` text (carets) or POSIX shell.
fn words(text: &str) -> Result<Vec<String>, CurlError> {
    if is_cmd(text) {
        Ok(windows_words(&strip_carets(text)))
    } else {
        posix_words(text)
    }
}

/// Whether the text is in `cmd` style: a caret before a line break, or a
/// caret-quote `^"` outside what a POSIX shell would read as quoted or
/// escaped.
fn is_cmd(text: &str) -> bool {
    if text.contains("^\n") || text.contains("^\r\n") {
        return true;
    }
    let (mut single, mut double) = (false, false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '\\' if !single => {
                chars.next();
            }
            '^' if !single && !double && chars.peek() == Some(&'"') => return true,
            _ => {}
        }
    }
    false
}

/// `cmd` escapes: `^` before a line break continues the line, before
/// anything else it is dropped.
fn strip_carets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' {
            match chars.next() {
                Some('\r') if chars.peek() == Some(&'\n') => {
                    chars.next();
                }
                Some('\n' | '\r') | None => {}
                Some(other) => out.push(other),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Arguments as a Windows program splits them: double quotes group, and
/// backslashes are literal unless they come before a quote.
fn windows_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut n = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    n += 1;
                }
                if chars.peek() == Some(&'"') {
                    chars.next();
                    word.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        word.push('"');
                    } else {
                        quoted = !quoted;
                    }
                } else {
                    word.extend(std::iter::repeat_n('\\', n));
                }
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            other => {
                word.push(other);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    out
}

/// POSIX shell words: single and double quotes, `$'…'`, backslash
/// escapes and continuations.
fn posix_words(text: &str) -> Result<Vec<String>, CurlError> {
    let cmd = false;
    let mut out = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '^' if cmd => match chars.next() {
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some('\n') | None => {}
                Some(other) => {
                    word.push(other);
                    started = true;
                }
            },
            // A continuation whose line break was lost: one-line text
            // fields drop line breaks from pasted text.
            '\\' if !cmd && !started && matches!(chars.peek(), Some(' ' | '\t')) => {}
            '\\' if !cmd => match chars.next() {
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                Some('\n') => {}
                Some(other) => {
                    word.push(other);
                    started = true;
                }
                None => word.push('\\'),
            },
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => word.push(ch),
                        None => return Err(error("a quote is not closed")),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                started = true;
                ansi_c(&mut chars, &mut word)?;
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') if !cmd => match chars.next() {
                            Some(e @ ('"' | '\\' | '$' | '`')) => word.push(e),
                            Some('\n') => {}
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(error("a quote is not closed")),
                        },
                        Some('^') if cmd => {
                            if let Some(next) = chars.next() {
                                word.push(next);
                            }
                        }
                        Some(ch) => word.push(ch),
                        None => return Err(error("a quote is not closed")),
                    }
                }
            }
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            other => {
                word.push(other);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    Ok(out)
}

/// The rest of a `$'…'` string.
fn ansi_c(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    word: &mut String,
) -> Result<(), CurlError> {
    let hex = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>, max: usize| {
        let mut digits = String::new();
        while digits.len() < max {
            match chars.peek() {
                Some(c) if c.is_ascii_hexdigit() => {
                    digits.push(*c);
                    chars.next();
                }
                _ => break,
            }
        }
        u32::from_str_radix(&digits, 16)
            .ok()
            .and_then(char::from_u32)
    };
    loop {
        match chars.next() {
            Some('\'') => return Ok(()),
            Some('\\') => match chars.next() {
                Some('n') => word.push('\n'),
                Some('t') => word.push('\t'),
                Some('r') => word.push('\r'),
                Some('0') => word.push('\0'),
                Some('x') => word.extend(hex(chars, 2)),
                Some('u') => word.extend(hex(chars, 4)),
                Some('U') => word.extend(hex(chars, 8)),
                Some(other) => word.push(other),
                None => return Err(error("a quote is not closed")),
            },
            Some(c) => word.push(c),
            None => return Err(error("a quote is not closed")),
        }
    }
}

/// Options that take a value, so it is never read as the URL.
const WITH_VALUE: &[&str] = &[
    "-X",
    "--request",
    "-H",
    "--header",
    "-d",
    "--data",
    "--data-raw",
    "--data-ascii",
    "--data-binary",
    "--data-urlencode",
    "--json",
    "-F",
    "--form",
    "--form-string",
    "-u",
    "--user",
    "-b",
    "--cookie",
    "-A",
    "--user-agent",
    "-e",
    "--referer",
    "--url",
    "-o",
    "--output",
    "-w",
    "--write-out",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-x",
    "--proxy",
    "-U",
    "--proxy-user",
    "--cacert",
    "--capath",
    "-E",
    "--cert",
    "--key",
    "--cert-type",
    "--key-type",
    "--pass",
    "-c",
    "--cookie-jar",
    "--resolve",
    "--connect-to",
    "-r",
    "--range",
    "-T",
    "--upload-file",
    "--retry",
    "--retry-delay",
    "--max-redirs",
    "-K",
    "--config",
    "--interface",
    "--limit-rate",
    "-y",
    "--speed-time",
    "-Y",
    "--speed-limit",
    "--oauth2-bearer",
    "--aws-sigv4",
    "--variable",
    "--expand-url",
];

/// A data piece and how curl treats it.
enum Data {
    /// `-d`, `--data`, `--data-ascii`: `@file` reads a file.
    Plain(String),
    /// `--data-raw`: never a file.
    Raw(String),
    Binary(String),
    UrlEncode(String),
    Json(String),
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match text
                    .get(i + 1..i + 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `name=content` as `--data-urlencode` sends it.
fn urlencode_piece(piece: &str) -> String {
    match piece.split_once('=') {
        Some(("", content)) => percent_encode(content),
        Some((name, content)) => format!("{name}={}", percent_encode(content)),
        None => percent_encode(piece),
    }
}

fn language(content_type: Option<&str>, text: &str) -> RawLanguage {
    let ct = content_type.unwrap_or_default().to_ascii_lowercase();
    if ct.contains("json")
        || (ct.is_empty() && serde_json::from_str::<serde_json::Value>(text).is_ok())
    {
        RawLanguage::Json
    } else if ct.contains("xml") {
        RawLanguage::Xml
    } else if ct.contains("html") {
        RawLanguage::Html
    } else if ct.contains("javascript") {
        RawLanguage::JavaScript
    } else {
        RawLanguage::Text
    }
}

// CONTRACT
/// The request a cURL command describes (F11-AC1).
///
/// # Errors
/// [`CurlError`] for text that is not a cURL command, has an unclosed
/// quote, or names no URL.
#[allow(clippy::too_many_lines)]
pub fn parse(text: &str) -> Result<RequestDraft, CurlError> {
    if !is_curl(text) {
        return Err(error("it does not start with curl"));
    }
    let words = words(text)?;
    let mut draft = RequestDraft::default();
    draft.options.follow_redirects = false;
    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KeyValue> = Vec::new();
    let mut data: Vec<Data> = Vec::new();
    let mut form: Vec<FormField> = Vec::new();
    let mut get = false;
    let mut head = false;
    let mut i = 1;
    while i < words.len() {
        let w = &words[i];
        i += 1;
        // `-Hvalue` and `-XPOST`: a short option with its value attached.
        let after_letter = w.char_indices().nth(2).map(|(at, _)| at);
        let (opt, attached) = if let Some(at) = after_letter
            && w.starts_with('-')
            && !w.starts_with("--")
        {
            let (o, rest) = w.split_at(at);
            if WITH_VALUE.contains(&o) {
                (o.to_owned(), Some(rest.to_owned()))
            } else {
                // Combined flags such as `-sSLk`.
                for flag in w[1..].chars() {
                    match flag {
                        'L' => draft.options.follow_redirects = true,
                        'k' => draft.options.verify_tls = false,
                        'G' => get = true,
                        'I' => head = true,
                        _ => {}
                    }
                }
                continue;
            }
        } else {
            (w.clone(), None)
        };
        let takes_value = WITH_VALUE.contains(&opt.as_str());
        let value = if takes_value {
            if let Some(v) = attached {
                v
            } else {
                let Some(v) = words.get(i) else {
                    return Err(CurlError(format!("{opt} needs a value")));
                };
                i += 1;
                v.clone()
            }
        } else {
            String::new()
        };
        match opt.as_str() {
            "-X" | "--request" => method = Some(value),
            "-H" | "--header" => {
                if let Some((k, v)) = value.split_once(':') {
                    if !v.trim().is_empty() {
                        headers.push(KeyValue::new(k.trim(), v.trim_start()));
                    }
                } else if let Some(k) = value.strip_suffix(';') {
                    headers.push(KeyValue::new(k.trim(), ""));
                }
            }
            "-d" | "--data" | "--data-ascii" => data.push(Data::Plain(value)),
            "--data-raw" => data.push(Data::Raw(value)),
            "--data-binary" => data.push(Data::Binary(value)),
            "--data-urlencode" => data.push(Data::UrlEncode(value)),
            "--json" => data.push(Data::Json(value)),
            "-F" | "--form" | "--form-string" => {
                let (name, v) = value.split_once('=').unwrap_or((value.as_str(), ""));
                let file = (opt != "--form-string")
                    .then(|| v.strip_prefix('@').or_else(|| v.strip_prefix('<')))
                    .flatten()
                    .map(|p| {
                        p.split(';')
                            .next()
                            .unwrap_or(p)
                            .trim_matches('"')
                            .to_owned()
                    });
                form.push(match file {
                    Some(path) => FormField {
                        row: KeyValue::new(name, ""),
                        file: Some(path.into()),
                    },
                    None => FormField {
                        row: KeyValue::new(name, v),
                        file: None,
                    },
                });
            }
            "-u" | "--user" => {
                let (user, pass) = value.split_once(':').unwrap_or((value.as_str(), ""));
                draft.auth.kind = AuthKind::Basic;
                user.clone_into(&mut draft.auth.username);
                pass.clone_into(&mut draft.auth.password);
            }
            "-b" | "--cookie" if value.contains('=') => {
                headers.push(KeyValue::new("Cookie", value));
            }
            "-A" | "--user-agent" => headers.push(KeyValue::new("User-Agent", value)),
            "-e" | "--referer" => headers.push(KeyValue::new("Referer", value)),
            "--url" => url = url.or(Some(value)),
            "-L" | "--location" => draft.options.follow_redirects = true,
            "-k" | "--insecure" => draft.options.verify_tls = false,
            "-G" | "--get" => get = true,
            "-I" | "--head" => head = true,
            o if o.starts_with('-') => {}
            _ => url = url.or(Some(opt.clone())),
        }
    }
    let mut url = url.ok_or_else(|| error("it names no URL"))?;
    let content_type = headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.clone());
    let has_json = data.iter().any(|d| matches!(d, Data::Json(_)));
    if has_json {
        for (name, value) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ] {
            if !headers.iter().any(|h| h.key.eq_ignore_ascii_case(name)) {
                headers.push(KeyValue::new(name, value));
            }
        }
    }
    if get && !data.is_empty() {
        let query: Vec<String> = data
            .iter()
            .map(|d| match d {
                Data::UrlEncode(p) => urlencode_piece(p),
                Data::Plain(p) | Data::Raw(p) | Data::Binary(p) | Data::Json(p) => p.clone(),
            })
            .collect();
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&query.join("&"));
        data.clear();
    }
    if !form.is_empty() {
        draft.body.mode = BodyMode::FormData;
        draft.body.form = form;
    } else if let [Data::Binary(p) | Data::Plain(p)] = data.as_slice()
        && let Some(path) = p.strip_prefix('@')
    {
        draft.body.mode = BodyMode::Binary;
        draft.body.binary = Some(path.into());
    } else if !data.is_empty() {
        let urlencoded_type = content_type
            .as_deref()
            .is_none_or(|ct| ct.to_ascii_lowercase().contains("x-www-form-urlencoded"));
        let fields: Option<Vec<KeyValue>> = (urlencoded_type && !has_json)
            .then(|| {
                data.iter()
                    .map(|d| match d {
                        Data::UrlEncode(p) => {
                            let (k, v) = p.split_once('=').unwrap_or(("", p.as_str()));
                            Some(vec![KeyValue::new(k, v)])
                        }
                        Data::Plain(p) | Data::Raw(p) | Data::Binary(p) => p
                            .split('&')
                            .map(|pair| {
                                pair.split_once('=').map(|(k, v)| {
                                    KeyValue::new(percent_decode(k), percent_decode(v))
                                })
                            })
                            .collect(),
                        Data::Json(_) => None,
                    })
                    .collect::<Option<Vec<Vec<KeyValue>>>>()
                    .map(|v| v.into_iter().flatten().collect())
            })
            .flatten();
        if let Some(fields) = fields {
            draft.body.mode = BodyMode::UrlEncoded;
            draft.body.urlencoded = fields;
        } else {
            let text: Vec<String> = data
                .iter()
                .map(|d| match d {
                    Data::UrlEncode(p) => urlencode_piece(p),
                    Data::Plain(p) | Data::Raw(p) | Data::Binary(p) | Data::Json(p) => p.clone(),
                })
                .collect();
            let text = text.join("&");
            draft.body.mode = BodyMode::Raw;
            draft.body.language = language(content_type.as_deref(), &text);
            draft.body.raw = text;
        }
    }
    let has_body = draft.body.mode != BodyMode::None;
    let method = match method {
        Some(m) => m,
        None if head => "HEAD".to_owned(),
        None if has_body => "POST".to_owned(),
        None => "GET".to_owned(),
    };
    draft.method = Method::parse(method.trim()).map_err(|e| CurlError(e.to_string()))?;
    draft.params = params::rows_from_url(&url, &[]);
    draft.url = url;
    draft.headers = headers;
    Ok(draft)
}
