//! Top-level window: Phase 0 empty state.

use iced::widget::{center, column, text};

use crate::Element;
use crate::probe::Probe;

/// Messages of the top-level window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// A frame was rendered (only while probing startup).
    Frame,
}

/// Top-level UI state.
#[derive(Debug, Default)]
pub struct Shell {
    probe: Probe,
}

impl Shell {
    /// Initial state, reading probe settings from the environment.
    #[must_use]
    pub fn boot() -> Self {
        Self {
            probe: Probe::from_env(),
        }
    }

    /// Apply a message.
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Frame => self.probe.frame(),
        }
    }

    /// Render the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        center(
            column![
                text("Ferry").size(28),
                text("No request open. Press Ctrl+T to start one.").size(14),
            ]
            .spacing(8)
            .align_x(iced::Center),
        )
        .into()
    }

    /// Subscriptions: startup probe only.
    pub fn subscription(&self) -> iced::Subscription<Message> {
        self.probe.subscription(|| Message::Frame)
    }
}
