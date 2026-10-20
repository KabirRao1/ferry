//! The scripts a request, folder or collection carries (F15-AC1).

use serde::{Deserialize, Serialize};

// CONTRACT
/// Pre-request and post-response JavaScript, saved as `[scripts]` in the
/// item's file; empty scripts are left out.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scripts {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pre_request: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub post_response: String,
}

// CONTRACT
/// When a script runs (F15-AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Phase {
    #[default]
    PreRequest,
    PostResponse,
}

impl Phase {
    /// `pm.info.eventName` (F15-AC13).
    #[must_use]
    pub fn event_name(self) -> &'static str {
        match self {
            Self::PreRequest => "prerequest",
            Self::PostResponse => "test",
        }
    }
}

impl Scripts {
    // CONTRACT
    /// The script for `phase`.
    #[must_use]
    pub fn get(&self, phase: Phase) -> &str {
        match phase {
            Phase::PreRequest => &self.pre_request,
            Phase::PostResponse => &self.post_response,
        }
    }

    // CONTRACT
    /// Replace the script for `phase`.
    pub fn set(&mut self, phase: Phase, text: String) {
        match phase {
            Phase::PreRequest => self.pre_request = text,
            Phase::PostResponse => self.post_response = text,
        }
    }

    // CONTRACT
    /// Whether both scripts are empty or only whitespace.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.pre_request.trim().is_empty() && self.post_response.trim().is_empty()
    }
}
