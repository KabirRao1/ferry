//! Top-level window: wires [`App`] to iced, runs effects, handles shortcuts.

use ferry_app::{App, Client};
use iced::keyboard::{self, Key, key::Named};
use iced::{Subscription, Task, Theme};

use crate::probe::Probe;
use crate::{Element, screen};

/// Messages of the top-level window.
#[derive(Debug, Clone)]
pub enum Message {
    App(ferry_app::Message),
    /// A frame was rendered (only while probing startup).
    Frame,
}

/// Top-level UI state.
pub struct Shell {
    app: App,
    client: Client,
    probe: Probe,
}

impl Shell {
    /// Initial state, reading probe settings from the environment.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            app: App::default(),
            client,
            probe: Probe::from_env(),
        }
    }

    /// Apply a message; sends run as background tasks so the UI never blocks (F05-AC7).
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Frame => {
                self.probe.frame();
                Task::none()
            }
            Message::App(message) => match self.app.update(message) {
                Some(effect) => {
                    Task::perform(ferry_app::run(effect, self.client.clone()), Message::App)
                }
                None => Task::none(),
            },
        }
    }

    /// Render the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        screen::view(&self.app).map(Message::App)
    }

    /// Ctrl/Cmd+Enter anywhere sends; plus the startup probe.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().filter_map(shortcut),
            self.probe.subscription(|| Message::Frame),
        ])
    }

    #[must_use]
    pub fn theme(&self) -> Theme {
        Theme::Dark
    }
}

/// Global keyboard shortcuts (F05-AC1: Ctrl/Cmd+Enter sends).
#[must_use]
pub fn shortcut(event: keyboard::Event) -> Option<Message> {
    match event {
        keyboard::Event::KeyPressed {
            key: Key::Named(Named::Enter),
            modifiers,
            ..
        } if modifiers.command() => Some(Message::App(ferry_app::Message::SendPressed)),
        _ => None,
    }
}
