//! File and directory names for workspace items (F09-AC8, ADR-0014).

/// Longest name kept, in characters, before an extension or suffix.
pub const MAX_NAME_CHARS: usize = 100;

/// Names Windows reserves for devices, with or without an extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

// CONTRACT
/// A file or directory name for `name` that every supported OS accepts:
/// `<>:"/\|?*` and control characters become `_`, trailing dots and spaces
/// are trimmed, Windows reserved names get a `_` after them, an empty
/// result becomes `Untitled`, and at most [`MAX_NAME_CHARS`] characters are
/// kept.
#[must_use]
pub fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(MAX_NAME_CHARS)
        .collect();
    trim_end(&mut out);
    let stem_len = out.find('.').unwrap_or(out.len());
    if RESERVED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(&out[..stem_len]))
    {
        out.insert(stem_len, '_');
        if out.chars().count() > MAX_NAME_CHARS {
            out.pop();
            trim_end(&mut out);
        }
    }
    if out.is_empty() {
        out.push_str("Untitled");
    }
    out
}

fn trim_end(s: &mut String) {
    let kept = s.trim_end_matches(['.', ' ']).len();
    s.truncate(kept);
}

// CONTRACT
/// A name for a new item `<sanitize(name)><extension>` that collides with
/// none of `siblings` (compared ignoring case), adding ` (2)`, ` (3)`, …
/// before the extension as needed. `extension` is `""` for directories.
#[must_use]
pub fn unique(name: &str, extension: &str, siblings: &[&str]) -> String {
    let base = sanitize(name);
    let taken = |candidate: &str| {
        siblings
            .iter()
            .any(|s| s.to_lowercase() == candidate.to_lowercase())
    };
    let mut candidate = format!("{base}{extension}");
    let mut n = 2u64;
    while taken(&candidate) {
        candidate = format!("{base} ({n}){extension}");
        n += 1;
    }
    candidate
}

/// File names Ferry uses for its own data inside collection folders.
const RESERVED_FILES: [&str; 3] = ["collection.toml", "folder.toml", "_order.toml"];

// CONTRACT
/// The file or directory name for a new workspace item: like [`unique`],
/// but never hidden (a leading `.` becomes `_`), never one of Ferry's own
/// files (`collection.toml`, `folder.toml`, `_order.toml`), and never a
/// local file (`*.local.toml`).
#[must_use]
pub fn item_file(name: &str, extension: &str, siblings: &[&str]) -> String {
    let mut base = sanitize(name);
    if base.starts_with('.') {
        base.replace_range(..1, "_");
    }
    let is_toml = extension.eq_ignore_ascii_case(".toml");
    if is_toml && base.to_lowercase().ends_with(".local") {
        if base.chars().count() >= MAX_NAME_CHARS {
            base.pop();
        }
        base.push('_');
    }
    let mut taken = siblings.to_vec();
    if is_toml {
        taken.extend(RESERVED_FILES);
    }
    unique(&base, extension, &taken)
}
