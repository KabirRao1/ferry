//! Authorization helpers (F04).

use std::fmt;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

// CONTRACT
/// The selected authorization type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    /// Use the nearest parent folder's or collection's auth (F04-AC6).
    Inherit,
    #[default]
    None,
    Basic,
    Bearer,
    ApiKey,
}

impl AuthKind {
    // CONTRACT
    /// All types, in menu order.
    pub const ALL: [AuthKind; 5] = [
        AuthKind::Inherit,
        AuthKind::None,
        AuthKind::Basic,
        AuthKind::Bearer,
        AuthKind::ApiKey,
    ];
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Inherit => "Inherit auth from parent",
            Self::None => "No Auth",
            Self::Basic => "Basic Auth",
            Self::Bearer => "Bearer Token",
            Self::ApiKey => "API Key",
        })
    }
}

// CONTRACT
/// Where an API key goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

impl ApiKeyLocation {
    pub const ALL: [ApiKeyLocation; 2] = [ApiKeyLocation::Header, ApiKeyLocation::Query];
}

impl fmt::Display for ApiKeyLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Header => "Header",
            Self::Query => "Query Params",
        })
    }
}

// CONTRACT
/// Authorization as edited: every type's fields, plus the selected type
/// (F04-AC1).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthDraft {
    #[serde(skip_serializing_if = "crate::is_default")]
    pub kind: AuthKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub location: ApiKeyLocation,
}

// CONTRACT
/// What the selected auth type adds to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthEffect {
    Header { name: String, value: String },
    Query { key: String, value: String },
}

impl AuthDraft {
    // CONTRACT
    /// What the selected type adds, if anything (F04-AC2..AC4).
    #[must_use]
    pub fn effect(&self) -> Option<AuthEffect> {
        match self.kind {
            // Inheritance is resolved by the caller, which knows the parents.
            AuthKind::Inherit | AuthKind::None => None,
            AuthKind::Basic => {
                let credentials = format!("{}:{}", self.username, self.password);
                let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
                Some(AuthEffect::Header {
                    name: "Authorization".to_owned(),
                    value: format!("Basic {encoded}"),
                })
            }
            AuthKind::Bearer if self.token.is_empty() => None,
            AuthKind::Bearer => Some(AuthEffect::Header {
                name: "Authorization".to_owned(),
                value: format!("Bearer {}", self.token),
            }),
            AuthKind::ApiKey if self.key.is_empty() => None,
            AuthKind::ApiKey => Some(match self.location {
                ApiKeyLocation::Header => AuthEffect::Header {
                    name: self.key.clone(),
                    value: self.value.clone(),
                },
                ApiKeyLocation::Query => AuthEffect::Query {
                    key: self.key.clone(),
                    value: self.value.clone(),
                },
            }),
        }
    }
}
