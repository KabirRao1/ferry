//! Display-line indexing for large response bodies.
//!
//! A [`LineIndex`] maps display-line numbers to byte ranges of a body so a
//! viewer can render only the visible lines of a body of any size (F06).

use std::num::NonZeroU32;
use std::ops::Range;

/// Errors building a [`LineIndex`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    /// The body exceeds the largest size the index can address.
    #[error("body is {len} bytes; the viewer supports bodies up to {max} bytes")]
    TooLarge { len: usize, max: usize },
}

// CONTRACT
/// Byte offsets of every display line in a body.
///
/// Lines split on `\n`; a trailing `\r` is excluded from the line's range.
/// Logical lines longer than `wrap` bytes are split into several display
/// lines of at most `wrap` bytes, never inside a UTF-8 character.
///
/// Only line starts are stored (4 bytes per display line); ends are derived
/// from the body on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    /// Largest body (in bytes) that can be indexed.
    pub const MAX_LEN: usize = u32::MAX as usize;

    // CONTRACT
    /// Index `body`, wrapping display lines at `wrap` bytes.
    ///
    /// # Errors
    /// [`IndexError::TooLarge`] if `body` is longer than [`Self::MAX_LEN`].
    pub fn build(body: &[u8], wrap: NonZeroU32) -> Result<Self, IndexError> {
        let len = u32::try_from(body.len()).map_err(|_| IndexError::TooLarge {
            len: body.len(),
            max: Self::MAX_LEN,
        })?;
        let wrap = wrap.get() as usize;
        let mut starts = vec![0u32];
        let mut line_start = 0usize;
        for nl in memchr::memchr_iter(b'\n', body).chain(std::iter::once(body.len())) {
            push_wrapped(&mut starts, body, line_start..nl, wrap);
            if nl < body.len() {
                line_start = nl + 1;
                starts.push(as_u32(line_start));
            }
        }
        starts.shrink_to_fit();
        Ok(Self { starts, len })
    }

    // CONTRACT
    /// Number of display lines. Always at least 1; an empty body has one empty line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    // CONTRACT
    /// Byte range of display line `line` within `body`, or `None` if out of
    /// bounds. `body` must be the slice this index was built from.
    #[must_use]
    pub fn line(&self, body: &[u8], line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)? as usize;
        let next = self.starts.get(line + 1).copied().unwrap_or(self.len) as usize;
        let mut end = next.min(body.len());
        if end > start && body[end - 1] == b'\n' {
            end -= 1;
            if end > start && body[end - 1] == b'\r' {
                end -= 1;
            }
        }
        Some(start.min(end)..end)
    }
}

/// Push the starts of the wrapped continuation lines of the logical line
/// `body[line]`. The caller pushes the logical line's own start.
fn push_wrapped(starts: &mut Vec<u32>, body: &[u8], line: Range<usize>, wrap: usize) {
    // A `\r` right before `\n` is a terminator, not content: it never forces a wrap.
    let ends_crlf = line.end < body.len() && line.end > line.start && body[line.end - 1] == b'\r';
    let content_end = if ends_crlf { line.end - 1 } else { line.end };
    let mut pos = line.start;
    while content_end - pos > wrap {
        let cut = char_cut(body, pos, pos + wrap, content_end);
        if cut >= content_end {
            break;
        }
        starts.push(as_u32(cut));
        pos = cut;
    }
}

/// Choose where to end a display line starting at `pos` that would otherwise
/// end at `cut` (`pos < cut < end <= body.len()`).
///
/// Backs off to the start of the UTF-8 character containing `cut`. If that
/// character starts at `pos` (it is wider than the wrap width), the line holds
/// just that character. Bytes that are not valid UTF-8 are cut byte-wise.
fn char_cut(body: &[u8], pos: usize, cut: usize, end: usize) -> usize {
    // A character is at most 4 bytes: look back at most 3 continuation bytes.
    let boundary = (cut.saturating_sub(3).max(pos)..=cut)
        .rev()
        .find(|&i| !is_continuation(body[i]));
    match boundary {
        Some(b) if b > pos => b,
        // The character starting at `pos` is wider than the wrap width.
        Some(_) => {
            let limit = end.min(pos + 4);
            (pos + 1..limit)
                .find(|&i| !is_continuation(body[i]))
                .unwrap_or(limit)
        }
        None => cut,
    }
}

/// UTF-8 continuation byte (`10xxxxxx`).
fn is_continuation(b: u8) -> bool {
    b & 0xC0 == 0x80
}

fn as_u32(n: usize) -> u32 {
    // Callers only pass offsets within a body already checked against `MAX_LEN`.
    u32::try_from(n).unwrap_or(u32::MAX)
}
