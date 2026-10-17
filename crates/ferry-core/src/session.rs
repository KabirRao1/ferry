//! The saved workbench: open tabs and layout (F07-AC5, F07-AC7, F14-AC4).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::draft::RequestDraft;

// CONTRACT
/// One saved request tab: its request as edited, and the saved request it
/// is linked to, if any (F07-AC7).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionTab {
    /// Workspace-relative path of the linked request file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(flatten)]
    pub draft: RequestDraft,
}

impl From<RequestDraft> for SessionTab {
    fn from(draft: RequestDraft) -> Self {
        Self { link: None, draft }
    }
}

// CONTRACT
/// Everything restored on the next start. Responses are not saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Format version, for future migrations.
    pub version: u32,
    pub tabs: Vec<SessionTab>,
    /// Index into `tabs` of the active tab.
    pub active: usize,
    pub sidebar_open: bool,
    /// Share of the workbench height given to the request builder.
    pub split: f32,
    /// Path of the selected environment (F08-AC9).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// The workspace folder chosen with `Open Workspace…` (F09-AC1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
}

impl Session {
    /// The current format version.
    pub const VERSION: u32 = 1;
    /// Default request/response split.
    pub const DEFAULT_SPLIT: f32 = 0.45;
    /// Smallest and largest split (F14-AC4).
    pub const SPLIT_RANGE: (f32, f32) = (0.2, 0.8);
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            tabs: Vec::new(),
            active: 0,
            sidebar_open: true,
            split: Self::DEFAULT_SPLIT,
            environment: None,
            workspace: None,
        }
    }
}
