//! Collection Runner data files: CSV with a header row, or a JSON array of
//! objects (F16-AC3). Each row is one iteration.

use std::fmt;

// CONTRACT
/// One iteration's values, in column order.
pub type Row = Vec<(String, String)>;

// CONTRACT
/// Shown for a file that is neither form (F16-AC3).
pub const INVALID: &str = "Data file must be CSV with a header row or a JSON array of objects";

// CONTRACT
/// Why a data file was refused. `Display` is the text shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError;

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(INVALID)
    }
}

impl std::error::Error for DataError {}

// CONTRACT
/// Parse a data file: JSON when it starts with `[` (after whitespace),
/// otherwise CSV. A file without rows is refused.
///
/// # Errors
/// [`DataError`] for anything else.
pub fn parse(text: &str) -> Result<Vec<Row>, DataError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rows = if text.trim_start().starts_with('[') {
        parse_json(text)?
    } else {
        parse_csv(text)?
    };
    if rows.is_empty() {
        Err(DataError)
    } else {
        Ok(rows)
    }
}

fn parse_json(text: &str) -> Result<Vec<Row>, DataError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| DataError)?;
    let serde_json::Value::Array(items) = value else {
        return Err(DataError);
    };
    items
        .into_iter()
        .map(|item| match item {
            serde_json::Value::Object(map) => Ok(map
                .into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, v)
                })
                .collect()),
            _ => Err(DataError),
        })
        .collect()
}

/// RFC 4180 records: quoted fields may hold commas, line breaks and `""`.
fn records(text: &str) -> Result<Vec<Vec<String>>, DataError> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut started = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                quoted = true;
                started = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                if started || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                started = false;
            }
            other => {
                field.push(other);
                started = true;
            }
        }
    }
    if quoted {
        return Err(DataError);
    }
    if started || !field.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

fn parse_csv(text: &str) -> Result<Vec<Row>, DataError> {
    let mut records = records(text)?.into_iter();
    let header = records.next().ok_or(DataError)?;
    if header.iter().all(|h| h.trim().is_empty()) {
        return Err(DataError);
    }
    records
        .map(|record| {
            if record.len() > header.len() {
                return Err(DataError);
            }
            Ok(header
                .iter()
                .enumerate()
                .filter(|(_, h)| !h.trim().is_empty())
                .map(|(i, h)| {
                    (
                        h.trim().to_owned(),
                        record.get(i).cloned().unwrap_or_default(),
                    )
                })
                .collect())
        })
        .collect()
}
