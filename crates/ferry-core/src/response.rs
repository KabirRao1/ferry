//! Responses as received (F06).

use std::time::Duration;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

// CONTRACT
/// One header, in order. Received values that are not valid UTF-8 are
/// decoded lossily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

// CONTRACT
/// Where the time went (F06-AC2). Phases that did not happen, such as DNS
/// and connecting on a reused connection, are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timing {
    pub dns: Option<Duration>,
    /// TCP and TLS handshakes, after DNS.
    pub connect: Option<Duration>,
    /// From the connection being ready to the response headers arriving.
    pub waiting: Duration,
    /// From the response headers to the end of the body.
    pub download: Duration,
}

// CONTRACT
/// A `Set-Cookie` header received during a send, with the URL it came
/// from (F17-AC1). Redirect hops included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCookie {
    pub url: String,
    pub header: String,
}

// CONTRACT
/// A complete response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    pub status: u16,
    /// Canonical reason phrase for `status`, or empty if it has none.
    pub reason: String,
    pub headers: Vec<Header>,
    pub body: Bytes,
    /// Time from sending the request to receiving the whole body.
    pub elapsed: Duration,
    pub timing: Timing,
    /// The URL of the final response, after redirects.
    pub url: String,
    /// The request headers as sent on the first hop, including the ones the
    /// client adds and the jar's `Cookie` (F18-AC1).
    pub sent: Vec<Header>,
    /// Cookies to store in the jar, from every hop (F17-AC1). Empty when the
    /// jar was off for the request.
    pub set_cookies: Vec<SetCookie>,
}

impl Response {
    // CONTRACT
    /// Status code and reason, for example `200 OK`, or just `299` when there
    /// is no reason phrase (F06-AC1).
    #[must_use]
    pub fn status_line(&self) -> String {
        if self.reason.is_empty() {
            self.status.to_string()
        } else {
            format!("{} {}", self.status, self.reason)
        }
    }
}
