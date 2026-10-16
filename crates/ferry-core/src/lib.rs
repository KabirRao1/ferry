//! Ferry domain types and pure logic.
//!
//! This crate performs no I/O. Clocks and ID generation are injected by callers.
#![warn(clippy::pedantic)]

pub mod error;
pub mod format;
pub mod request;
pub mod response;
pub mod text;
