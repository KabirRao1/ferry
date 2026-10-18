//! Scroll state for the virtualized response body viewer (F06-AC7,
//! F06-AC9, F06-AC10).

use std::borrow::Cow;
use std::num::NonZeroU32;
use std::ops::Range;

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
    /// The viewport now shows this many display lines.
    Resized(usize),
}

/// Viewport height assumed by [`BodyView::reveal`] before the real one is
/// reported.
const ASSUMED_ROWS: usize = 30;

// CONTRACT
/// How a piece of a line is highlighted by search (F06-AC8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Plain,
    Match,
    /// The current match.
    Current,
}

// CONTRACT
/// A visible line split into highlighted pieces: `(line number, pieces)`.
pub type MarkedLine<'a> = (usize, Vec<(Cow<'a, str>, Mark)>);

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
    /// Display lines the viewport shows; 0 until known.
    rows: usize,
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
            rows: 0,
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
            rows: 0,
        }
    }

    // CONTRACT
    /// The bytes shown.
    #[must_use]
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    // CONTRACT
    /// Whether the body is shown as text (not a hex dump).
    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self.source, Source::Text(_))
    }

    // CONTRACT
    /// The byte offset where the top visible line starts; 0 for a hex dump.
    #[must_use]
    pub fn top_offset(&self) -> usize {
        match &self.source {
            Source::Text(index) => index.line(&self.body, self.top).map_or(0, |r| r.start),
            Source::Hex => 0,
        }
    }

    // CONTRACT
    /// Display lines the viewport shows, as last reported; 0 until known.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
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
            Message::Resized(rows) => {
                self.rows = rows;
                self.top
            }
        }
        .min(last);
    }

    // CONTRACT
    /// Scroll so the display line holding byte `offset` is fully visible,
    /// about a third of the way down, unless it already is (F06-AC8).
    /// Does nothing for a hex dump.
    pub fn reveal(&mut self, offset: usize) {
        let Source::Text(index) = &self.source else {
            return;
        };
        let line = index.line_at(offset);
        let rows = if self.rows == 0 {
            ASSUMED_ROWS
        } else {
            self.rows
        };
        // The last row may be cut off, so it does not count as visible.
        let shown = rows.saturating_sub(1).max(1);
        if line < self.top || line >= self.top + shown {
            // Never scroll past the point where the last line is at the
            // bottom: that would only show blank space.
            let lowest = self.line_count().saturating_sub(shown);
            self.top = line.saturating_sub(shown / 3).min(lowest);
        }
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

    // CONTRACT
    /// Like [`visible`](Self::visible), with each line split around the
    /// matches: `matches` are ascending, non-overlapping byte offsets of
    /// `len`-byte matches, and `current` indexes the current one (F06-AC8).
    /// A hex dump has no marks.
    #[must_use]
    pub fn visible_marked(
        &self,
        rows: usize,
        matches: &[usize],
        len: usize,
        current: Option<usize>,
    ) -> Vec<MarkedLine<'_>> {
        let Source::Text(index) = &self.source else {
            return self
                .visible(rows)
                .into_iter()
                .map(|(n, line)| (n, vec![(line, Mark::Plain)]))
                .collect();
        };
        (self.top..self.top.saturating_add(rows))
            .map_while(|n| {
                let range = index.line(&self.body, n)?;
                Some((n, pieces(&self.body, range, matches, len, current)))
            })
            .collect()
    }
}

/// Split the `line` range of `body` around the matches that overlap it.
fn pieces<'a>(
    body: &'a [u8],
    line: Range<usize>,
    matches: &[usize],
    len: usize,
    current: Option<usize>,
) -> Vec<(Cow<'a, str>, Mark)> {
    let mut out = Vec::new();
    let mut at = line.start;
    if len > 0 {
        let first = matches.partition_point(|&m| m + len <= line.start);
        for (i, &m) in matches.iter().enumerate().skip(first) {
            if m >= line.end {
                break;
            }
            let (start, end) = (m.max(line.start), (m + len).min(line.end));
            if start > at {
                out.push((String::from_utf8_lossy(&body[at..start]), Mark::Plain));
            }
            let mark = if current == Some(i) {
                Mark::Current
            } else {
                Mark::Match
            };
            out.push((String::from_utf8_lossy(&body[start..end]), mark));
            at = end;
        }
    }
    if at < line.end || out.is_empty() {
        out.push((String::from_utf8_lossy(&body[at..line.end]), Mark::Plain));
    }
    out
}
