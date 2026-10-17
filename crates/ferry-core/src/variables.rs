//! `{{variable}}` references and their resolution (F08-AC1..AC7).
//!
//! Resolution never fails half-way: unresolved names are collected across
//! every field of a request, so the user sees them all at once (F08-AC5).

use std::fmt::{self, Write as _};
use std::ops::Range;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::time;

/// How many variables deep a value may refer to others (F08-AC4).
pub const MAX_DEPTH: usize = 10;

// CONTRACT
/// One variable as defined in a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variable {
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "crate::is_true")]
    pub enabled: bool,
    /// Masked in the UI; its value is saved only to the local file
    /// (ADR-0040).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
    /// Which file the value came from, so saving puts it back there
    /// (F08-AC10).
    #[serde(skip)]
    pub origin: Origin,
}

// CONTRACT
/// Where a variable's value is stored (F08-AC10, ADR-0040).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Origin {
    /// In the shared file, unless the variable is secret.
    #[default]
    Shared,
    /// In the git-ignored local file, overriding `shared`, the shared
    /// file's value, which saving leaves unchanged.
    Overridden { shared: String },
    /// Only in the local file.
    LocalOnly,
}

impl Default for Variable {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
            secret: false,
            origin: Origin::Shared,
        }
    }
}

impl Variable {
    // CONTRACT
    /// An enabled, non-secret variable.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            ..Self::default()
        }
    }
}

// CONTRACT
/// A `{{name}}` reference in a text (F08-AC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    /// Byte range of the whole `{{name}}`.
    pub range: Range<usize>,
}

// CONTRACT
/// Every reference in `text`, in order (F08-AC1).
#[must_use]
pub fn references(text: &str) -> Vec<Reference> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(found) = memchr::memmem::find(&bytes[i..], b"{{") {
        let start = i + found;
        let name_start = start + 2;
        // The name runs to the first brace or line break.
        let end = bytes[name_start..]
            .iter()
            .position(|b| matches!(b, b'{' | b'}' | b'\r' | b'\n'))
            .map_or(bytes.len(), |p| name_start + p);
        if end > name_start && bytes[end..].starts_with(b"}}") {
            out.push(Reference {
                name: text[name_start..end].to_owned(),
                range: start..end + 2,
            });
            i = end + 2;
        } else {
            // Not a reference here; a later `{{` may start one.
            i = start + 1;
        }
    }
    out
}

// CONTRACT
/// Where a variable is defined, in precedence order (F08-AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Environment,
    Collection,
    Global,
}

impl fmt::Display for ScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Environment => "Environment",
            Self::Collection => "Collection",
            Self::Global => "Global",
        })
    }
}

// CONTRACT
/// The variables in effect, highest precedence first (F08-AC2).
#[derive(Debug, Clone, Default)]
pub struct Vars<'a> {
    scopes: Vec<(ScopeKind, &'a [Variable])>,
}

impl<'a> Vars<'a> {
    // CONTRACT
    /// No variables.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // CONTRACT
    /// Add a scope with lower precedence than those already added.
    #[must_use]
    pub fn with(mut self, kind: ScopeKind, variables: &'a [Variable]) -> Self {
        self.scopes.push((kind, variables));
        self
    }

    // CONTRACT
    /// The first enabled definition of `name`, and its scope.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<(&'a Variable, ScopeKind)> {
        self.scopes.iter().find_map(|(kind, variables)| {
            variables
                .iter()
                .find(|v| v.enabled && v.key == name)
                .map(|v| (v, *kind))
        })
    }
}

// CONTRACT
/// Clock and randomness for dynamic variables (F08-AC7). Injected so tests
/// are deterministic.
pub trait Ambient {
    fn now(&self) -> SystemTime;
    /// A uniformly random 64-bit number.
    fn random(&mut self) -> u64;
}

// CONTRACT
/// Why a request's variables cannot be resolved. `Display` is the exact
/// text shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarError {
    /// Names in order of first appearance, each once (F08-AC5).
    Unresolved(Vec<String>),
    /// A reference that leads back to itself (F08-AC4).
    Cycle(String),
    /// Nesting past [`MAX_DEPTH`], at this name (F08-AC4).
    TooDeep(String),
}

impl fmt::Display for VarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unresolved(names) => {
                let list: Vec<String> = names.iter().map(|n| format!("{{{{{n}}}}}")).collect();
                let noun = if names.len() == 1 {
                    "variable"
                } else {
                    "variables"
                };
                write!(f, "Unresolved {noun} {}", list.join(", "))
            }
            Self::Cycle(name) => write!(f, "Variable {{{{{name}}}}} refers to itself"),
            Self::TooDeep(name) => write!(
                f,
                "Variables are nested more than {MAX_DEPTH} levels deep at {{{{{name}}}}}"
            ),
        }
    }
}

impl std::error::Error for VarError {}

// CONTRACT
/// Resolves texts against [`Vars`] and dynamic variables, collecting
/// problems until [`Resolver::finish`].
pub struct Resolver<'a, 'v> {
    vars: &'a Vars<'v>,
    ambient: &'a mut dyn Ambient,
    unresolved: Vec<String>,
    fatal: Option<VarError>,
}

impl<'a, 'v> Resolver<'a, 'v> {
    // CONTRACT
    #[must_use]
    pub fn new(vars: &'a Vars<'v>, ambient: &'a mut dyn Ambient) -> Self {
        Self {
            vars,
            ambient,
            unresolved: Vec::new(),
            fatal: None,
        }
    }

    // CONTRACT
    /// `text` with every reference replaced. Unresolved references stay
    /// verbatim and are recorded for [`Resolver::finish`].
    pub fn resolve(&mut self, text: &str) -> String {
        let mut stack = Vec::new();
        self.expand(text, &mut stack)
    }

    /// `text` with references replaced; `stack` holds the variables being
    /// expanded, outermost first.
    fn expand(&mut self, text: &str, stack: &mut Vec<String>) -> String {
        let refs = references(text);
        if refs.is_empty() {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for r in refs {
            out.push_str(&text[last..r.range.start]);
            last = r.range.end;
            let reference = &text[r.range];
            if let Some((variable, _)) = self.vars.lookup(&r.name) {
                if stack.contains(&r.name) {
                    self.fail(VarError::Cycle(r.name));
                    out.push_str(reference);
                } else if stack.len() == MAX_DEPTH {
                    self.fail(VarError::TooDeep(r.name));
                    out.push_str(reference);
                } else {
                    stack.push(r.name);
                    let value = self.expand(&variable.value, stack);
                    stack.pop();
                    out.push_str(&value);
                }
            } else if let Some(value) = dynamic(&r.name, self.ambient) {
                out.push_str(&value);
            } else {
                if !self.unresolved.contains(&r.name) {
                    self.unresolved.push(r.name);
                }
                out.push_str(reference);
            }
        }
        out.push_str(&text[last..]);
        out
    }

    fn fail(&mut self, error: VarError) {
        if self.fatal.is_none() {
            self.fatal = Some(error);
        }
    }

    // CONTRACT
    /// The first cycle or depth problem met, else the unresolved names.
    ///
    /// # Errors
    /// [`VarError`] if anything could not be resolved.
    pub fn finish(self) -> Result<(), VarError> {
        match self.fatal {
            Some(error) => Err(error),
            None if self.unresolved.is_empty() => Ok(()),
            None => Err(VarError::Unresolved(self.unresolved)),
        }
    }
}

// CONTRACT
/// The value of a dynamic variable such as `$guid`, or `None` if `name`
/// is not one (F08-AC7).
pub fn dynamic(name: &str, ambient: &mut dyn Ambient) -> Option<String> {
    Some(match name {
        "$guid" | "$randomUUID" => uuid_v4([ambient.random(), ambient.random()]),
        "$timestamp" => time::unix_millis(ambient.now())
            .div_euclid(1000)
            .to_string(),
        "$isoTimestamp" => time::iso8601_utc(ambient.now()),
        "$randomInt" => (ambient.random() % 1001).to_string(),
        _ => return None,
    })
}

// CONTRACT
/// A version 4 UUID from 128 random bits, lowercase and hyphenated.
#[must_use]
pub fn uuid_v4(random: [u64; 2]) -> String {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&random[0].to_be_bytes());
    bytes[8..].copy_from_slice(&random[1].to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut hex = String::with_capacity(32);
    for b in bytes {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{b:02x}");
    }
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

// CONTRACT
/// Whether `name` is a dynamic variable such as `$guid` (F08-AC7).
#[must_use]
pub fn is_dynamic(name: &str) -> bool {
    matches!(
        name,
        "$guid" | "$randomUUID" | "$timestamp" | "$isoTimestamp" | "$randomInt"
    )
}
