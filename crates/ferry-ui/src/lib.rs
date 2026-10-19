//! Ferry views and widgets.
//!
//! Views are pure functions of `ferry-app` state, plus the widget state iced
//! keeps outside views (`editors`). No business logic lives here.

pub mod body_view;
pub mod chrome;
pub mod console;
pub mod dialogs;
pub mod docs;
pub mod editors;
pub mod preview;
pub mod probe;
pub mod request;
pub mod response;
pub mod screen;
pub mod shell;
pub mod sidebar;
pub mod theme;

/// The `iced` element type used across Ferry views.
pub type Element<'a, Message> = iced::Element<'a, Message>;
