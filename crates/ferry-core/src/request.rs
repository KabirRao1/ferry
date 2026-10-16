//! Requests as the user builds them (F01).

use std::fmt;

// CONTRACT
/// An HTTP method. Standard methods have their own variants; anything else
/// that is a valid RFC 9110 token is kept verbatim as `Custom`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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
fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
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
/// A request ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: url::Url,
}
