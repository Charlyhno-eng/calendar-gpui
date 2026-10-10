//! Validated calendar data, navigation, time-zone conversion and holidays.

mod dates;
mod holidays;
mod model;
mod storage;

pub use dates::*;
pub use holidays::*;
pub use model::*;
pub use storage::*;

use std::fmt;

/// Errors are surfaced to callers and never treated as a successful change.
#[derive(Debug)]
pub struct CalendarError(pub String);

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CalendarError {}

pub type Result<T> = std::result::Result<T, CalendarError>;

/// The calendar layout selected by the user.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CalendarView {
    #[default]
    Week,
    Month,
}

impl CalendarView {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Week => "Week",
            Self::Month => "Month",
        }
    }
}
