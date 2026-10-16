//! Ferry domain types and pure logic.
//!
//! This crate performs no I/O. Clocks and ID generation are injected by callers.
#![warn(clippy::pedantic)]

pub mod auth;
pub mod body;
pub mod draft;
pub mod error;
pub mod format;
pub mod json;
pub mod kv;
pub mod params;
pub mod request;
pub mod response;
pub mod session;
pub mod text;
