//! Ferry views and widgets.
//!
//! Views are pure functions of `ferry-app` state. No business logic lives here.

pub mod body_view;
pub mod probe;
pub mod screen;
pub mod shell;

/// The `iced` element type used across Ferry views.
pub type Element<'a, Message> = iced::Element<'a, Message>;
