//! User-facing errors from sending a request (F05-AC3, F05-AC6, F01-AC5).

use std::time::Duration;

use crate::format;

// CONTRACT
/// Why a request did not produce a response. `Display` is the exact text
/// shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    InvalidUrl,
    Dns { host: String },
    ConnectionRefused { host: String, port: u16 },
    Tls { detail: String },
    Timeout { after: Duration },
    Other { detail: String },
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("Enter a URL starting with http:// or https://"),
            Self::Dns { host } => write!(f, "DNS lookup failed for {host}"),
            Self::ConnectionRefused { host, port } => {
                write!(f, "Connection refused by {host}:{port}")
            }
            Self::Tls { detail } => write!(f, "TLS handshake failed: {detail}"),
            Self::Timeout { after } => {
                write!(f, "Request timed out after {} s", format::seconds(*after))
            }
            Self::Other { detail } => write!(f, "Request failed: {detail}"),
        }
    }
}

impl std::error::Error for SendError {}
