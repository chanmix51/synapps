#![warn(missing_docs)]

//! # SynApps
//! This crate provides a simple event dispatcher that allows you to subscribe
//! to events and dispatch them to multiple listeners.

/// The `event` module provides the `Event` type and its associated methods.
mod event;

/// publish symbols from the `event` module.
pub use event::*;

/// This crate does not use Anyhow, but it is a common pattern to define a custom Result type.
type StdResult<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;
