//! Dependency-free query benchmark; run with --release --no-default-features.
use std::{hint::black_box, time::Instant};

use calendar_gpui::calendar::{Calendar, ItemDraft, ItemKind, month_days};
use chrono::{Duration, NaiveDate};
use chrono_tz::Europe::Paris;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selected = NaiveDate::from_ymd_opt(2026, 10, 10).expect("valid benchmark date");
    let start = selected.and_hms_opt(8, 0, 0).expect("valid time").and_utc();
    let mut calendar = Calendar::default();
    for index in 0..10_000 {
        let start = start + Duration::days(index % 730 - 365);
        calendar.create(ItemDraft {
            title: format!("Item {index}"),
            description: "Description".repeat(100),
            kind: ItemKind::Event,
            start,
            end: start + Duration::hours(1 + index % 48),
            timezone: Paris,
            url: String::new(),
            completed: false,
        })?;
    }
    let days = month_days(selected);
    let started = Instant::now();
    for _ in 0..100 {
        for date in &days {
            black_box(calendar.items_on(black_box(*date)));
        }
    }
    let individual = started.elapsed();
    let started = Instant::now();
    for _ in 0..100 {
        black_box(calendar.items_on_days(black_box(&days)));
    }
    let batched = started.elapsed();
    println!("100 month queries, 10,000 items: individual={individual:?}, batched={batched:?}");
    println!(
        "Query speedup: {:.2}x",
        individual.as_secs_f64() / batched.as_secs_f64()
    );
    Ok(())
}
