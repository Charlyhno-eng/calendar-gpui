//! Calendar grids, agenda and editing forms.

use super::*;
use gpui::{AnyElement, Div, SharedString, Stateful, relative};

enum CardLayout {
    Agenda,
    Month(NaiveDate),
    Timeline(NaiveDate),
}

struct ItemTooltip {
    title: String,
    times: String,
    description: String,
}

impl Render for ItemTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(420.0))
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(ACCENT))
            .bg(gpui::rgba(0x1c1924f4))
            .text_color(rgb(TEXT))
            .text_size(px(12.0))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.title.clone())
            .child(self.times.clone())
            .when(!self.description.is_empty(), |tip| {
                tip.child(div().text_color(rgb(MUTED)).child(self.description.clone()))
            })
    }
}

impl CalendarApp {
    fn button(
        &self,
        id: &str,
        label: impl Into<SharedString>,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(SharedString::from(id.to_owned()))
            .tab_index(0)
            .px_3()
            .py(px(6.0))
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgba((SURFACE << 8) | 0x70))
            .text_color(rgb(TEXT))
            .text_size(px(12.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x32273f)))
            .focus(|style| style.border_color(rgb(ACCENT)).bg(rgb(0x30263e)))
            .child(label.into())
            .on_click(cx.listener(move |this, _, window, cx| this.execute(command, window, cx)))
    }

    pub(super) fn editor_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.editor.as_ref().expect("editor panel needs editor");
        if self.delete_pending {
            return div()
                .flex()
                .flex_col()
                .gap_4()
                .p_8()
                .max_w(px(650.0))
                .child(div().text_size(px(20.0)).child("Delete this item?"))
                .child(editor.fields[0].read(cx).value())
                .child("This permanently removes the saved item.")
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(self.button("keep", "Keep item", Command::CancelDelete, cx))
                        .child(self.button(
                            "confirm-delete",
                            "Delete permanently",
                            Command::ConfirmDelete,
                            cx,
                        )),
                )
                .into_any_element();
        }
        let labels = [
            "Title",
            "Description",
            "Start · YYYY-MM-DD HH:MM",
            "End · YYYY-MM-DD HH:MM",
            "Item time zone · IANA",
            "Link · HTTP / HTTPS",
        ];
        let mut fields = div().flex().flex_col().gap_3();
        for (field, label) in editor.fields.iter().zip(labels) {
            fields = fields.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(MUTED))
                            .child(label),
                    )
                    .child(field.clone()),
            );
        }
        div().id("editor-scroll").flex_1().overflow_y_scroll().p_6()
            .child(div().max_w(px(760.0)).mx_auto().flex().flex_col().gap_4()
                .child(div().text_size(px(20.0)).child(if editor.id.is_some() { "Edit item" } else { "New item" }))
                .child(div().flex().gap_3()
                    .child(self.button("event-kind", if editor.kind == ItemKind::Event { "● Event" } else { "Event" }, Command::Kind(ItemKind::Event), cx))
                    .child(self.button("task-kind", if editor.kind == ItemKind::Task { "● Task" } else { "Task" }, Command::Kind(ItemKind::Task), cx)))
                .child(fields)
                .child(div().flex().gap_2()
                    .child(self.button("start-fold", format!("Start: {}", occurrence_label(editor.start_occurrence)), Command::StartOccurrence, cx))
                    .child(self.button("end-fold", format!("End: {}", occurrence_label(editor.end_occurrence)), Command::EndOccurrence, cx)))
                .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child("For a repeated clock time, choose its earlier or later occurrence. Missing times cannot be saved."))
                .when(editor.kind == ItemKind::Task, |panel| panel.child(
                    self.button("complete", if editor.completed { "☑ Completed" } else { "☐ Mark complete" }, Command::Complete, cx)))
                .child(div().flex().gap_3()
                    .child(self.button("save", "Save · Ctrl/Cmd+S", Command::Save, cx))
                    .child(self.button("cancel", "Cancel · Esc", Command::Cancel, cx))
                    .when(editor.id.is_some(), |row| row.child(self.button("delete", "Delete…", Command::Delete, cx))))
                .when_some(editor.id.filter(|id| self.calendar.item(*id).is_ok_and(|item| !item.url().is_empty())),
                    |panel, id| panel.child(self.button("open-editor-link", "Open saved link ↗", Command::OpenLink(id), cx))))
            .into_any_element()
    }

    pub(super) fn settings_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self
            .settings
            .as_ref()
            .expect("settings panel needs settings");
        div().p_8().max_w(px(760.0)).flex().flex_col().gap_4()
            .child(div().text_size(px(20.0)).child("Calendar settings"))
            .child("Display time zone (IANA)")
            .child(settings.zone.clone())
            .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child("Changing this zone changes the display, not the saved instants. Each item retains its original zone."))
            .child(self.button("holidays", if settings.holidays { "☑ French national public holidays" } else { "☐ French national public holidays" }, Command::ToggleHolidays, cx))
            .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child("National France calendar only; regional and overseas holidays are excluded."))
            .child(div().flex().gap_3()
                .child(self.button("save-settings", "Apply", Command::SaveSettings, cx))
                .child(self.button("cancel-settings", "Cancel", Command::Cancel, cx)))
            .child(div().text_size(px(12.0)).text_color(rgb(MUTED)).child(format!("Data: {}", self.store.as_ref().map(|store| store.path().display().to_string()).unwrap_or_default())))
            .into_any_element()
    }

    fn item_card(
        &self,
        item: &CalendarItem,
        id: String,
        layout: CardLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let compact = !matches!(layout, CardLayout::Agenda);
        let fill_height = matches!(layout, CardLayout::Timeline(_));
        let day = match layout {
            CardLayout::Agenda => None,
            CardLayout::Month(date) | CardLayout::Timeline(date) => Some(date),
        };
        let zone = self.calendar.timezone();
        let start = item.start().with_timezone(&zone);
        let end = item.end().with_timezone(&zone);
        let prefix = match (item.kind(), item.completed()) {
            (ItemKind::Event, _) => "●",
            (ItemKind::Task, false) => "☐",
            (ItemKind::Task, true) => "☑",
        };
        let label = format!(
            "{prefix} {}{}",
            item.title(),
            if compact && !item.url().is_empty() {
                " ↗"
            } else {
                ""
            }
        );
        let time = if let Some(date) = day {
            let from = if start.date_naive() < date {
                "↤ 00:00".to_owned()
            } else {
                start.format("%H:%M").to_string()
            };
            let until = if end.date_naive() > date {
                if (item.end() - Duration::nanoseconds(1))
                    .with_timezone(&zone)
                    .date_naive()
                    > date
                {
                    "24:00 ↦".to_owned()
                } else {
                    "24:00".to_owned()
                }
            } else {
                end.format("%H:%M").to_string()
            };
            format!("{from} – {until}")
        } else {
            format!(
                "{} → {}",
                start.format("%d %b %H:%M %:z"),
                end.format("%d %b %H:%M %:z")
            )
        };
        let color = if item.kind() == ItemKind::Task {
            0x245f54
        } else {
            0x60439b
        };
        let item_id = item.id();
        let tooltip_title = format!("{prefix} {}", item.title());
        let tooltip_times = format!(
            "{} → {} ({zone})",
            start.format("%d %b %Y %H:%M %:z"),
            end.format("%d %b %Y %H:%M %:z")
        );
        let mut description = item.description().chars();
        let mut tooltip_description: String = description.by_ref().take(200).collect();
        if description.next().is_some() {
            tooltip_description.push('…');
        }
        div()
            .id(SharedString::from(id))
            .tab_index(0)
            .w_full()
            .when(fill_height, |card| card.h_full())
            .p_2()
            .when(compact, |card| card.p(px(3.0)).line_height(px(14.0)))
            .rounded_md()
            .bg(rgb(color))
            .border_1()
            .border_color(rgb(if item.completed() { BORDER } else { 0x8064b5 }))
            .text_color(rgb(if item.completed() { MUTED } else { TEXT }))
            .text_size(px(11.0))
            .overflow_hidden()
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(ACCENT)))
            .focus(|style| style.border_color(rgb(ACCENT)))
            .tooltip(move |_, cx| {
                cx.new(|_| ItemTooltip {
                    title: tooltip_title.clone(),
                    times: tooltip_times.clone(),
                    description: tooltip_description.clone(),
                })
                .into()
            })
            .child(div().when(compact, |line| line.truncate()).child(label))
            .child(
                div()
                    .text_color(rgb(0xd7c8e9))
                    .when(compact, |line| line.truncate())
                    .child(time),
            )
            .when(!compact && !item.url().is_empty(), |card| {
                card.child("↗ Link attached")
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute(Command::Edit(item_id), window, cx)
            }))
            .into_any_element()
    }

    fn date_button(
        &self,
        date: NaiveDate,
        id: String,
        diameter: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let today = Utc::now()
            .with_timezone(&self.calendar.timezone())
            .date_naive();
        div()
            .id(SharedString::from(id))
            .tab_index(0)
            .size(px(diameter))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .text_size(px(11.0))
            .text_color(rgb(if date == today {
                TEXT
            } else if date.month() != self.selected.month() {
                MUTED
            } else {
                TEXT
            }))
            .when(date == today, |day| day.bg(rgb(0x7652be)))
            .when(date == self.selected && date != today, |day| {
                day.bg(rgb(0x3d3055))
            })
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x49375f)))
            .focus(|style| style.border_1().border_color(rgb(ACCENT)))
            .child(date.day().to_string())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute(Command::Select(date), window, cx)
            }))
    }

    fn mini_month(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut heading = div().flex().items_center().gap_1().child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .child(self.selected.format("%B %Y").to_string()),
        );
        for (forward, label) in [(false, "‹"), (true, "›")] {
            if let Ok(date) = navigate(self.selected, CalendarView::Month, forward) {
                heading = heading.child(
                    self.button(
                        if forward {
                            "mini-next"
                        } else {
                            "mini-previous"
                        },
                        label,
                        Command::Select(date),
                        cx,
                    )
                    .px_2()
                    .border_0(),
                );
            }
        }
        let mut mini = div().flex().flex_col().gap_1().child(heading);
        let mut names = div().flex();
        for name in ["M", "T", "W", "T", "F", "S", "S"] {
            names = names.child(
                div()
                    .flex_1()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(name),
            );
        }
        mini = mini.child(names);
        for week in month_days(self.selected).chunks(7) {
            let mut row = div().flex();
            for &date in week {
                row = row.child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_center()
                        .child(self.date_button(date, format!("mini-{date}"), 26.0, cx)),
                );
            }
            mini = mini.child(row);
        }
        mini.into_any_element()
    }

    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let items = self.calendar.items_on(self.selected);
        let mut agenda = div().flex().flex_col().gap_3();
        for item in &items {
            agenda = agenda.child(self.item_card(
                item,
                format!("agenda-{}", item.id()),
                CardLayout::Agenda,
                cx,
            ));
            if !item.url().is_empty() {
                agenda = agenda.child(self.button(
                    &format!("link-{}", item.id()),
                    "Open link ↗",
                    Command::OpenLink(item.id()),
                    cx,
                ));
            }
        }
        div()
            .w(px(232.0))
            .flex_shrink_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                self.button(
                    "new-selected",
                    "+  Create",
                    Command::Create(self.selected, 9),
                    cx,
                )
                .py_3()
                .bg(rgb(0x3b2d53))
                .border_color(rgb(0x55416f)),
            )
            .child(self.mini_month(cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(self.jump.clone()))
                    .child(self.button("jump", "Go", Command::Jump, cx)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .child(self.selected.format("%A, %d %B").to_string()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(self.calendar.timezone().name()),
            )
            .child(div().text_size(px(12.0)).child(format!(
                    "{} item(s) · {}",
                    items.len(),
                    Utc::now()
                        .with_timezone(&self.calendar.timezone())
                        .format("%H:%M %:z")
                )))
            .child(
                div()
                    .id("agenda-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(agenda)
                    .when(items.is_empty(), |panel| {
                        panel.child(div().text_color(rgb(MUTED)).child("No events or tasks."))
                    }),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child("Tab to navigate · Enter to open\nT Today · W Week · M Month"),
            )
            .into_any_element()
    }

    fn day_heading(&self, date: NaiveDate, cx: &mut Context<Self>) -> AnyElement {
        let today = Utc::now()
            .with_timezone(&self.calendar.timezone())
            .date_naive();
        let holiday = if self.calendar.show_holidays() {
            holidays(date).join(" · ")
        } else {
            String::new()
        };
        div()
            .flex_1()
            .min_w(px(96.0))
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(if date == today { ACCENT } else { MUTED }))
                    .child(date.format("%a").to_string().to_uppercase()),
            )
            .child(
                self.date_button(date, format!("day-{date}"), 34.0, cx)
                    .text_size(px(22.0)),
            )
            .child(
                div()
                    .h(px(18.0))
                    .w_full()
                    .px_1()
                    .text_center()
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(rgb(0xe9c987))
                    .child(holiday),
            )
            .into_any_element()
    }

    pub(super) fn week_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let days = visible_days(self.selected, CalendarView::Week);
        let items_by_day = self.calendar.items_on_days(&days);
        let now = Utc::now().with_timezone(&self.calendar.timezone());
        let mut heading = div().flex().w_full().child(
            div()
                .w(px(64.0))
                .flex_shrink_0()
                .flex()
                .items_end()
                .justify_center()
                .pb_2()
                .text_size(px(10.0))
                .text_color(rgb(MUTED))
                .child(now.format("UTC%:z").to_string()),
        );
        let mut body = div().flex().w_full().h(px(1440.0));
        let mut hours = div().w(px(64.0)).flex_shrink_0().relative();
        for hour in 0..24 {
            hours = hours.child(
                div()
                    .absolute()
                    .top(px(hour as f32 * 60.0))
                    .right_2()
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .child(format!("{hour:02}:00")),
            );
        }
        body = body.child(hours);
        for (date, items) in days.into_iter().zip(items_by_day) {
            heading = heading.child(self.day_heading(date, cx));
            let mut column = div()
                .flex_1()
                .min_w(px(96.0))
                .relative()
                .h_full()
                .bg(rgba(if is_weekend(date) {
                    0x30263b35
                } else {
                    0x00000000
                }))
                .border_l_1()
                .border_color(rgb(BORDER));
            for hour in 0..24 {
                column = column.child(
                    div()
                        .id(SharedString::from(format!("slot-{date}-{hour}")))
                        .absolute()
                        .top(px(hour as f32 * 60.0))
                        .w_full()
                        .h(px(60.0))
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x261e33)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.execute(Command::Create(date, hour), window, cx)
                        })),
                );
            }
            for (item, segment) in day_layout(&items, date, self.calendar.timezone()) {
                column = column.child(
                    div()
                        .absolute()
                        .top(px(segment.start_minute))
                        .left(relative(segment.column as f32 / segment.columns as f32))
                        .w(relative(1.0 / segment.columns as f32))
                        .h(px((segment.end_minute - segment.start_minute).max(20.0)))
                        .overflow_hidden()
                        .px(px(2.0))
                        .child(self.item_card(
                            item,
                            format!("week-{date}-{}", item.id()),
                            CardLayout::Timeline(date),
                            cx,
                        )),
                );
            }
            if date == now.date_naive() {
                let minute = (now.hour() * 60 + now.minute()) as f32;
                column = column.child(
                    div()
                        .absolute()
                        .top(px(minute))
                        .w_full()
                        .h(px(2.0))
                        .bg(rgb(0xef737b))
                        .child(
                            div()
                                .absolute()
                                .left(px(-4.0))
                                .top(px(-3.0))
                                .size(px(8.0))
                                .rounded_full()
                                .bg(rgb(0xef737b)),
                        ),
                );
            }
            body = body.child(column);
        }
        div()
            .id("week-horizontal")
            .flex_1()
            .overflow_x_scroll()
            .child(
                div()
                    .min_w(px(740.0))
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(heading)
                    .child(
                        div()
                            .id("week-scroll")
                            .flex_1()
                            .overflow_y_scroll()
                            .track_scroll(&self.week_scroll)
                            .child(body),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn month_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let days = month_days(self.selected);
        let items_by_day = self.calendar.items_on_days(&days);
        let mut grid = div().flex_1().min_h_0().flex().flex_col();
        let mut names = div().flex().h(px(34.0));
        for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
            names = names.child(
                div()
                    .flex_1()
                    .px_2()
                    .py_1()
                    .text_size(px(12.0))
                    .text_color(rgb(MUTED))
                    .child(name),
            );
        }
        grid = grid.child(names);
        for (row_index, (week, week_items)) in
            days.chunks(7).zip(items_by_day.chunks(7)).enumerate()
        {
            let mut row = div().flex().flex_1().min_h_0();
            for (date, items) in week.iter().zip(week_items) {
                let date = *date;
                let mut cell = div()
                    .id(SharedString::from(format!("month-{date}")))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_1()
                    .border_1()
                    .border_color(rgb(if date == self.selected {
                        ACCENT
                    } else {
                        BORDER
                    }))
                    .bg(rgba(if is_weekend(date) {
                        0x30263b35
                    } else {
                        0x00000000
                    }))
                    .text_color(rgb(if date.month() != self.selected.month() {
                        MUTED
                    } else {
                        TEXT
                    }))
                    .child(
                        self.date_button(date, format!("month-day-{date}"), 26.0, cx)
                            .mx_auto(),
                    );
                if self.calendar.show_holidays() {
                    for holiday in holidays(date) {
                        cell = cell.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(0xe9c987))
                                .child(holiday),
                        );
                    }
                }
                let mut list = div().flex().flex_col().gap_1();
                for item in items {
                    list = list.child(self.item_card(
                        item,
                        format!("month-item-{date}-{}", item.id()),
                        CardLayout::Month(date),
                        cx,
                    ));
                }
                cell = cell.child(
                    div()
                        .id(SharedString::from(format!("month-list-{row_index}-{date}")))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(list),
                );
                row = row.child(cell);
            }
            grid = grid.child(row);
        }
        div()
            .id("month-scroll")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(grid)
            .into_any_element()
    }

    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self.selected.format("%B %Y").to_string();
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .child(
                div()
                    .text_size(px(20.0))
                    .text_color(rgb(TEXT))
                    .w(px(208.0))
                    .flex_shrink_0()
                    .child("Calendar"),
            )
            .child(
                self.button("today", "Today", Command::Today, cx)
                    .rounded_full(),
            )
            .child(
                self.button("previous", "‹", Command::Previous, cx)
                    .tooltip(|_, cx| {
                        cx.new(|_| ItemTooltip {
                            title: "Previous period".into(),
                            times: String::new(),
                            description: String::new(),
                        })
                        .into()
                    }),
            )
            .child(
                self.button("next", "›", Command::Next, cx)
                    .tooltip(|_, cx| {
                        cx.new(|_| ItemTooltip {
                            title: "Next period".into(),
                            times: String::new(),
                            description: String::new(),
                        })
                        .into()
                    }),
            )
            .child(div().flex_1().px_3().text_size(px(20.0)).child(title))
            .child(
                self.button(
                    "week",
                    if self.view == CalendarView::Week {
                        "● Week"
                    } else {
                        "Week"
                    },
                    Command::View(CalendarView::Week),
                    cx,
                )
                .when(self.view == CalendarView::Week, |button| {
                    button.bg(rgb(0x3d3055)).border_color(rgb(0x55416f))
                }),
            )
            .child(
                self.button(
                    "month",
                    if self.view == CalendarView::Month {
                        "● Month"
                    } else {
                        "Month"
                    },
                    Command::View(CalendarView::Month),
                    cx,
                )
                .when(self.view == CalendarView::Month, |button| {
                    button.bg(rgb(0x3d3055)).border_color(rgb(0x55416f))
                }),
            )
            .child(self.button("settings", "Settings", Command::Settings, cx))
            .into_any_element()
    }
}
