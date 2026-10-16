//! The saved workbench: open tabs and layout (F07-AC5, F14-AC4).

use serde::{Deserialize, Serialize};

use crate::draft::RequestDraft;

// CONTRACT
/// Everything restored on the next start. Responses are not saved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Format version, for future migrations.
    pub version: u32,
    pub tabs: Vec<RequestDraft>,
    /// Index into `tabs` of the active tab.
    pub active: usize,
    pub sidebar_open: bool,
    /// Share of the workbench height given to the request builder.
    pub split: f32,
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
        }
    }
}
