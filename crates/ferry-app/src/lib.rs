//! Ferry application state, messages and `update()` logic.
//!
//! Everything here is testable headless; `ferry-ui` only renders it and runs
//! the [`Effect`]s that `update()` returns.

mod app;
pub mod body_view;

pub use app::{App, Effect, Finished, Message, Phase, Received, ResponseTab, WRAP_BYTES, run};
pub use bytes::Bytes;
pub use ferry_core::error::SendError;
pub use ferry_core::format;
pub use ferry_core::request::Method;
pub use ferry_core::response::{Header, Response};
pub use ferry_core::text::IndexError;
pub use ferry_http::{Client, Config};
