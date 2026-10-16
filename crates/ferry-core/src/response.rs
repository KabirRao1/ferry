//! Responses as received (F06).

use std::time::Duration;

use bytes::Bytes;

// CONTRACT
/// One response header, in received order. Values that are not valid UTF-8
/// are decoded lossily.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

// CONTRACT
/// A complete response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// Canonical reason phrase for `status`, or empty if it has none.
    pub reason: String,
    pub headers: Vec<Header>,
    pub body: Bytes,
    /// Time from sending the request to receiving the whole body.
    pub elapsed: Duration,
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
