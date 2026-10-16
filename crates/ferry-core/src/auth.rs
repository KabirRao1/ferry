//! Authorization helpers (F04).

use std::fmt;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

// CONTRACT
/// The selected authorization type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthKind {
    #[default]
    None,
    Basic,
    Bearer,
    ApiKey,
}

impl AuthKind {
    // CONTRACT
    /// All types, in menu order.
    pub const ALL: [AuthKind; 4] = [
        AuthKind::None,
        AuthKind::Basic,
        AuthKind::Bearer,
        AuthKind::ApiKey,
    ];
}

impl fmt::Display for AuthKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
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
    pub kind: AuthKind,
    pub username: String,
    pub password: String,
    pub token: String,
    pub key: String,
    pub value: String,
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
            AuthKind::None => None,
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
