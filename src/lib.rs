//! Shared calendar foundation, independent of the desktop interface.

pub mod calendar;
pub mod mcp;

#[cfg(feature = "desktop")]
pub mod ui;
