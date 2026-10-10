use calendar_gpui::calendar::CalendarView;

#[test]
fn calendar_opens_in_week_view() {
    assert_eq!(CalendarView::default(), CalendarView::Week);
}
