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
const SIDEBAR_WIDTH: f32 = 232.0;

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
    let loaded = default_data_path()
        .and_then(|path| load_desktop_calendar(&path).map(|calendar| (path, calendar)));
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
            KeyBinding::new("enter", Save, Some("CalendarEditor")),
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
    Delete,
    OpenLink(u64),
    Settings,
    ToggleHolidays,
    ToggleWeekends,
    SaveSettings,
    Jump,
}

struct Editor {
    id: Option<u64>,
    fields: [Entity<TextInput>; 5],
    original: Option<ItemDraft>,
    zone: chrono_tz::Tz,
    start_occurrence: Occurrence,
    end_occurrence: Occurrence,
}

impl Editor {
    fn new(
        id: Option<u64>,
        draft: ItemDraft,
        zone: chrono_tz::Tz,
        cx: &mut Context<CalendarApp>,
    ) -> Self {
        let original = id.map(|_| draft.clone());
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
            draft.url,
        ];
        let fields = values.map(|value| cx.new(|cx| TextInput::new(&value, "", cx)));
        Self {
            id,
            fields,
            original,
            zone,
            start_occurrence: editor_occurrence(draft.start, zone),
            end_occurrence: editor_occurrence(draft.end, zone),
        }
    }

    fn draft(&self, cx: &App) -> Result<ItemDraft> {
        let values = self.fields.each_ref().map(|field| field.read(cx).value());
        let zone = self.zone;
        let mut draft = ItemDraft {
            title: values[0].clone(),
            description: values[1].clone(),
            kind: ItemKind::Event,
            start: resolve_local(&values[2], zone, self.start_occurrence)?,
            end: resolve_local(&values[3], zone, self.end_occurrence)?,
            timezone: zone,
            url: values[4].clone(),
            completed: false,
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
    holidays: bool,
}

struct CalendarApp {
    calendar: Calendar,
    calendar_revision: u64,
    store: Option<std::path::PathBuf>,
    startup_error: Option<String>,
    view: CalendarView,
    show_weekends: bool,
    selected: NaiveDate,
    editor: Option<Editor>,
    settings: Option<Settings>,
    jump: Entity<TextInput>,
    error: Option<String>,
    focus: FocusHandle,
    week_scroll: ScrollHandle,
    _clock: Task<()>,
}

impl CalendarApp {
    fn new(
        loaded: Result<(std::path::PathBuf, Calendar)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (store, calendar, startup_error) = match loaded {
            Ok((path, calendar)) => (Some(path), calendar, None),
            Err(error) => (None, Calendar::default(), Some(error.to_string())),
        };
        let selected = Utc::now().with_timezone(&calendar.timezone()).date_naive();
        let focus = cx.focus_handle();
        window.focus(&focus);
        let jump = cx.new(|cx| TextInput::new("", "YYYY-MM-DD", cx));
        let week_scroll = ScrollHandle::new();
        // Start the viewport at 07:00; the full day remains scrollable.
        week_scroll.set_offset(point(px(0.0), px(-7.0 * 60.0)));
        let refresh_path = store.clone();
        let clock = cx.spawn(async move |entity, cx| {
            let mut minute = Utc::now().timestamp().div_euclid(60);
            loop {
                // External changes appear within a second; repaint the clock each minute.
                cx.background_executor()
                    .timer(StdDuration::from_secs(1))
                    .await;
                let Ok(revision) = entity.update(cx, |this, _| this.calendar_revision) else {
                    break;
                };
                let path = refresh_path.clone();
                let loaded = cx
                    .background_executor()
                    .spawn(async move { path.map(|path| load_desktop_calendar(&path)) })
                    .await;
                let current_minute = Utc::now().timestamp().div_euclid(60);
                let clock_changed = current_minute != minute;
                minute = current_minute;
                if entity
                    .update(cx, |this, cx| {
                        // A desktop save during the read makes this snapshot obsolete.
                        if let Some(loaded) = loaded.filter(|_| revision == this.calendar_revision)
                        {
                            match loaded {
                                Ok(calendar) if calendar != this.calendar => {
                                    this.calendar = calendar;
                                    cx.notify();
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    let message = error.to_string();
                                    if this.error.as_ref() != Some(&message) {
                                        this.error = Some(message);
                                        cx.notify();
                                    }
                                }
                            }
                        }
                        if clock_changed {
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
            calendar_revision: 0,
            store,
            startup_error,
            view: CalendarView::default(),
            show_weekends: false,
            selected,
            editor: None,
            settings: None,
            jump,
            error: None,
            focus,
            week_scroll,
            _clock: clock,
        }
    }

    /// Apply a change to the latest document; update memory only after saving.
    fn persist(&mut self, change: impl FnOnce(&mut Calendar) -> Result<()>) -> Result<()> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| CalendarError("Calendar storage is unavailable.".into()))?;
        let (calendar, ()) = CalendarStore::transact(store, change)?;
        self.calendar = use_system_timezone(calendar)?;
        self.calendar_revision = self.calendar_revision.wrapping_add(1);
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
            Command::ToggleWeekends => self.show_weekends = !self.show_weekends,
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
                    resolve_local(&format!("{date} {hour:02}:00"), zone, Occurrence::Earlier)?;
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
                self.editor = Some(Editor::new(None, draft, zone, cx));
                self.focus_editor(window, cx);
            }
            Command::Edit(id) => {
                self.editor = Some(Editor::new(
                    Some(id),
                    self.calendar.item(id)?.draft(),
                    self.calendar.timezone(),
                    cx,
                ));
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
                let id = editor.id;
                let original = editor.original.clone();
                self.persist(move |calendar| {
                    if let Some(id) = id {
                        ensure_unchanged(calendar, id, original.as_ref())?;
                        calendar.update(id, draft)?;
                    } else {
                        calendar.create(draft)?;
                    }
                    Ok(())
                })?;
                self.selected = date;
                self.reveal_hour(hour);
                self.editor = None;
                window.focus(&self.focus);
            }
            Command::Cancel => {
                self.editor = None;
                self.settings = None;
                window.focus(&self.focus);
            }
            Command::Delete => {
                if let Some(id) = self.editor.as_ref().and_then(|editor| editor.id) {
                    let original = self
                        .editor
                        .as_ref()
                        .and_then(|editor| editor.original.clone());
                    self.persist(move |calendar| {
                        ensure_unchanged(calendar, id, original.as_ref())?;
                        calendar.delete(id)
                    })?;
                    self.editor = None;
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
                window.focus(&self.focus);
                self.settings = Some(Settings {
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
                    let holidays = settings.holidays;
                    self.persist(move |calendar| {
                        calendar.set_show_holidays(holidays);
                        Ok(())
                    })?;
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
            .key_context(if self.editor.is_some() {
                "CalendarEditor"
            } else {
                "Calendar"
            })
            .on_action(cx.listener(|_, _: &NextFocus, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &PreviousFocus, window, _| window.focus_prev()))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.execute(Command::Cancel, window, cx);
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
                this.execute(Command::Save, window, cx);
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

// Preserve repeated saved instants; use the first occurrence for new clock times.
fn editor_occurrence(instant: chrono::DateTime<Utc>, zone: chrono_tz::Tz) -> Occurrence {
    match occurrence_at(instant, zone) {
        Occurrence::Later => Occurrence::Later,
        _ => Occurrence::Earlier,
    }
}

fn use_system_timezone(mut calendar: Calendar) -> Result<Calendar> {
    let name = iana_time_zone::get_timezone()
        .map_err(|error| CalendarError(format!("Cannot detect system time zone: {error}")))?;
    calendar.set_timezone(parse_zone(&name)?);
    Ok(calendar)
}

fn load_desktop_calendar(path: &std::path::Path) -> Result<Calendar> {
    use_system_timezone(CalendarStore::load(path)?)
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

fn ensure_unchanged(calendar: &Calendar, id: u64, original: Option<&ItemDraft>) -> Result<()> {
    if original != Some(&calendar.item(id)?.draft()) {
        return Err(CalendarError(
            "This item changed outside the editor. Reopen it before saving or deleting.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_occurrence_policy_preserves_both_repeated_instants() {
        let zone = chrono_tz::Europe::Paris;
        for occurrence in [Occurrence::Earlier, Occurrence::Later] {
            let instant = resolve_local("2026-10-25 02:30", zone, occurrence).unwrap();
            assert_eq!(editor_occurrence(instant, zone), occurrence);
        }
        let instant = resolve_local("2026-10-24 02:30", zone, Occurrence::Reject).unwrap();
        assert_eq!(editor_occurrence(instant, zone), Occurrence::Earlier);
    }

    #[test]
    fn desktop_uses_system_timezone_without_changing_saved_items() {
        let calendar = Calendar::default();
        let desktop = use_system_timezone(calendar.clone()).unwrap();
        let expected = parse_zone(&iana_time_zone::get_timezone().unwrap()).unwrap();
        assert_eq!(desktop.timezone(), expected);
        assert_eq!(desktop.items(), calendar.items());
    }

    #[test]
    fn editor_rejects_external_updates_and_deletions() {
        let mut calendar = Calendar::default();
        let start = resolve_local(
            "2026-10-10 09:00",
            chrono_tz::Europe::Paris,
            Occurrence::Reject,
        )
        .unwrap();
        let original = ItemDraft {
            title: "Meeting".into(),
            description: String::new(),
            kind: ItemKind::Event,
            start,
            end: start + Duration::hours(1),
            timezone: chrono_tz::Europe::Paris,
            url: String::new(),
            completed: false,
        };
        let id = calendar.create(original.clone()).unwrap();
        assert!(ensure_unchanged(&calendar, id, Some(&original)).is_ok());
        let mut changed = original.clone();
        changed.title = "External update".into();
        calendar.update(id, changed).unwrap();
        assert!(ensure_unchanged(&calendar, id, Some(&original)).is_err());
        calendar.delete(id).unwrap();
        assert!(ensure_unchanged(&calendar, id, Some(&original)).is_err());
    }

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
