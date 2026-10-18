//! The console: every request as sent and how it ended (F18).

use std::collections::BTreeSet;
use std::time::Duration;

use ferry_core::format;
use ferry_core::request::RequestBody;
use ferry_core::response::Header;
use ferry_core::text::looks_binary;

/// Entries kept (F18-AC4).
pub const CONSOLE_MAX: usize = 500;
/// Bytes of a body shown (F18-AC1).
pub const CONSOLE_BODY: usize = 10 * 1024;

// CONTRACT
/// How a logged send stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleState {
    Pending,
    Done {
        status: u16,
        reason: String,
        elapsed: Duration,
        size: u64,
        headers: Vec<Header>,
        /// Up to [`CONSOLE_BODY`] bytes of text, or a description.
        body: String,
    },
    Failed(String),
    Cancelled,
}

// CONTRACT
/// One logged send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleEntry {
    pub id: u64,
    pub method: String,
    /// The URL as sent, variables resolved.
    pub url: String,
    /// The headers as sent: what the request built, replaced by what the
    /// client actually sent once the response arrives.
    pub request_headers: Vec<Header>,
    pub request_body: String,
    pub state: ConsoleState,
}

impl ConsoleEntry {
    // CONTRACT
    /// The one-line summary: `GET https://… 200 · 85 ms`, `… · Failed`,
    /// `… · Cancelled` (F18-AC2).
    #[must_use]
    pub fn summary(&self) -> String {
        let request = format!("{} {}", self.method, self.url);
        match &self.state {
            ConsoleState::Pending => request,
            ConsoleState::Done {
                status, elapsed, ..
            } => format!("{request} {status} · {}", format::duration(*elapsed)),
            ConsoleState::Failed(_) => format!("{request} · Failed"),
            ConsoleState::Cancelled => format!("{request} · Cancelled"),
        }
    }

    // CONTRACT
    /// A failed send or a response with status 400 or above (F18-AC3).
    #[must_use]
    pub fn is_error(&self) -> bool {
        match &self.state {
            ConsoleState::Failed(_) => true,
            ConsoleState::Done { status, .. } => *status >= 400,
            ConsoleState::Pending | ConsoleState::Cancelled => false,
        }
    }
}

// CONTRACT
/// The console panel's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Console {
    /// Oldest first.
    pub entries: Vec<ConsoleEntry>,
    pub open: bool,
    pub filter: String,
    pub errors_only: bool,
    pub expanded: BTreeSet<u64>,
    pub(crate) next: u64,
}

impl Console {
    // CONTRACT
    /// The entries the filters keep (F18-AC3).
    #[must_use]
    pub fn visible(&self) -> Vec<&ConsoleEntry> {
        let filter = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|e| !self.errors_only || e.is_error())
            .filter(|e| {
                filter.is_empty()
                    || e.url.to_lowercase().contains(&filter)
                    || e.method.to_lowercase().contains(&filter)
            })
            .collect()
    }

    /// Log a new send, dropping the oldest past [`CONSOLE_MAX`]. Returns its id.
    pub(crate) fn push(
        &mut self,
        method: String,
        url: String,
        request_headers: Vec<Header>,
        request_body: String,
    ) -> u64 {
        let id = self.next;
        self.next += 1;
        self.entries.push(ConsoleEntry {
            id,
            method,
            url,
            request_headers,
            request_body,
            state: ConsoleState::Pending,
        });
        if self.entries.len() > CONSOLE_MAX {
            let excess = self.entries.len() - CONSOLE_MAX;
            for gone in self.entries.drain(..excess) {
                self.expanded.remove(&gone.id);
            }
        }
        id
    }

    pub(crate) fn entry_mut(&mut self, id: u64) -> Option<&mut ConsoleEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }
}

// CONTRACT
/// A request body as the console shows it (F18-AC1): text up to
/// [`CONSOLE_BODY`] bytes, `<n> bytes` for binary data,
/// `multipart/form-data, <n> fields`, or `file <name>`.
#[must_use]
pub fn describe_body(body: &RequestBody) -> String {
    match body {
        RequestBody::Empty => String::new(),
        RequestBody::Bytes(bytes) => preview(bytes),
        RequestBody::Multipart(parts) => format!(
            "multipart/form-data, {} field{}",
            parts.len(),
            if parts.len() == 1 { "" } else { "s" }
        ),
        RequestBody::File(path) => format!(
            "file {}",
            path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned()
            )
        ),
    }
}

// CONTRACT
/// Up to [`CONSOLE_BODY`] bytes of text, marked when cut, or `<n> bytes`
/// for binary data.
#[must_use]
pub fn preview(bytes: &[u8]) -> String {
    if looks_binary(bytes) {
        return format!("{} bytes", bytes.len());
    }
    if bytes.len() <= CONSOLE_BODY {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut end = CONSOLE_BODY;
    // Back up to a character boundary (at most 3 bytes for UTF-8).
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() && CONSOLE_BODY - end < 4 {
        end -= 1;
    }
    format!(
        "{}\n… ({} in total)",
        String::from_utf8_lossy(&bytes[..end]),
        format::size(bytes.len() as u64)
    )
}
