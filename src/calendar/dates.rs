use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use super::{CalendarError, CalendarView, Result};

pub const MIN_YEAR: i32 = 1900;
pub const MAX_YEAR: i32 = 2100;

pub fn validate_date(date: NaiveDate) -> Result<()> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&date.year()) {
        return Err(CalendarError(format!(
            "Dates must be between {MIN_YEAR} and {MAX_YEAR}."
        )));
    }
    Ok(())
}

pub fn parse_zone(value: &str) -> Result<Tz> {
    value.trim().parse().map_err(|_| {
        CalendarError("Use an IANA time zone, for example Europe/Paris or America/New_York.".into())
    })
}

/// Repeated wall-clock times require an explicit occurrence.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Occurrence {
    #[default]
    Reject,
    Earlier,
    Later,
}

/// Determine the occurrence of a saved instant, including sub-minute precision.
pub fn occurrence_at(instant: DateTime<Utc>, zone: Tz) -> Occurrence {
    match zone.from_local_datetime(&instant.with_timezone(&zone).naive_local()) {
        LocalResult::Ambiguous(first, second) => {
            if first.min(second).with_timezone(&Utc) == instant {
                Occurrence::Earlier
            } else {
                Occurrence::Later
            }
        }
        _ => Occurrence::Reject,
    }
}

/// Parse a local date and time. Gaps are rejected; folds require a choice.
pub fn resolve_local(value: &str, zone: Tz, occurrence: Occurrence) -> Result<DateTime<Utc>> {
    let local = NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M")
        .map_err(|_| CalendarError("Use date and time in YYYY-MM-DD HH:MM format.".into()))?;
    validate_date(local.date())?;
    match zone.from_local_datetime(&local) {
        LocalResult::Single(time) => Ok(time.with_timezone(&Utc)),
        LocalResult::Ambiguous(first, second) => match occurrence {
            Occurrence::Earlier => Ok(first.min(second).with_timezone(&Utc)),
            Occurrence::Later => Ok(first.max(second).with_timezone(&Utc)),
            Occurrence::Reject => Err(CalendarError(format!(
                "{value} occurs twice in {zone}. Choose the earlier or later occurrence."
            ))),
        },
        LocalResult::None => Err(CalendarError(format!(
            "{value} does not exist in {zone} because the clocks change. Choose another time."
        ))),
    }
}

pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// A consistent six-row, Monday-first month grid, including neighboring days.
pub fn month_days(date: NaiveDate) -> Vec<NaiveDate> {
    let start = week_start(date.with_day(1).expect("first day exists"));
    (0..42)
        .map(|offset| start + Duration::days(offset))
        .collect()
}

pub fn visible_days(date: NaiveDate, view: CalendarView) -> Vec<NaiveDate> {
    match view {
        CalendarView::Week => {
            let start = week_start(date);
            (0..7)
                .map(|offset| start + Duration::days(offset))
                .collect()
        }
        CalendarView::Month => month_days(date),
    }
}

/// Month movement clamps the selected day (Jan 31 -> Feb 28/29).
pub fn navigate(date: NaiveDate, view: CalendarView, forward: bool) -> Result<NaiveDate> {
    let next = match view {
        CalendarView::Week => date.checked_add_signed(Duration::days(if forward { 7 } else { -7 })),
        CalendarView::Month => {
            let month_index =
                date.year() * 12 + date.month0() as i32 + if forward { 1 } else { -1 };
            let year = month_index.div_euclid(12);
            let month = month_index.rem_euclid(12) as u32 + 1;
            (1..=date.day())
                .rev()
                .find_map(|day| NaiveDate::from_ymd_opt(year, month, day))
        }
    }
    .ok_or_else(|| CalendarError("The requested date is out of range.".into()))?;
    validate_date(next)?;
    Ok(next)
}

pub fn is_weekend(date: NaiveDate) -> bool {
    date.weekday().num_days_from_monday() >= 5
}
