//! The cookie jar: storing `Set-Cookie` headers and choosing the cookies to
//! send, as RFC 6265 describes (F17, ADR-0038). Times are milliseconds since
//! the Unix epoch, passed in by the caller.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::collection::{FormatError, toml_error};
use crate::environment::to_toml;
use crate::time;

/// Whether `host` is an IPv4 or IPv6 address, which only match exactly.
fn is_ip(host: &str) -> bool {
    host.trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok()
}

/// RFC 6265 domain matching: `host` is `domain` or a subdomain of it.
fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain
        || (!is_ip(host)
            && host
                .strip_suffix(domain)
                .is_some_and(|rest| rest.ends_with('.')))
}

/// RFC 6265 section 5.1.4: the directory of the request path.
fn default_path(path: &str) -> String {
    if !path.starts_with('/') {
        return "/".to_owned();
    }
    match path.rfind('/') {
        Some(0) | None => "/".to_owned(),
        Some(i) => path[..i].to_owned(),
    }
}

/// RFC 6265 section 5.1.4 path matching.
fn path_matches(request: &str, cookie: &str) -> bool {
    request == cookie
        || request
            .strip_prefix(cookie)
            .is_some_and(|rest| cookie.ends_with('/') || rest.starts_with('/'))
}

// CONTRACT
/// One stored cookie.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// Lowercase, without a leading dot.
    pub domain: String,
    /// Sent only to `domain` itself, not its subdomains.
    #[serde(default)]
    pub host_only: bool,
    pub path: String,
    /// Expiry time; `None` for a session cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
    /// When it was first stored, for ordering.
    #[serde(default)]
    pub created: i64,
}

// CONTRACT
/// Why a cookie was not stored. `Display` is the text the cookie manager
/// shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieError {
    /// No `name=` (F17-AC5).
    Syntax,
    /// The `Domain` attribute does not match the host, or is a single label.
    Domain { domain: String },
}

impl fmt::Display for CookieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("Enter a cookie as name=value; attributes"),
            Self::Domain { domain } => {
                write!(f, "The cookie's domain {domain} does not match this host")
            }
        }
    }
}

impl std::error::Error for CookieError {}

// CONTRACT
/// Parse one `Set-Cookie` value received from `host` for a request to
/// `path` (F17-AC1). An expiry in the past gives a cookie that
/// [`Cookie::is_expired`] at `now`.
///
/// # Errors
/// [`CookieError`] if the cookie must be ignored.
pub fn parse_set_cookie(
    header: &str,
    host: &str,
    path: &str,
    now: i64,
) -> Result<Cookie, CookieError> {
    let host = host.to_ascii_lowercase();
    let mut parts = header.split(';');
    let pair = parts.next().unwrap_or_default();
    let (name, value) = pair.split_once('=').ok_or(CookieError::Syntax)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(CookieError::Syntax);
    }
    let mut cookie = Cookie {
        name: name.to_owned(),
        value: value.trim().to_owned(),
        domain: host.clone(),
        host_only: true,
        path: default_path(path),
        expires: None,
        secure: false,
        http_only: false,
        same_site: None,
        created: now,
    };
    let mut expires = None;
    let mut max_age = None;
    let mut domain_attr = None;
    for attribute in parts {
        let (key, value) = match attribute.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (attribute.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "expires" => expires = parse_date(value).or(expires),
            "max-age" => {
                let valid = value.strip_prefix('-').unwrap_or(value);
                if !valid.is_empty() && valid.bytes().all(|b| b.is_ascii_digit()) {
                    // Absurdly large values saturate; negative ones mean "now".
                    let seconds: i64 = value.parse().unwrap_or(if value.starts_with('-') {
                        i64::MIN
                    } else {
                        i64::MAX
                    });
                    max_age = Some(if seconds <= 0 {
                        i64::MIN
                    } else {
                        now.saturating_add(seconds.saturating_mul(1000))
                    });
                }
            }
            "domain" if !value.is_empty() => {
                domain_attr = Some(value.trim_start_matches('.').to_ascii_lowercase());
            }
            "path" if value.starts_with('/') => value.clone_into(&mut cookie.path),
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            "samesite" if !value.is_empty() => cookie.same_site = Some(value.to_owned()),
            _ => {}
        }
    }
    cookie.expires = max_age.or(expires);
    if let Some(domain) = domain_attr {
        let single_label = !domain.contains('.') && domain != host;
        if single_label || !domain_matches(&host, &domain) {
            return Err(CookieError::Domain { domain });
        }
        cookie.domain = domain;
        cookie.host_only = false;
    }
    Ok(cookie)
}

// CONTRACT
/// Parse a cookie date as RFC 6265 section 5.1.1 describes, for example
/// `Wed, 21 Oct 2026 07:28:00 GMT`; `None` if it is not one.
#[must_use]
pub fn parse_date(text: &str) -> Option<i64> {
    let is_delimiter = |c: char| matches!(c, '\t' | ' '..='/' | ';'..='@' | '['..='`' | '{'..='~');
    let mut time = None;
    let mut day = None;
    let mut month = None;
    let mut year = None;
    for token in text.split(is_delimiter).filter(|t| !t.is_empty()) {
        if time.is_none()
            && let Some(t) = parse_time(token)
        {
            time = Some(t);
        } else if day.is_none()
            && let Some(d) = leading_digits(token, 1, 2)
        {
            day = Some(d);
        } else if month.is_none()
            && let Some(m) = time::MONTHS
                .iter()
                .position(|m| token.get(..3).is_some_and(|t| t.eq_ignore_ascii_case(m)))
        {
            month = Some(m + 1);
        } else if year.is_none()
            && let Some(y) = leading_digits(token, 2, 4)
        {
            year = Some(y);
        }
    }
    let (hour, minute, second) = time?;
    let (day, month, mut year) = (day?, month?, year?);
    if (70..=99).contains(&year) {
        year += 1900;
    } else if year <= 69 {
        year += 2000;
    }
    if !(1..=31).contains(&day) || year < 1601 || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // `day` is 1..=31 and `month` 1..=12 here.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let days = time::days_from_civil(i64::from(year), month as u8, day as u8);
    Some(
        (days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
            * 1000,
    )
}

/// `1*2DIGIT ":" 1*2DIGIT ":" 1*2DIGIT ( non-digit *OCTET )`.
fn parse_time(token: &str) -> Option<(u32, u32, u32)> {
    let mut parts = token.splitn(3, ':');
    let hour = digits_exact(parts.next()?, 1, 2)?;
    let minute = digits_exact(parts.next()?, 1, 2)?;
    let second = leading_digits(parts.next()?, 1, 2)?;
    Some((hour, minute, second))
}

fn digits_exact(text: &str, min: usize, max: usize) -> Option<u32> {
    ((min..=max).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// `min` to `max` digits, then the end or a non-digit.
fn leading_digits(token: &str, min: usize, max: usize) -> Option<u32> {
    let n = token.bytes().take_while(u8::is_ascii_digit).count();
    if (min..=max).contains(&n) {
        token[..n].parse().ok()
    } else {
        None
    }
}

impl Cookie {
    // CONTRACT
    #[must_use]
    pub fn is_expired(&self, now: i64) -> bool {
        self.expires.is_some_and(|at| at <= now)
    }

    // CONTRACT
    /// The cookie as `Set-Cookie` text, for editing (F17-AC5).
    #[must_use]
    pub fn to_set_cookie(&self) -> String {
        let mut out = format!("{}={}; Path={}", self.name, self.value, self.path);
        if !self.host_only {
            out.push_str("; Domain=");
            out.push_str(&self.domain);
        }
        if let Some(at) = self.expires {
            out.push_str("; Expires=");
            out.push_str(&time::http_date(at));
        }
        if self.secure {
            out.push_str("; Secure");
        }
        if self.http_only {
            out.push_str("; HttpOnly");
        }
        if let Some(same_site) = &self.same_site {
            out.push_str("; SameSite=");
            out.push_str(same_site);
        }
        out
    }

    // CONTRACT
    /// Whether this cookie goes with a request to `host` and `path` over
    /// `secure` (https) at `now` (F17-AC2).
    #[must_use]
    pub fn matches(&self, host: &str, path: &str, secure: bool, now: i64) -> bool {
        let host_ok = if self.host_only {
            host == self.domain
        } else {
            domain_matches(host, &self.domain)
        };
        host_ok
            && path_matches(if path.is_empty() { "/" } else { path }, &self.path)
            && (secure || !self.secure)
            && !self.is_expired(now)
    }
}

// CONTRACT
/// Every stored cookie.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Jar {
    #[serde(default)]
    pub cookies: Vec<Cookie>,
}

impl Jar {
    // CONTRACT
    /// Store a `Set-Cookie` value from a response (F17-AC1). A cookie with
    /// the same name, domain and path is replaced, keeping its creation
    /// time; an expired one removes it.
    ///
    /// # Errors
    /// [`CookieError`] if the cookie was ignored.
    pub fn store(
        &mut self,
        header: &str,
        host: &str,
        path: &str,
        now: i64,
    ) -> Result<(), CookieError> {
        let mut cookie = parse_set_cookie(header, host, path, now)?;
        let same = |c: &Cookie| {
            c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path
        };
        if let Some(i) = self.cookies.iter().position(same) {
            let old = self.cookies.remove(i);
            cookie.created = old.created;
        }
        if !cookie.is_expired(now) {
            self.cookies.push(cookie);
        }
        Ok(())
    }

    // CONTRACT
    /// The `Cookie` header for a request, or `None` if no cookie matches
    /// (F17-AC2): longer paths first, then older ones first.
    #[must_use]
    pub fn header_for(&self, host: &str, path: &str, secure: bool, now: i64) -> Option<String> {
        let mut matching: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|c| c.matches(host, path, secure, now))
            .collect();
        if matching.is_empty() {
            return None;
        }
        matching.sort_by(|a, b| {
            b.path
                .len()
                .cmp(&a.path.len())
                .then(a.created.cmp(&b.created))
        });
        Some(
            matching
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    // CONTRACT
    /// The domains with cookies, sorted.
    #[must_use]
    pub fn domains(&self) -> Vec<String> {
        let mut domains: Vec<String> = self.cookies.iter().map(|c| c.domain.clone()).collect();
        domains.sort();
        domains.dedup();
        domains
    }

    // CONTRACT
    /// The cookies of a domain, by name.
    #[must_use]
    pub fn for_domain(&self, domain: &str) -> Vec<&Cookie> {
        let mut cookies: Vec<&Cookie> =
            self.cookies.iter().filter(|c| c.domain == domain).collect();
        cookies.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        cookies
    }

    // CONTRACT
    /// Remove one cookie; `true` if it existed.
    pub fn remove(&mut self, domain: &str, name: &str, path: &str) -> bool {
        let before = self.cookies.len();
        self.cookies
            .retain(|c| !(c.domain == domain && c.name == name && c.path == path));
        self.cookies.len() != before
    }

    // CONTRACT
    /// Remove every cookie of a domain.
    pub fn remove_domain(&mut self, domain: &str) {
        self.cookies.retain(|c| c.domain != domain);
    }

    // CONTRACT
    /// Drop expired cookies.
    pub fn purge(&mut self, now: i64) {
        self.cookies.retain(|c| !c.is_expired(now));
    }
}

// CONTRACT
/// Parse `cookies.toml`, dropping cookies expired at `now` (F17-AC6).
///
/// # Errors
/// [`FormatError`] if the text is not valid.
pub fn parse(text: &str, now: i64) -> Result<Jar, FormatError> {
    let mut jar: Jar = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    jar.purge(now);
    Ok(jar)
}

// CONTRACT
#[must_use]
pub fn write(jar: &Jar) -> String {
    to_toml(jar)
}
