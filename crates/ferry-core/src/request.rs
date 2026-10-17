//! Requests as the user builds them (F01) and as they are sent (F02..F05).

use std::fmt;
use std::path::PathBuf;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::response::Header;

// CONTRACT
/// An HTTP method. Standard methods have their own variants; anything else
/// that is a valid RFC 9110 token is kept verbatim as `Custom`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Custom(String),
}

// CONTRACT
/// Why a method name was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MethodError {
    #[error("Method cannot be empty")]
    Empty,
    #[error("Method contains invalid character {0:?}")]
    InvalidChar(char),
}

impl Method {
    // CONTRACT
    /// The standard methods, in menu order.
    pub const STANDARD: &'static [Method] = &[
        Method::Get,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Head,
        Method::Options,
    ];

    // CONTRACT
    /// Parse a method name. Surrounding whitespace is ignored. Standard
    /// names match case-insensitively; other names must be RFC 9110 tokens
    /// and keep their case.
    ///
    /// # Errors
    /// [`MethodError`] if the name is empty or not a token.
    pub fn parse(name: &str) -> Result<Self, MethodError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MethodError::Empty);
        }
        if let Some(m) = Self::STANDARD
            .iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(name))
        {
            return Ok(m.clone());
        }
        match name.chars().find(|&c| !is_tchar(c)) {
            Some(c) => Err(MethodError::InvalidChar(c)),
            None => Ok(Self::Custom(name.to_owned())),
        }
    }

    // CONTRACT
    /// The method name as sent on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Custom(name) => name,
        }
    }
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// RFC 9110 `tchar`: the characters allowed in a token.
pub(crate) fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

impl TryFrom<String> for Method {
    type Error = MethodError;

    fn try_from(name: String) -> Result<Self, MethodError> {
        Self::parse(&name)
    }
}

impl From<Method> for String {
    fn from(method: Method) -> String {
        method.as_str().to_owned()
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// CONTRACT
/// The URL was not an absolute `http` or `https` URL with a host (F01-AC5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Enter a URL starting with http:// or https://")]
pub struct UrlError;

// CONTRACT
/// Parse user input as a request URL. Surrounding whitespace is ignored.
///
/// # Errors
/// [`UrlError`] unless the input is an absolute `http`/`https` URL with a host.
pub fn parse_url(input: &str) -> Result<url::Url, UrlError> {
    let input = input.trim();
    // The WHATWG parser forgives `https:///x` and `http:/x` (treating them as
    // `https://x`); require the literal prefix the error message promises.
    let rest = strip_prefix_ignore_case(input, "http://")
        .or_else(|| strip_prefix_ignore_case(input, "https://"))
        .ok_or(UrlError)?;
    if rest.starts_with(['/', '\\']) {
        return Err(UrlError);
    }
    // The prefix fixes the scheme to http(s), and the WHATWG parser rejects
    // http(s) URLs with an empty host, so a successful parse has both
    // (checked by `f01_ac5_prop_accepted_urls_are_http_with_host`).
    url::Url::parse(input).map_err(|_| UrlError)
}

// CONTRACT
/// Per-request transport options (F05-AC4, F05-AC5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendOptions {
    /// Follow up to 10 redirects; otherwise show the 3xx response.
    #[serde(skip_serializing_if = "crate::is_true")]
    pub follow_redirects: bool,
    /// Reject untrusted certificates.
    #[serde(skip_serializing_if = "crate::is_true")]
    pub verify_tls: bool,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            verify_tls: true,
        }
    }
}

// CONTRACT
/// One multipart field as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub value: PartValue,
}

// CONTRACT
/// A multipart field's content. Files are read when the request is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartValue {
    Text(String),
    File(PathBuf),
}

// CONTRACT
/// A request body as sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RequestBody {
    #[default]
    Empty,
    Bytes(Bytes),
    /// Sent as `multipart/form-data` with a generated boundary.
    Multipart(Vec<Part>),
    /// The file's bytes, read when the request is sent.
    File(PathBuf),
}

// CONTRACT
/// A request ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: url::Url,
    /// Headers in order, duplicates included. The client adds `User-Agent`,
    /// `Accept` and `Accept-Encoding` unless these override them.
    pub headers: Vec<Header>,
    pub body: RequestBody,
    pub options: SendOptions,
}

impl Request {
    // CONTRACT
    /// A request with no headers or body and default options.
    #[must_use]
    pub fn new(method: Method, url: url::Url) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: RequestBody::Empty,
            options: SendOptions::default(),
        }
    }
}
