//! Desktop presentation. All changes go through the validated calendar library.

mod input;
mod views;

use std::time::Duration as StdDuration;

use chrono::{Datelike, Duration, NaiveDate, Timelike, Utc};
use gpui::{
    App, Application, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, KeyBinding,
    ScrollHandle, Task, Window, WindowBounds, WindowOptions, actions, div, point, prelude::*, px,
    rgb, rgba, size,
};

use crate::calendar::*;
use input::TextInput;

const BACKGROUND: u32 = 0x101114;
const SURFACE: u32 = 0x202127;
const BORDER: u32 = 0x303138;
const ACCENT: u32 = 0xbca2ff;
const TEXT: u32 = 0xf3effa;
const MUTED: u32 = 0xb0a9bc;

actions!(
    calendar,
    [
        NextFocus,
        PreviousFocus,
        Escape,
        NewItem,
        Today,
        Week,
        Month,
        Save
    ]
);

pub fn run() {
    let loaded = default_data_path().and_then(CalendarStore::open);
    Application::new().run(move |cx: &mut App| {
        input::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("tab", NextFocus, None),
            KeyBinding::new("shift-tab", PreviousFocus, None),
            KeyBinding::new("escape", Escape, None),
            KeyBinding::new("ctrl-n", NewItem, None),
            KeyBinding::new("cmd-n", NewItem, None),
            KeyBinding::new("ctrl-s", Save, None),
            KeyBinding::new("cmd-s", Save, None),
            KeyBinding::new("t", Today, Some("Calendar && !TextInput")),
            KeyBinding::new("w", Week, Some("Calendar && !TextInput")),
            KeyBinding::new("m", Month, Some("Calendar && !TextInput")),
        ]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1380.0), px(900.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Calendar GPUI".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                window_min_size: Some(size(px(900.0), px(650.0))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| CalendarApp::new(loaded, window, cx)),
        );
        match result {
            Ok(_) => cx.activate(true),
            Err(error) => {
                eprintln!("Failed to open calendar window: {error}");
                cx.quit();
            }
        }
    });
}

#[derive(Clone, Copy)]
enum Command {
    Previous,
    Next,
    Today,
    View(CalendarView),
    Select(NaiveDate),
    Create(NaiveDate, u32),
    Edit(u64),
    Save,
    Cancel,
    Kind(ItemKind),
    Complete,
    StartOccurrence,
    EndOccurrence,
    Delete,
    ConfirmDelete,
    CancelDelete,
    OpenLink(u64),
    Settings,
    ToggleHolidays,
    SaveSettings,
    Jump,
}

struct Editor {
    id: Option<u64>,
    fields: [Entity<TextInput>; 6],
    original: Option<ItemDraft>,
    kind: ItemKind,
    completed: bool,
    start_occurrence: Occurrence,
    end_occurrence: Occurrence,
}

impl Editor {
    fn new(id: Option<u64>, draft: ItemDraft, cx: &mut Context<CalendarApp>) -> Self {
        let original = id.map(|_| draft.clone());
        let zone = draft.timezone;
        let values = [
            draft.title,
            draft.description,
            draft
                .start
                .with_timezone(&zone)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            draft
                .end
                .with_timezone(&zone)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            zone.name().to_owned(),
            draft.url,
        ];
        let fields = values.map(|value| cx.new(|cx| TextInput::new(&value, "", cx)));
        Self {
            id,
            fields,
            original,
            kind: draft.kind,
            completed: draft.completed,
            start_occurrence: occurrence_at(draft.start, zone),
            end_occurrence: occurrence_at(draft.end, zone),
        }
    }

    fn draft(&self, cx: &App) -> Result<ItemDraft> {
        let values = self.fields.each_ref().map(|field| field.read(cx).value());
        let zone = parse_zone(&values[4])?;
        let mut draft = ItemDraft {
            title: values[0].clone(),
            description: values[1].clone(),
            kind: self.kind,
            start: resolve_local(&values[2], zone, self.start_occurrence)?,
            end: resolve_local(&values[3], zone, self.end_occurrence)?,
            timezone: zone,
            url: values[5].clone(),
            completed: self.completed,
        };
        if let Some(original) = &self.original {
            draft.start = preserve_precision(original.start, draft.start);
            draft.end = preserve_precision(original.end, draft.end);
        }
        draft.validate()?;
        Ok(draft)
    }
}

struct Settings {
    zone: Entity<TextInput>,
    holidays: bool,
}

struct CalendarApp {
    calendar: Calendar,
    store: Option<CalendarStore>,
    startup_error: Option<String>,
    view: CalendarView,
    selected: NaiveDate,
    editor: Option<Editor>,
    delete_pending: bool,
    settings: Option<Settings>,
    jump: Entity<TextInput>,
    error: Option<String>,
    focus: FocusHandle,
    week_scroll: ScrollHandle,
    _clock: Task<()>,
}

impl CalendarApp {
    fn new(
        loaded: Result<(CalendarStore, Calendar)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (store, calendar, startup_error) = match loaded {
            Ok((store, calendar)) => (Some(store), calendar, None),
            Err(error) => (None, Calendar::default(), Some(error.to_string())),
        };
        let selected = Utc::now().with_timezone(&calendar.timezone()).date_naive();
        let focus = cx.focus_handle();
        window.focus(&focus);
        let jump = cx.new(|cx| TextInput::new("", "YYYY-MM-DD", cx));
        let week_scroll = ScrollHandle::new();
        let hour = Utc::now().with_timezone(&calendar.timezone()).hour();
        week_scroll.set_offset(point(px(0.0), px(-(hour.saturating_sub(2) as f32) * 60.0)));
        let clock = cx.spawn(async move |entity, cx| {
            loop {
                // The marker and agenda clock display minutes. Wake at the next
                // minute boundary rather than repainting twice per minute.
                let now = Utc::now();
                let delay = StdDuration::from_secs(60 - u64::from(now.second()))
                    - StdDuration::from_nanos(u64::from(now.nanosecond()));
                cx.background_executor().timer(delay).await;
                if entity
                    .update(cx, |this, cx| {
                        if this.editor.is_none()
                            && this.settings.is_none()
                            && this.startup_error.is_none()
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            calendar,
            store,
            startup_error,
            view: CalendarView::default(),
            selected,
            editor: None,
            delete_pending: false,
            settings: None,
            jump,
            error: None,
            focus,
            week_scroll,
            _clock: clock,
        }
    }

    /// Apply in memory only after a successful disk write.
    fn persist(&mut self, next: Calendar) -> Result<()> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| CalendarError("Calendar storage is unavailable.".into()))?;
        store.save(&next)?;
        self.calendar = next;
        Ok(())
    }

    fn execute(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if let Err(error) = self.handle(command, window, cx) {
            self.error = Some(error.to_string());
        }
        cx.notify();
    }

    fn handle(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        match command {
            Command::Previous => self.selected = navigate(self.selected, self.view, false)?,
            Command::Next => self.selected = navigate(self.selected, self.view, true)?,
            Command::Today => {
                let now = Utc::now().with_timezone(&self.calendar.timezone());
                self.selected = now.date_naive();
                self.reveal_hour(now.hour());
            }
            Command::View(view) => self.view = view,
            Command::Select(date) => {
                validate_date(date)?;
                self.selected = date;
            }
            Command::Jump => {
                let date = NaiveDate::parse_from_str(self.jump.read(cx).value().trim(), "%Y-%m-%d")
                    .map_err(|_| CalendarError("Enter a date in YYYY-MM-DD format.".into()))?;
                validate_date(date)?;
                self.selected = date;
            }
            Command::Create(date, hour) => {
                validate_date(date)?;
                let zone = self.calendar.timezone();
                let start =
                    resolve_local(&format!("{date} {hour:02}:00"), zone, Occurrence::Reject)?;
                let draft = ItemDraft {
                    title: String::new(),
                    description: String::new(),
                    kind: ItemKind::Event,
                    start,
                    end: start + Duration::hours(1),
                    timezone: zone,
                    url: String::new(),
                    completed: false,
                };
                self.editor = Some(Editor::new(None, draft, cx));
                self.focus_editor(window, cx);
            }
            Command::Edit(id) => {
                self.editor = Some(Editor::new(Some(id), self.calendar.item(id)?.draft(), cx));
                self.focus_editor(window, cx);
            }
            Command::Save => {
                let Some(editor) = self.editor.as_ref() else {
                    return Ok(());
                };
                let draft = editor.draft(cx)?;
                let local_start = draft.start.with_timezone(&self.calendar.timezone());
                let date = local_start.date_naive();
                let hour = local_start.hour();
                let mut next = self.calendar.clone();
                if let Some(id) = editor.id {
                    next.update(id, draft)?;
                } else {
                    next.create(draft)?;
                }
                self.persist(next)?;
                self.selected = date;
                self.reveal_hour(hour);
                self.editor = None;
                window.focus(&self.focus);
            }
            Command::Cancel => {
                self.editor = None;
                self.settings = None;
                self.delete_pending = false;
                window.focus(&self.focus);
            }
            Command::Kind(kind) => {
                if let Some(editor) = &mut self.editor {
                    editor.kind = kind;
                    if kind == ItemKind::Event {
                        editor.completed = false;
                    }
                }
            }
            Command::Complete => {
                if let Some(editor) = &mut self.editor {
                    editor.completed = !editor.completed;
                }
            }
            Command::StartOccurrence => {
                if let Some(editor) = &mut self.editor {
                    editor.start_occurrence = next_occurrence(editor.start_occurrence);
                }
            }
            Command::EndOccurrence => {
                if let Some(editor) = &mut self.editor {
                    editor.end_occurrence = next_occurrence(editor.end_occurrence);
                }
            }
            Command::Delete => {
                self.delete_pending = true;
                window.focus(&self.focus);
            }
            Command::CancelDelete => {
                self.delete_pending = false;
                self.focus_editor(window, cx);
            }
            Command::ConfirmDelete => {
                if let Some(id) = self.editor.as_ref().and_then(|editor| editor.id) {
                    let mut next = self.calendar.clone();
                    next.delete(id)?;
                    self.persist(next)?;
                    self.editor = None;
                    self.delete_pending = false;
                    window.focus(&self.focus);
                }
            }
            Command::OpenLink(id) => {
                let item = self.calendar.item(id)?;
                if !item.url().is_empty() {
                    cx.open_url(item.url());
                }
            }
            Command::Settings => {
                let value = self.calendar.timezone().name();
                let zone = cx.new(|cx| TextInput::new(value, "Europe/Paris", cx));
                window.focus(&zone.read(cx).focus_handle(cx));
                self.settings = Some(Settings {
                    zone,
                    holidays: self.calendar.show_holidays(),
                });
            }
            Command::ToggleHolidays => {
                if let Some(settings) = &mut self.settings {
                    settings.holidays = !settings.holidays;
                }
            }
            Command::SaveSettings => {
                if let Some(settings) = &self.settings {
                    let mut next = self.calendar.clone();
                    next.set_timezone(parse_zone(&settings.zone.read(cx).value())?);
                    next.set_show_holidays(settings.holidays);
                    self.persist(next)?;
                    self.settings = None;
                    window.focus(&self.focus);
                }
            }
        }
        Ok(())
    }

    fn reveal_hour(&self, hour: u32) {
        self.week_scroll
            .set_offset(point(px(0.0), px(-(hour.saturating_sub(2) as f32) * 60.0)));
    }

    fn focus_editor(&self, window: &mut Window, cx: &App) {
        if let Some(editor) = &self.editor {
            window.focus(&editor.fields[0].read(cx).focus_handle(cx));
        }
    }
}

impl Render for CalendarApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba((BACKGROUND << 8) | 0xea))
            .text_color(rgb(TEXT))
            .text_size(px(12.0))
            .track_focus(&self.focus)
            .key_context("Calendar")
            .on_action(cx.listener(|_, _: &NextFocus, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &PreviousFocus, window, _| window.focus_prev()))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if this.delete_pending {
                    this.execute(Command::CancelDelete, window, cx);
                } else {
                    this.execute(Command::Cancel, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NewItem, window, cx| {
                if this.editor.is_none() && this.settings.is_none() && this.startup_error.is_none()
                {
                    this.execute(Command::Create(this.selected, 9), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Today, window, cx| {
                if this.editor.is_none() && this.settings.is_none() {
                    this.execute(Command::Today, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Week, window, cx| {
                if this.editor.is_none() && this.settings.is_none() {
                    this.execute(Command::View(CalendarView::Week), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Month, window, cx| {
                if this.editor.is_none() && this.settings.is_none() {
                    this.execute(Command::View(CalendarView::Month), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Save, window, cx| {
                if !this.delete_pending {
                    this.execute(Command::Save, window, cx);
                }
            }));
        if let Some(error) = &self.startup_error {
            return root.child(div().p_8().flex().flex_col().gap_4()
                .child(div().text_size(px(20.0)).child("Cannot open calendar"))
                .child(error.clone())
                .child("Resolve the file or storage error, then restart. Your existing file has not been overwritten."));
        }
        if let Some(error) = &self.error {
            root = root.child(
                div()
                    .p_3()
                    .bg(rgb(0x4a202e))
                    .text_color(rgb(0xffd4df))
                    .child(error.clone()),
            );
        }
        if self.editor.is_some() {
            root = root.child(self.editor_panel(cx));
        } else if self.settings.is_some() {
            root = root.child(self.settings_panel(cx));
        } else {
            root = root.child(self.toolbar(cx)).child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .mr_3()
                            .mb_3()
                            .rounded_xl()
                            .overflow_hidden()
                            .bg(rgba(0x18191c50))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .child(match self.view {
                                CalendarView::Week => self.week_grid(cx),
                                CalendarView::Month => self.month_grid(cx),
                            }),
                    ),
            );
        }
        root
    }
}

fn next_occurrence(occurrence: Occurrence) -> Occurrence {
    match occurrence {
        Occurrence::Reject => Occurrence::Earlier,
        Occurrence::Earlier => Occurrence::Later,
        Occurrence::Later => Occurrence::Reject,
    }
}

fn occurrence_label(occurrence: Occurrence) -> &'static str {
    match occurrence {
        Occurrence::Reject => "unambiguous time",
        Occurrence::Earlier => "earlier occurrence",
        Occurrence::Later => "later occurrence",
    }
}

// Changing a title or task status must not round a saved instant to the minute.
fn preserve_precision(
    original: chrono::DateTime<Utc>,
    edited: chrono::DateTime<Utc>,
) -> chrono::DateTime<Utc> {
    let minute = original
        .with_second(0)
        .and_then(|value| value.with_nanosecond(0));
    if minute == Some(edited) {
        original
    } else {
        edited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_keeps_seconds_only_when_the_instant_minute_is_unchanged() {
        let minute = resolve_local(
            "2026-10-25 02:30",
            chrono_tz::Europe::Paris,
            Occurrence::Earlier,
        )
        .unwrap();
        let original = minute + Duration::seconds(12) + Duration::milliseconds(500);
        assert_eq!(preserve_precision(original, minute), original);
        let changed = minute + Duration::minutes(1);
        assert_eq!(preserve_precision(original, changed), changed);
        let later = minute + Duration::hours(1);
        assert_eq!(preserve_precision(original, later), later);
    }
}
