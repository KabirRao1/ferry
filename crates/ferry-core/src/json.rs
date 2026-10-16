//! JSON validation and pretty-printing (F03-AC3, F06-AC6).
//!
//! The pretty-printer streams tokens straight to its output without building
//! a tree, so it keeps key order and number spelling, and its memory use is
//! the output plus one byte of nesting state per level.

use std::fmt;

// CONTRACT
/// Why a text is not valid JSON, with a 1-based position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub line: usize,
    pub column: usize,
    pub reason: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid JSON at line {}, column {}: {}",
            self.line, self.column, self.reason
        )
    }
}

impl std::error::Error for JsonError {}

// CONTRACT
/// Check that `text` is one JSON value, optionally surrounded by whitespace.
///
/// # Errors
/// [`JsonError`] describing the first problem.
pub fn validate(text: &str) -> Result<(), JsonError> {
    serde_json::from_str::<serde::de::IgnoredAny>(text)
        .map(|_| ())
        .map_err(|e| {
            let full = e.to_string();
            let suffix = format!(" at line {} column {}", e.line(), e.column());
            let reason = full.strip_suffix(&suffix).unwrap_or(&full).to_owned();
            JsonError {
                line: e.line(),
                column: e.column(),
                reason,
            }
        })
}

const INDENT: &[u8] = b"  ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

/// What the tokenizer expects next.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Any value (top level, after `:` or after `,` in an array).
    Value,
    /// A value or `]` right after `[`.
    ValueOrClose,
    /// A key string or `}` right after `{`.
    KeyOrClose,
    /// A key string after `,` in an object.
    Key,
    /// `:` after a key.
    Colon,
    /// `,` or the closing bracket after a value.
    CommaOrClose,
    /// Only whitespace after the top-level value.
    End,
}

/// Largest output, as a multiple of the input size plus a fixed allowance.
/// Indentation grows with depth, so pathological nesting could otherwise
/// turn a small body into gigabytes of spaces.
const MAX_GROWTH: usize = 8;
const MAX_EXTRA: usize = 64 * 1024;

// CONTRACT
/// Re-indent `input` with 2 spaces per level, one member or element per
/// line, empty containers as `{}` and `[]`. Strings and numbers are copied
/// byte for byte. Returns `None` unless `input` is exactly one valid JSON
/// value surrounded by optional whitespace, or if indentation would grow the
/// output past 8 times the input plus 64 KiB (deep nesting).
#[must_use]
pub fn pretty(input: &[u8]) -> Option<Vec<u8>> {
    let limit = input
        .len()
        .saturating_mul(MAX_GROWTH)
        .saturating_add(MAX_EXTRA);
    pretty_within(input, limit)
}

// CONTRACT
/// [`pretty`] with an explicit output limit: `None` once a line break and
/// its indentation take the output past `limit` bytes. Only the closing
/// bracket can follow the last line break, so a result is at most
/// `limit + 1` bytes.
#[must_use]
pub fn pretty_within(input: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() + input.len() / 4);
    let mut stack: Vec<Container> = Vec::new();
    let mut expect = Expect::Value;
    let mut i = 0;
    while i < input.len() {
        let b = input[i];
        if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
            continue;
        }
        match expect {
            Expect::End => return None,
            Expect::Colon => {
                if b != b':' {
                    return None;
                }
                out.extend_from_slice(b": ");
                expect = Expect::Value;
                i += 1;
            }
            Expect::CommaOrClose => {
                let top = *stack.last()?;
                match (b, top) {
                    (b',', _) => {
                        out.push(b',');
                        newline(&mut out, stack.len(), limit)?;
                        expect = if top == Container::Object {
                            Expect::Key
                        } else {
                            Expect::Value
                        };
                    }
                    (b'}', Container::Object) | (b']', Container::Array) => {
                        stack.pop();
                        newline(&mut out, stack.len(), limit)?;
                        out.push(b);
                        expect = after_value(&stack);
                    }
                    _ => return None,
                }
                i += 1;
            }
            Expect::KeyOrClose | Expect::ValueOrClose
                if (b == b'}' && expect == Expect::KeyOrClose)
                    || (b == b']' && expect == Expect::ValueOrClose) =>
            {
                // Empty container: undo the newline and indent after the opener.
                stack.pop();
                let opener = out.iter().rposition(|&c| c == b'{' || c == b'[')?;
                out.truncate(opener + 1);
                out.push(b);
                expect = after_value(&stack);
                i += 1;
            }
            Expect::Key | Expect::KeyOrClose => {
                if b != b'"' {
                    return None;
                }
                i = copy_string(input, i, &mut out)?;
                expect = Expect::Colon;
            }
            Expect::Value | Expect::ValueOrClose => {
                match b {
                    b'{' | b'[' => {
                        out.push(b);
                        if b == b'{' {
                            stack.push(Container::Object);
                            expect = Expect::KeyOrClose;
                        } else {
                            stack.push(Container::Array);
                            expect = Expect::ValueOrClose;
                        }
                        newline(&mut out, stack.len(), limit)?;
                        i += 1;
                        continue;
                    }
                    b'"' => i = copy_string(input, i, &mut out)?,
                    b't' => i = copy_literal(input, i, b"true", &mut out)?,
                    b'f' => i = copy_literal(input, i, b"false", &mut out)?,
                    b'n' => i = copy_literal(input, i, b"null", &mut out)?,
                    b'-' | b'0'..=b'9' => i = copy_number(input, i, &mut out)?,
                    _ => return None,
                }
                expect = after_value(&stack);
            }
        }
    }
    (expect == Expect::End).then_some(out)
}

fn after_value(stack: &[Container]) -> Expect {
    if stack.is_empty() {
        Expect::End
    } else {
        Expect::CommaOrClose
    }
}

/// Start a new line indented to `depth`, or `None` past the output limit.
fn newline(out: &mut Vec<u8>, depth: usize, limit: usize) -> Option<()> {
    out.push(b'\n');
    // Overshoots `limit` by at most one line's indentation, which is bounded
    // by the input's nesting depth.
    for _ in 0..depth {
        out.extend_from_slice(INDENT);
    }
    (out.len() <= limit).then_some(())
}

/// Copy the string starting at `start` (a `"`), returning the index after it.
fn copy_string(input: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    let mut i = start + 1;
    loop {
        match *input.get(i)? {
            b'"' => break,
            b'\\' => match *input.get(i + 1)? {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => i += 2,
                b'u' => {
                    let hex = input.get(i + 2..i + 6)?;
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return None;
                    }
                    i += 6;
                }
                _ => return None,
            },
            0x00..=0x1f => return None,
            _ => i += 1,
        }
    }
    let end = i + 1;
    let text = input.get(start..end)?;
    // Raw bytes inside strings must be UTF-8, as JSON text is.
    std::str::from_utf8(text).ok()?;
    out.extend_from_slice(text);
    Some(end)
}

fn copy_literal(input: &[u8], start: usize, word: &[u8], out: &mut Vec<u8>) -> Option<usize> {
    let end = start + word.len();
    if input.get(start..end)? != word {
        return None;
    }
    out.extend_from_slice(word);
    Some(end)
}

/// Copy a number matching the JSON grammar: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
fn copy_number(input: &[u8], start: usize, out: &mut Vec<u8>) -> Option<usize> {
    let digits = |mut i: usize| {
        let from = i;
        while input.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > from).then_some(i)
    };
    let mut i = start;
    if input.get(i) == Some(&b'-') {
        i += 1;
    }
    i = match input.get(i)? {
        b'0' => i + 1,
        b'1'..=b'9' => digits(i)?,
        _ => return None,
    };
    if input.get(i) == Some(&b'.') {
        i = digits(i + 1)?;
    }
    if matches!(input.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(input.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(i)?;
    }
    out.extend_from_slice(input.get(start..i)?);
    Some(i)
}
