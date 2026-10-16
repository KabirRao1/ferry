//! Ferry application state, messages and `update()` logic.
//!
//! Everything here is testable headless; `ferry-ui` only renders it and runs
//! the [`Effect`]s that `update()` returns.

mod app;
pub mod body_view;
mod tab;

pub use app::{App, Effect, Finished, Message, SidebarSection, WRAP_BYTES, received, run};
pub use bytes::Bytes;
pub use ferry_core::auth::{ApiKeyLocation, AuthKind};
pub use ferry_core::body::{BodyMode, RawLanguage};
pub use ferry_core::draft::{AutoHeader, RequestDraft};
pub use ferry_core::error::SendError;
pub use ferry_core::format;
pub use ferry_core::json::JsonError;
pub use ferry_core::kv::KeyValue;
pub use ferry_core::request::{Method, Request, RequestBody};
pub use ferry_core::response::{Header, Response, Timing};
pub use ferry_core::session::Session;
pub use ferry_core::text::IndexError;
pub use ferry_http::{Client, Config};
pub use ferry_store::{Loaded, RealFs, SessionStore, data_dir};
pub use tab::{
    BodyFormat, Edit, FileTarget, Phase, Received, RequestTab, ResponseTab, RowEdit, Tab, TabId,
    Table,
};
