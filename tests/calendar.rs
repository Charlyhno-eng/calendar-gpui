use calendar_gpui::calendar::*;
use chrono::{Duration, NaiveDate};
use chrono_tz::{Europe::Paris, UTC};

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn draft(start: &str, end: &str) -> ItemDraft {
    ItemDraft {
        title: "Team meeting".into(),
        description: "Discuss the roadmap".into(),
        kind: ItemKind::Event,
        start: resolve_local(start, Paris, Occurrence::Reject).unwrap(),
        end: resolve_local(end, Paris, Occurrence::Reject).unwrap(),
        timezone: Paris,
        url: "https://meet.google.com/abc-defg-hij".into(),
        completed: false,
    }
}

#[test]
fn create_update_delete_preserves_ids_and_validates_before_mutating() {
    let mut calendar = Calendar::default();
    let id = calendar
        .create(draft("2026-10-10 09:00", "2026-10-10 10:00"))
        .unwrap();
    let mut change = calendar.item(id).unwrap().draft();
    change.title = "  Updated task  ".into();
    change.kind = ItemKind::Task;
    change.completed = true;
    calendar.update(id, change).unwrap();
    assert_eq!(calendar.item(id).unwrap().title(), "Updated task");
    assert!(calendar.item(id).unwrap().completed());

    let mut invalid = calendar.item(id).unwrap().draft();
    invalid.end = invalid.start;
    assert!(calendar.update(id, invalid.clone()).is_err());
    assert!(calendar.create(invalid).is_err());
    assert_eq!(calendar.items().len(), 1);
    assert_eq!(calendar.item(id).unwrap().title(), "Updated task");
    calendar.delete(id).unwrap();
    assert!(calendar.delete(id).is_err());
    let next = calendar
        .create(draft("2026-10-10 09:00", "2026-10-10 10:00"))
        .unwrap();
    assert!(next > id);
}

#[test]
fn invalid_titles_intervals_links_and_event_completion_are_rejected() {
    let base = draft("2026-10-10 09:00", "2026-10-10 10:00");
    for title in [" ", "bad\ntitle", &"a".repeat(201)] {
        let mut item = base.clone();
        item.title = title.into();
        assert!(item.validate().is_err());
    }
    for link in [
        "not a URL",
        "https://exam\nple.com",
        "javascript:alert(1)",
        "file:///tmp/a",
        "https://user:pass@example.com",
    ] {
        let mut item = base.clone();
        item.url = link.into();
        assert!(item.validate().is_err(), "{link}");
    }
    let mut item = base.clone();
    item.end = item.start - Duration::minutes(1);
    assert!(item.validate().is_err());
    item = base;
    item.completed = true;
    assert!(item.validate().is_err());
    item.kind = ItemKind::Task;
    assert!(item.validate().is_ok());
    item.url.clear();
    assert!(item.validate().is_ok());
}

#[test]
fn clock_gaps_and_folds_require_deliberate_resolution() {
    assert!(resolve_local("2026-03-29 02:30", Paris, Occurrence::Earlier).is_err());
    assert!(resolve_local("2026-10-25 02:30", Paris, Occurrence::Reject).is_err());
    let earlier = resolve_local("2026-10-25 02:30", Paris, Occurrence::Earlier).unwrap();
    let later = resolve_local("2026-10-25 02:30", Paris, Occurrence::Later).unwrap();
    assert_eq!(later - earlier, Duration::hours(1));
    assert_eq!(
        occurrence_at(earlier + Duration::seconds(15), Paris),
        Occurrence::Earlier
    );
    assert_eq!(
        occurrence_at(later + Duration::seconds(15), Paris),
        Occurrence::Later
    );
    assert_eq!(occurrence_at(later, UTC), Occurrence::Reject);
    assert_eq!(earlier.to_rfc3339(), "2026-10-25T00:30:00+00:00");
    assert_eq!(later.to_rfc3339(), "2026-10-25T01:30:00+00:00");
    assert!(resolve_local("2026-02-30 09:00", Paris, Occurrence::Reject).is_err());
    assert!(resolve_local("2101-01-01 09:00", Paris, Occurrence::Reject).is_err());
    assert!(parse_zone("invalid/zone").is_err());
}

#[test]
fn intervals_crossing_dst_use_elapsed_time() {
    let item = draft("2026-03-29 01:30", "2026-03-29 03:30");
    assert_eq!(item.end - item.start, Duration::hours(1));
    assert!(item.validate().is_ok());
}

#[test]
fn day_queries_split_overnight_items_and_exclude_midnight_end() {
    let mut calendar = Calendar::default();
    let midnight = calendar
        .create(draft("2026-10-10 23:00", "2026-10-11 00:00"))
        .unwrap();
    let overnight = calendar
        .create(draft("2026-10-10 22:00", "2026-10-12 01:00"))
        .unwrap();
    assert_eq!(calendar.items_on(date("2026-10-10")).len(), 2);
    assert_eq!(calendar.items_on(date("2026-10-11")).len(), 1);
    assert_eq!(calendar.items_on(date("2026-10-12"))[0].id(), overnight);
    assert!(calendar.items_on(date("2026-10-13")).is_empty());
    let segments = day_segments(&calendar, date("2026-10-10"));
    assert_eq!(
        segments
            .iter()
            .find(|segment| segment.id == midnight)
            .unwrap()
            .end_minute,
        1440.0
    );
    assert!(
        !segments
            .iter()
            .find(|segment| segment.id == midnight)
            .unwrap()
            .continues_after
    );
    let middle = day_segments(&calendar, date("2026-10-11"));
    assert_eq!(
        (middle[0].start_minute, middle[0].end_minute),
        (0.0, 1440.0)
    );
    assert!(middle[0].continues_before && middle[0].continues_after);
}

#[test]
fn batched_queries_preserve_order_duplicates_boundaries_and_display_zone() {
    let mut calendar = Calendar::default();
    let first = calendar
        .create(draft("2026-10-24 23:00", "2026-10-25 00:00"))
        .unwrap();
    let long = calendar
        .create(draft("2026-10-24 23:00", "2026-10-26 01:00"))
        .unwrap();
    let mut folded = draft("2026-10-25 01:00", "2026-10-25 04:00");
    folded.start = resolve_local("2026-10-25 02:45", Paris, Occurrence::Earlier).unwrap();
    folded.end = resolve_local("2026-10-25 02:15", Paris, Occurrence::Later).unwrap();
    let fold = calendar.create(folded).unwrap();
    let dates = [
        date("2026-10-26"),
        date("2026-10-24"),
        date("2026-10-25"),
        date("2026-10-24"),
        date("2026-10-27"),
    ];
    let ids = |days: Vec<Vec<&CalendarItem>>| {
        days.into_iter()
            .map(|items| items.into_iter().map(CalendarItem::id).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(calendar.items_on_days(&dates)),
        vec![
            vec![long],
            vec![first, long],
            vec![long, fold],
            vec![first, long],
            vec![]
        ]
    );
    assert!(calendar.items_on_days(&[]).is_empty());
    for zone in [Paris, UTC, chrono_tz::America::New_York] {
        calendar.set_timezone(zone);
        let expected = dates.iter().map(|&day| calendar.items_on(day)).collect();
        assert_eq!(ids(calendar.items_on_days(&dates)), ids(expected));
    }
    let spring = calendar
        .create(draft("2026-03-29 01:30", "2026-03-29 03:30"))
        .unwrap();
    calendar.set_timezone(Paris);
    assert_eq!(
        ids(calendar.items_on_days(&[date("2026-03-28"), date("2026-03-29"), date("2026-03-30")])),
        vec![vec![], vec![spring], vec![]]
    );
}

#[test]
fn display_zone_changes_day_membership_without_changing_instants() {
    let mut calendar = Calendar::default();
    let id = calendar
        .create(draft("2026-10-10 00:30", "2026-10-10 01:30"))
        .unwrap();
    let instant = calendar.item(id).unwrap().start();
    calendar.set_timezone(UTC);
    assert_eq!(calendar.items_on(date("2026-10-09"))[0].id(), id);
    assert!(calendar.items_on(date("2026-10-10")).is_empty());
    assert_eq!(calendar.item(id).unwrap().start(), instant);
    assert_eq!(calendar.item(id).unwrap().timezone(), Paris);
}

#[test]
fn overlapping_cards_get_distinct_columns_and_adjacent_items_reuse_columns() {
    let mut calendar = Calendar::default();
    let a = calendar
        .create(draft("2026-10-10 09:00", "2026-10-10 10:00"))
        .unwrap();
    let b = calendar
        .create(draft("2026-10-10 09:30", "2026-10-10 11:00"))
        .unwrap();
    let c = calendar
        .create(draft("2026-10-10 10:00", "2026-10-10 10:30"))
        .unwrap();
    let d = calendar
        .create(draft("2026-10-10 12:00", "2026-10-10 13:00"))
        .unwrap();
    let segments = day_segments(&calendar, date("2026-10-10"));
    let segment = |id| segments.iter().find(|s| s.id == id).unwrap();
    assert_ne!(segment(a).column, segment(b).column);
    assert_eq!(segment(a).column, segment(c).column);
    assert_eq!(segment(a).columns, 2);
    assert_eq!(segment(d).columns, 1);
}

#[test]
fn a_fold_interval_remains_visible_even_when_wall_clock_runs_backwards() {
    let mut item = draft("2026-10-25 01:00", "2026-10-25 04:00");
    item.start = resolve_local("2026-10-25 02:45", Paris, Occurrence::Earlier).unwrap();
    item.end = resolve_local("2026-10-25 02:15", Paris, Occurrence::Later).unwrap();
    let mut calendar = Calendar::default();
    calendar.create(item).unwrap();
    let segments = day_segments(&calendar, date("2026-10-25"));
    assert!(segments[0].end_minute > segments[0].start_minute);
}

#[test]
fn week_and_month_navigation_handle_leap_years_and_year_boundaries() {
    assert_eq!(week_start(date("2026-01-01")), date("2025-12-29"));
    assert_eq!(
        navigate(date("2024-01-31"), CalendarView::Month, true).unwrap(),
        date("2024-02-29")
    );
    assert_eq!(
        navigate(date("2026-01-31"), CalendarView::Month, true).unwrap(),
        date("2026-02-28")
    );
    assert_eq!(
        navigate(date("2026-12-15"), CalendarView::Month, true).unwrap(),
        date("2027-01-15")
    );
    assert_eq!(
        navigate(date("2026-01-15"), CalendarView::Month, false).unwrap(),
        date("2025-12-15")
    );
    assert_eq!(
        navigate(date("2026-12-30"), CalendarView::Week, true).unwrap(),
        date("2027-01-06")
    );
    assert!(navigate(date("2100-12-31"), CalendarView::Month, true).is_err());
    assert!(navigate(date("1900-01-01"), CalendarView::Week, false).is_err());
    let grid = month_days(date("2026-02-12"));
    assert_eq!(grid.len(), 42);
    assert_eq!(grid[0], date("2026-01-26"));
    assert!(is_weekend(date("2026-10-10")));
    assert!(!is_weekend(date("2026-10-09")));
}

#[test]
fn french_holidays_include_movable_dates_and_coincident_names() {
    assert_eq!(holidays(date("2026-04-06")), vec!["Easter Monday"]);
    assert_eq!(holidays(date("2026-05-14")), vec!["Ascension Day"]);
    assert_eq!(holidays(date("2026-05-25")), vec!["Whit Monday"]);
    assert_eq!(holidays(date("2026-07-14")), vec!["Bastille Day"]);
    assert!(holidays(date("2026-12-26")).is_empty());
    assert_eq!(
        holidays(date("2008-05-01")),
        vec!["Labour Day", "Ascension Day"]
    );
}

#[test]
fn atomic_storage_round_trip_retains_tasks_zones_and_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/calendar.json");
    let (store, mut calendar) = CalendarStore::open(&path).unwrap();
    let mut task = draft("2026-10-10 09:00", "2026-10-10 10:00");
    task.kind = ItemKind::Task;
    task.completed = true;
    let id = calendar.create(task).unwrap();
    calendar.set_timezone(UTC);
    calendar.set_show_holidays(false);
    store.save(&calendar).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&calendar).unwrap(),
        "streamed writes retain the existing JSON format"
    );
    assert!(
        CalendarStore::open(&path).is_err(),
        "second writer must be blocked"
    );
    drop(store);
    let (store, loaded) = CalendarStore::open(&path).unwrap();
    assert!(loaded.item(id).unwrap().completed());
    assert_eq!(
        loaded.item(id).unwrap().url(),
        "https://meet.google.com/abc-defg-hij"
    );
    assert_eq!(loaded.timezone(), UTC);
    assert!(!loaded.show_holidays());
    assert_eq!(loaded.item(id).unwrap().timezone(), Paris);
    store.save(&loaded).unwrap();
    drop(store);
    assert!(CalendarStore::open(path).is_ok());
}

#[test]
fn corrupt_or_unsupported_files_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    for content in [
        "not JSON",
        r#"{"version":2,"next_id":1,"timezone":"UTC","show_holidays":true,"items":[]}"#,
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(CalendarStore::open(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
}

#[test]
fn loaded_documents_reject_duplicate_ids_invalid_times_and_links() {
    let mut calendar = Calendar::default();
    calendar
        .create(draft("2026-10-10 09:00", "2026-10-10 10:00"))
        .unwrap();
    let base = serde_json::to_value(&calendar).unwrap();
    let mut invalids = Vec::new();
    let mut duplicate = base.clone();
    let item = duplicate["items"][0].clone();
    duplicate["items"].as_array_mut().unwrap().push(item);
    invalids.push(duplicate);
    let mut invalid = base.clone();
    invalid["items"][0]["end"] = invalid["items"][0]["start"].clone();
    invalids.push(invalid);
    let mut invalid = base;
    invalid["items"][0]["url"] = serde_json::json!("javascript:alert(1)");
    invalids.push(invalid);
    for invalid in invalids {
        let loaded: Calendar = serde_json::from_value(invalid).unwrap();
        assert!(loaded.validate().is_err());
    }
}

#[test]
fn failed_save_preserves_the_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calendar.json");
    let (store, calendar) = CalendarStore::open(&path).unwrap();
    store.save(&calendar).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut invalid = serde_json::to_value(&calendar).unwrap();
    invalid["next_id"] = serde_json::json!(0);
    let invalid: Calendar = serde_json::from_value(invalid).unwrap();
    assert!(store.save(&invalid).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}
