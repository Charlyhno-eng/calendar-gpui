//! Shared calendar foundation, independent of the desktop interface.

pub mod calendar;

#[cfg(feature = "desktop")]
pub mod ui;
