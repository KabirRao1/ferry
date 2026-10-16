//! Scroll state for the virtualized response body viewer (F06-AC7,
//! F06-AC9, F06-AC10).

use std::borrow::Cow;
use std::num::NonZeroU32;

use bytes::Bytes;

use ferry_core::text::{IndexError, LineIndex, hex_line, hex_line_count};

// CONTRACT
/// Messages the body viewer reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Scroll by a number of display lines; negative scrolls up.
    ScrolledBy(isize),
    /// Jump so `line` is the top visible line.
    ScrolledTo(usize),
}

#[derive(Debug, Clone)]
enum Source {
    /// Text split into display lines.
    Text(LineIndex),
    /// 16 bytes per line, rendered on demand.
    Hex,
}

// CONTRACT
/// A body plus its display lines and scroll position.
#[derive(Debug, Clone)]
pub struct BodyView {
    body: Bytes,
    source: Source,
    top: usize,
}

impl BodyView {
    // CONTRACT
    /// Index `body` as text, wrapping lines at `wrap` bytes.
    ///
    /// # Errors
    /// [`IndexError::TooLarge`] if the body cannot be indexed.
    pub fn new(body: Bytes, wrap: NonZeroU32) -> Result<Self, IndexError> {
        let index = LineIndex::build(&body, wrap)?;
        Ok(Self {
            body,
            source: Source::Text(index),
            top: 0,
        })
    }

    // CONTRACT
    /// Show `body` as a hex dump (F06-AC7). Costs nothing up front.
    #[must_use]
    pub fn hex(body: Bytes) -> Self {
        Self {
            body,
            source: Source::Hex,
            top: 0,
        }
    }

    // CONTRACT
    /// Total display lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        match &self.source {
            Source::Text(index) => index.line_count(),
            Source::Hex => hex_line_count(self.body.len()),
        }
    }

    // CONTRACT
    /// Index of the top visible line. Always `< line_count()`.
    #[must_use]
    pub fn top(&self) -> usize {
        self.top
    }

    // CONTRACT
    /// Apply a message. Scrolling clamps to `0..line_count()`.
    pub fn update(&mut self, message: Message) {
        let last = self.line_count().saturating_sub(1);
        self.top = match message {
            Message::ScrolledTo(line) => line,
            Message::ScrolledBy(delta) => self.top.saturating_add_signed(delta),
        }
        .min(last);
    }

    // CONTRACT
    /// Up to `rows` lines starting at `top()`, as `(line number, text)`.
    /// Invalid UTF-8 is replaced with U+FFFD.
    #[must_use]
    pub fn visible(&self, rows: usize) -> Vec<(usize, Cow<'_, str>)> {
        (self.top..self.top.saturating_add(rows))
            .map_while(|n| match &self.source {
                Source::Text(index) => {
                    let range = index.line(&self.body, n)?;
                    Some((n, String::from_utf8_lossy(&self.body[range])))
                }
                Source::Hex => hex_line(&self.body, n).map(|line| (n, Cow::Owned(line))),
            })
            .collect()
    }
}
