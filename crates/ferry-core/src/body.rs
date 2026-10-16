//! Request bodies as the user edits them (F03).

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::kv::KeyValue;

// CONTRACT
/// Which body is sent. Each mode keeps its own content (F03-AC1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyMode {
    #[default]
    None,
    FormData,
    UrlEncoded,
    Raw,
    Binary,
}

impl BodyMode {
    // CONTRACT
    /// All modes, in the order the UI lists them.
    pub const ALL: [BodyMode; 5] = [
        BodyMode::None,
        BodyMode::FormData,
        BodyMode::UrlEncoded,
        BodyMode::Raw,
        BodyMode::Binary,
    ];

    // CONTRACT
    /// The label Postman uses.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::FormData => "form-data",
            Self::UrlEncoded => "x-www-form-urlencoded",
            Self::Raw => "raw",
            Self::Binary => "binary",
        }
    }
}

impl fmt::Display for BodyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// CONTRACT
/// The language of a raw body, which sets its `Content-Type` (F03-AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RawLanguage {
    #[default]
    Text,
    Json,
    Xml,
    Html,
    JavaScript,
}

impl RawLanguage {
    // CONTRACT
    /// All languages, in menu order.
    pub const ALL: [RawLanguage; 5] = [
        RawLanguage::Text,
        RawLanguage::JavaScript,
        RawLanguage::Json,
        RawLanguage::Html,
        RawLanguage::Xml,
    ];

    // CONTRACT
    /// The `Content-Type` sent for this language.
    #[must_use]
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Json => "application/json",
            Self::Xml => "application/xml",
            Self::Html => "text/html",
            Self::JavaScript => "application/javascript",
        }
    }
}

impl fmt::Display for RawLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Text => "Text",
            Self::Json => "JSON",
            Self::Xml => "XML",
            Self::Html => "HTML",
            Self::JavaScript => "JavaScript",
        })
    }
}

// CONTRACT
/// One form-data field: text, or a file read when the request is sent.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FormField {
    #[serde(flatten)]
    pub row: KeyValue,
    /// When set, the field sends this file and `row.value` is ignored.
    pub file: Option<PathBuf>,
}

// CONTRACT
/// A request body as edited: every mode's content, plus the selected mode.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyDraft {
    pub mode: BodyMode,
    pub raw: String,
    pub language: RawLanguage,
    pub urlencoded: Vec<KeyValue>,
    pub form: Vec<FormField>,
    pub binary: Option<PathBuf>,
}

impl BodyDraft {
    // CONTRACT
    /// Whether the selected mode sends anything.
    #[must_use]
    pub fn is_set(&self) -> bool {
        match self.mode {
            BodyMode::None => false,
            BodyMode::Raw => !self.raw.is_empty(),
            BodyMode::UrlEncoded => self.urlencoded.iter().any(KeyValue::is_active),
            BodyMode::FormData => self.form.iter().any(|f| f.row.is_active()),
            BodyMode::Binary => self.binary.is_some(),
        }
    }

    // CONTRACT
    /// The `Content-Type` this body implies, if any. For form-data the
    /// boundary is chosen when sending, so it is described, not given.
    #[must_use]
    pub fn content_type(&self) -> Option<String> {
        if !self.is_set() {
            return None;
        }
        Some(match self.mode {
            BodyMode::None => return None,
            BodyMode::Raw => self.language.content_type().to_owned(),
            BodyMode::UrlEncoded => "application/x-www-form-urlencoded".to_owned(),
            BodyMode::FormData => {
                "multipart/form-data; boundary=<calculated when request is sent>".to_owned()
            }
            BodyMode::Binary => guess_content_type(self.binary.as_deref()?).to_owned(),
        })
    }
}

// CONTRACT
/// The content type for a file, guessed from its extension, or
/// `application/octet-stream` (F03-AC5, F03-AC6).
#[must_use]
pub fn guess_content_type(path: &Path) -> &'static str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}
