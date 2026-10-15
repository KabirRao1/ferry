//! Startup instrumentation used by `cargo xtask budgets`.
//!
//! When `FERRY_PROBE` is set, the first rendered frame prints
//! [`INTERACTIVE_MARKER`] to stdout. The harness times process spawn to
//! marker and samples memory afterwards. Without `FERRY_PROBE` this is inert.

use std::io::Write;

use iced::Subscription;

/// Line printed on the first frame when probing.
pub const INTERACTIVE_MARKER: &str = "FERRY_PROBE interactive";

/// Tracks whether the first frame is still pending.
#[derive(Debug, Clone, Copy, Default)]
pub struct Probe {
    awaiting_first_frame: bool,
}

impl Probe {
    /// A probe that is active iff `FERRY_PROBE` is set.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            awaiting_first_frame: std::env::var_os("FERRY_PROBE").is_some(),
        }
    }

    /// Frame events while the first frame is pending; nothing afterwards, so
    /// an idle window does not keep redrawing.
    pub fn subscription<M: 'static>(&self, on_frame: fn() -> M) -> Subscription<M> {
        if self.awaiting_first_frame {
            iced::window::frames().with(on_frame).map(|(f, _)| f())
        } else {
            Subscription::none()
        }
    }

    /// Record a frame; prints the marker once.
    pub fn frame(&mut self) {
        if std::mem::take(&mut self.awaiting_first_frame) {
            let mut out = std::io::stdout().lock();
            // A failed write only loses instrumentation; the app keeps running.
            let _ = writeln!(out, "{INTERACTIVE_MARKER}");
            let _ = out.flush();
        }
    }
}
