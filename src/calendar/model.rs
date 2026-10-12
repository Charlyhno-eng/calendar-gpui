use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{CalendarError, Result, validate_date};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Event,
    Task,
}

/// A draft is validated before it can become a saved item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDraft {
    pub title: String,
    pub description: String,
    pub kind: ItemKind,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub timezone: Tz,
    pub url: String,
    pub completed: bool,
}

impl ItemDraft {
    pub fn validate(&self) -> Result<()> {
        ItemValidation {
            title: &self.title,
            description: &self.description,
            kind: self.kind,
            start: self.start,
            end: self.end,
            timezone: self.timezone,
            url: &self.url,
            completed: self.completed,
        }
        .validate()
    }
}

/// Borrow text during validation, including when checking a loaded document.
struct ItemValidation<'a> {
    title: &'a str,
    description: &'a str,
    kind: ItemKind,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timezone: Tz,
    url: &'a str,
    completed: bool,
}

impl ItemValidation<'_> {
    fn validate(&self) -> Result<()> {
        let title = self.title.trim();
        if title.is_empty()
            || title.chars().count() > 200
            || self.title.chars().any(char::is_control)
        {
            return Err(CalendarError(
                "Title must contain 1–200 characters without line breaks.".into(),
            ));
        }
        if self.description.chars().count() > 10_000 {
            return Err(CalendarError(
                "Description must contain at most 10,000 characters.".into(),
            ));
        }
        validate_date(self.start.with_timezone(&self.timezone).date_naive())?;
        validate_date(self.end.with_timezone(&self.timezone).date_naive())?;
        if self.end <= self.start {
            return Err(CalendarError("End must be after start.".into()));
        }
        if !self.url.trim().is_empty() {
            if self.url.len() > 4096 || self.url.chars().any(char::is_control) {
                return Err(CalendarError(
                    "Links must be at most 4096 bytes and contain no control characters.".into(),
                ));
            }
            let url = Url::parse(self.url.trim())
                .map_err(|_| CalendarError("Enter a valid HTTP or HTTPS URL.".into()))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(CalendarError(
                    "Links must be HTTP/HTTPS URLs without embedded credentials.".into(),
                ));
            }
        }
        if self.kind == ItemKind::Event && self.completed {
            return Err(CalendarError("Only tasks can be marked complete.".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarItem {
    id: u64,
    title: String,
    description: String,
    kind: ItemKind,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timezone: Tz,
    url: String,
    completed: bool,
}

impl CalendarItem {
    fn from_draft(id: u64, draft: ItemDraft) -> Self {
        Self {
            id,
            title: draft.title.trim().to_owned(),
            description: draft.description,
            kind: draft.kind,
            start: draft.start,
            end: draft.end,
            timezone: draft.timezone,
            url: draft.url.trim().to_owned(),
            completed: draft.completed,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn kind(&self) -> ItemKind {
        self.kind
    }
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }
    pub fn timezone(&self) -> Tz {
        self.timezone
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn draft(&self) -> ItemDraft {
        ItemDraft {
            title: self.title.clone(),
            description: self.description.clone(),
            kind: self.kind,
            start: self.start,
            end: self.end,
            timezone: self.timezone,
            url: self.url.clone(),
            completed: self.completed,
        }
    }
}

/// Versioned document. Times are stored as UTC instants; the original zone is retained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    version: u32,
    next_id: u64,
    timezone: Tz,
    show_holidays: bool,
    items: Vec<CalendarItem>,
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            timezone: chrono_tz::Europe::Paris,
            show_holidays: true,
            items: Vec::new(),
        }
    }
}

impl Calendar {
    pub fn timezone(&self) -> Tz {
        self.timezone
    }
    pub fn set_timezone(&mut self, timezone: Tz) {
        self.timezone = timezone;
    }
    pub fn show_holidays(&self) -> bool {
        self.show_holidays
    }
    pub fn set_show_holidays(&mut self, show: bool) {
        self.show_holidays = show;
    }
    pub fn items(&self) -> &[CalendarItem] {
        &self.items
    }
    pub fn item(&self, id: u64) -> Result<&CalendarItem> {
        self.items
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| CalendarError("This calendar item no longer exists.".into()))
    }
    pub fn create(&mut self, draft: ItemDraft) -> Result<u64> {
        draft.validate()?;
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or_else(|| CalendarError("No more item IDs available.".into()))?;
        self.items.push(CalendarItem::from_draft(id, draft));
        Ok(id)
    }
    pub fn update(&mut self, id: u64, draft: ItemDraft) -> Result<()> {
        draft.validate()?;
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| CalendarError("This calendar item no longer exists.".into()))?;
        *item = CalendarItem::from_draft(id, draft);
        Ok(())
    }
    pub fn delete(&mut self, id: u64) -> Result<()> {
        self.item(id)?;
        self.items.retain(|item| item.id != id);
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(CalendarError("Unsupported calendar file version.".into()));
        }
        let mut ids = HashSet::with_capacity(self.items.len());
        for item in &self.items {
            ItemValidation {
                title: &item.title,
                description: &item.description,
                kind: item.kind,
                start: item.start,
                end: item.end,
                timezone: item.timezone,
                url: &item.url,
                completed: item.completed,
            }
            .validate()?;
            if item.id == 0 || item.id >= self.next_id || !ids.insert(item.id) {
                return Err(CalendarError(
                    "Calendar file has invalid or duplicate IDs.".into(),
                ));
            }
        }
        if self.next_id == 0 {
            return Err(CalendarError(
                "Calendar file has an invalid next ID.".into(),
            ));
        }
        Ok(())
    }

    /// End is exclusive, so an item ending at midnight does not appear the next day.
    pub fn items_on(&self, date: NaiveDate) -> Vec<&CalendarItem> {
        let zone = self.timezone;
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|item| {
                let start_date = item.start.with_timezone(&zone).date_naive();
                let end_date = (item.end - Duration::nanoseconds(1))
                    .with_timezone(&zone)
                    .date_naive();
                start_date <= date && end_date >= date
            })
            .collect();
        items.sort_by_key(|item| (item.start, item.id));
        items
    }

    /// Query a whole grid in one scan, converting each item's dates only once.
    /// Results follow the requested dates (including duplicates), with items
    /// sorted by start instant and ID, just like `items_on`.
    pub fn items_on_days(&self, dates: &[NaiveDate]) -> Vec<Vec<&CalendarItem>> {
        let mut result = vec![Vec::new(); dates.len()];
        if dates.is_empty() {
            return result;
        }
        let mut ordered_dates: Vec<_> = dates.iter().copied().zip(0..dates.len()).collect();
        ordered_dates.sort_unstable_by_key(|&(date, _)| date);
        for item in &self.items {
            let start = item.start.with_timezone(&self.timezone).date_naive();
            let end = (item.end - Duration::nanoseconds(1))
                .with_timezone(&self.timezone)
                .date_naive();
            // A historical offset rollback can make local dates run backwards.
            // Preserve items_on's membership rule instead of slicing backwards.
            if start > end {
                continue;
            }
            let first = ordered_dates.partition_point(|&(date, _)| date < start);
            let last = ordered_dates.partition_point(|&(date, _)| date <= end);
            for &(_, index) in &ordered_dates[first..last] {
                result[index].push(item);
            }
        }
        for items in &mut result {
            items.sort_unstable_by_key(|item| (item.start, item.id));
        }
        result
    }
}

/// A visible slice of an item on one day, measured in wall-clock minutes.
#[derive(Debug, Clone, PartialEq)]
pub struct DaySegment {
    pub id: u64,
    pub start_minute: f32,
    pub end_minute: f32,
    pub continues_before: bool,
    pub continues_after: bool,
    pub column: usize,
    pub columns: usize,
}

pub fn day_segments(calendar: &Calendar, date: NaiveDate) -> Vec<DaySegment> {
    day_layout(&calendar.items_on(date), date, calendar.timezone())
        .into_iter()
        .map(|(_, segment)| segment)
        .collect()
}

/// Lay out items already queried for this date, keeping their references with
/// the segments so rendering needs neither another scan nor ID lookups.
pub fn day_layout<'a>(
    items: &[&'a CalendarItem],
    date: NaiveDate,
    zone: Tz,
) -> Vec<(&'a CalendarItem, DaySegment)> {
    let mut segments: Vec<_> = items
        .iter()
        .map(|&item| {
            let start = item.start.with_timezone(&zone);
            let end = item.end.with_timezone(&zone);
            let before = start.date_naive() < date;
            let after = end.date_naive() > date;
            let start_minute = if before {
                0.0
            } else {
                (start.hour() * 60 + start.minute()) as f32
            };
            let end_minute = if after {
                1440.0
            } else {
                (end.hour() * 60 + end.minute()) as f32
            };
            // During a clock fold the wall-clock end may precede the start.
            // Keep the card visible; its labels retain the actual UTC offsets.
            (
                item,
                DaySegment {
                    id: item.id,
                    start_minute,
                    end_minute: end_minute.max(start_minute + 20.0).min(1440.0),
                    continues_before: before,
                    continues_after: (item.end - Duration::nanoseconds(1))
                        .with_timezone(&zone)
                        .date_naive()
                        > date,
                    column: 0,
                    columns: 1,
                },
            )
        })
        .collect();
    segments.sort_unstable_by(|(_, a), (_, b)| {
        a.start_minute
            .total_cmp(&b.start_minute)
            .then(a.id.cmp(&b.id))
    });

    // Connected overlap groups share a column width. Adjacent intervals can reuse a column.
    let mut group_start = 0;
    while group_start < segments.len() {
        let mut group_end = group_start + 1;
        let mut last_end = segments[group_start].1.end_minute;
        while group_end < segments.len() && segments[group_end].1.start_minute < last_end {
            last_end = last_end.max(segments[group_end].1.end_minute);
            group_end += 1;
        }
        let mut column_ends: Vec<f32> = Vec::new();
        for (_, segment) in &mut segments[group_start..group_end] {
            let column = column_ends
                .iter()
                .position(|end| *end <= segment.start_minute)
                .unwrap_or(column_ends.len());
            if column == column_ends.len() {
                column_ends.push(segment.end_minute);
            } else {
                column_ends[column] = segment.end_minute;
            }
            segment.column = column;
        }
        for (_, segment) in &mut segments[group_start..group_end] {
            segment.columns = column_ends.len();
        }
        group_start = group_end;
    }
    segments
}
