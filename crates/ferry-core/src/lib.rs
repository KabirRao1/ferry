//! Ferry domain types and pure logic.
//!
//! This crate performs no I/O. Clocks and ID generation are injected by callers.
#![warn(clippy::pedantic)]

pub mod auth;
pub mod body;
pub mod collection;
pub mod cookies;
pub mod draft;
pub mod environment;
pub mod error;
pub mod format;
pub mod history;
pub mod json;
pub mod kv;
pub mod names;
pub mod params;
pub mod request;
pub mod response;
pub mod scripts;
pub mod session;
pub mod text;
pub mod time;
pub mod variables;
pub mod workspace;

/// Whether `value` equals its type's default, for omitting default fields
/// from saved files.
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// For fields that default to `true`.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if signature
pub(crate) fn is_true(value: &bool) -> bool {
    *value
}
