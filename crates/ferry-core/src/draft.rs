//! The request as edited in a tab, and how it becomes a [`Request`]
//! (F01..F05).

use std::fmt;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthDraft, AuthEffect};
use crate::body::{BodyDraft, BodyMode, guess_content_type};
use crate::kv::KeyValue;
use crate::params::Param;
use crate::request::{
    Method, Part, PartValue, Request, RequestBody, SendOptions, UrlError, is_tchar, parse_url,
};
use crate::response::Header;

/// `User-Agent` the client sends unless overridden.
pub const USER_AGENT: &str = concat!("ferry/", env!("CARGO_PKG_VERSION"));
/// Value shown for headers the transport fills in.
pub const CALCULATED: &str = "<calculated when request is sent>";

// CONTRACT
/// Everything the user edits about one request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestDraft {
    pub method: Method,
    pub url: String,
    pub params: Vec<Param>,
    pub headers: Vec<KeyValue>,
    pub body: BodyDraft,
    pub auth: AuthDraft,
    pub options: SendOptions,
}

// CONTRACT
/// Why a draft cannot be sent. `Display` is the exact text shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    Url(UrlError),
    /// A header name that is not an RFC 9110 token (F02-AC2).
    HeaderName {
        name: String,
        invalid: char,
    },
    /// A header value with CR or LF (F02-AC2).
    HeaderLineBreak {
        name: String,
    },
    /// A header value with another control character.
    HeaderValue {
        name: String,
        invalid: char,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(e) => e.fmt(f),
            Self::HeaderName { name, invalid } => {
                write!(
                    f,
                    "Header name \"{name}\" contains invalid character {invalid:?}"
                )
            }
            Self::HeaderLineBreak { name } => {
                write!(f, "Header \"{name}\" value cannot contain line breaks")
            }
            Self::HeaderValue { name, invalid } => write!(
                f,
                "Header \"{name}\" value contains invalid character {}",
                invalid.escape_unicode()
            ),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<UrlError> for BuildError {
    fn from(e: UrlError) -> Self {
        Self::Url(e)
    }
}

// CONTRACT
/// A header Ferry adds on its own, as listed in the Headers tab (F02-AC3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoHeader {
    pub name: String,
    pub value: String,
    /// An enabled user header with the same name replaces this one.
    pub overridden: bool,
}

impl RequestDraft {
    // CONTRACT
    /// Enabled header rows with a non-blank key.
    pub fn active_headers(&self) -> impl Iterator<Item = &KeyValue> {
        self.headers.iter().filter(|h| h.is_active())
    }

    fn has_user_header(&self, name: &str) -> bool {
        self.active_headers()
            .any(|h| h.key.trim().eq_ignore_ascii_case(name))
    }

    // CONTRACT
    /// The headers Ferry adds, in display order (F02-AC3).
    #[must_use]
    pub fn auto_headers(&self) -> Vec<AutoHeader> {
        let mut out: Vec<(String, String)> = Vec::new();
        if let Some(content_type) = self.body.content_type() {
            out.push(("Content-Type".to_owned(), content_type));
        }
        if let Some(AuthEffect::Header { name, value }) = self.auth.effect() {
            out.push((name, value));
        }
        if self.body.is_set() {
            out.push(("Content-Length".to_owned(), CALCULATED.to_owned()));
        }
        out.push(("Host".to_owned(), CALCULATED.to_owned()));
        out.push(("User-Agent".to_owned(), USER_AGENT.to_owned()));
        out.push(("Accept".to_owned(), "*/*".to_owned()));
        out.push((
            "Accept-Encoding".to_owned(),
            "gzip, deflate, br, zstd".to_owned(),
        ));
        out.into_iter()
            .map(|(name, value)| AutoHeader {
                overridden: self.has_user_header(&name),
                name,
                value,
            })
            .collect()
    }

    // CONTRACT
    /// The request this draft sends.
    ///
    /// # Errors
    /// [`BuildError`] for an invalid URL or header.
    pub fn build(&self) -> Result<Request, BuildError> {
        let mut url = parse_url(&self.url)?;
        let auth = self.auth.effect();
        if let Some(AuthEffect::Query { key, value }) = &auth {
            url.query_pairs_mut().append_pair(key, value);
        }

        let mut headers = Vec::new();
        for row in self.active_headers() {
            headers.push(header(row.key.trim(), &row.value)?);
        }
        let (body, content_type) = self.body_to_send();
        if let Some(content_type) = content_type
            && !self.has_user_header("Content-Type")
        {
            headers.push(header("Content-Type", &content_type)?);
        }
        if let Some(AuthEffect::Header { name, value }) = &auth
            && !self.has_user_header(name)
        {
            headers.push(header(name, value)?);
        }

        Ok(Request {
            method: self.method.clone(),
            url,
            headers,
            body,
            options: self.options,
        })
    }

    /// The body and the `Content-Type` to add for it. Multipart bodies get
    /// theirs, with the boundary, from the transport.
    fn body_to_send(&self) -> (RequestBody, Option<String>) {
        let body = &self.body;
        if !body.is_set() {
            return (RequestBody::Empty, None);
        }
        match body.mode {
            BodyMode::None => (RequestBody::Empty, None),
            BodyMode::Raw => (
                RequestBody::Bytes(Bytes::from(body.raw.clone())),
                Some(body.language.content_type().to_owned()),
            ),
            BodyMode::UrlEncoded => {
                let mut form = url::form_urlencoded::Serializer::new(String::new());
                for row in body.urlencoded.iter().filter(|r| r.is_active()) {
                    form.append_pair(&row.key, &row.value);
                }
                (
                    RequestBody::Bytes(Bytes::from(form.finish())),
                    Some("application/x-www-form-urlencoded".to_owned()),
                )
            }
            BodyMode::FormData => {
                let parts = body
                    .form
                    .iter()
                    .filter(|f| f.row.is_active())
                    .map(|f| Part {
                        name: f.row.key.clone(),
                        value: match &f.file {
                            Some(path) => PartValue::File(path.clone()),
                            None => PartValue::Text(f.row.value.clone()),
                        },
                    })
                    .collect();
                (RequestBody::Multipart(parts), None)
            }
            BodyMode::Binary => match &body.binary {
                Some(path) => (
                    RequestBody::File(path.clone()),
                    Some(guess_content_type(path).to_owned()),
                ),
                None => (RequestBody::Empty, None),
            },
        }
    }
}

/// A validated header (F02-AC2).
fn header(name: &str, value: &str) -> Result<Header, BuildError> {
    if let Some(invalid) = name.chars().find(|&c| !is_tchar(c)) {
        return Err(BuildError::HeaderName {
            name: name.to_owned(),
            invalid,
        });
    }
    if value.contains(['\r', '\n']) {
        return Err(BuildError::HeaderLineBreak {
            name: name.to_owned(),
        });
    }
    if let Some(invalid) = value.chars().find(|&c| c.is_ascii_control() && c != '\t') {
        return Err(BuildError::HeaderValue {
            name: name.to_owned(),
            invalid,
        });
    }
    Ok(Header {
        name: name.to_owned(),
        value: value.to_owned(),
    })
}
