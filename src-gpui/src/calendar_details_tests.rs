use std::{
    collections::VecDeque,
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use chrono::NaiveDate;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, test::TestWindowExt};
use workreport_core::holidays::{Day, HolidayState, Year};

use crate::{
    Prototype,
    calendar_holidays::{HolidaySource, HolidayUpdate},
    workspace_edit_tests::{
        advance_time, click, drain_ready_tasks, open_window, open_workspace, replace_editor_text,
    },
};

struct ScriptedHolidays {
    cached: HolidayState,
    updates: Mutex<VecDeque<HolidayUpdate>>,
    loads: AtomicUsize,
    syncs: AtomicUsize,
    requested_years: Mutex<Vec<i32>>,
}

impl ScriptedHolidays {
    fn new(cached: HolidayState, updates: Vec<HolidayUpdate>) -> Self {
        Self {
            cached,
            updates: Mutex::new(updates.into()),
            loads: AtomicUsize::new(0),
            syncs: AtomicUsize::new(0),
            requested_years: Mutex::new(Vec::new()),
        }
    }
}

impl HolidaySource for ScriptedHolidays {
    fn load(&self) -> Result<HolidayState, String> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.cached.clone())
    }

    fn sync(&self, year: i32) -> HolidayUpdate {
        self.syncs.fetch_add(1, Ordering::SeqCst);
        self.requested_years.lock().unwrap().push(year);
        self.updates
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected duplicate holiday synchronization"))
    }
}

fn state(days: &[(&str, &str, bool)]) -> HolidayState {
    HolidayState {
        years: vec![Year {
            year: 2026,
            days: days
                .iter()
                .map(|(date, name, is_off_day)| Day {
                    date: (*date).into(),
                    name: (*name).into(),
                    is_off_day: *is_off_day,
                })
                .collect(),
            synced_at: "2026-02-17T12:00:00+08:00".into(),
            source_url: "https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/2026.json"
                .into(),
        }],
        failures: Vec::new(),
    }
}

fn fixture(content: &str) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    fs::create_dir(folder.path().join("2026")).unwrap();
    fs::write(folder.path().join("2026/20260217.md"), content).unwrap();
    folder
}

fn label(cx: &mut TestAppContext, window: AnyWindowHandle, date: &str) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(format!("calendar-day-{date}"))
            .label()
            .unwrap()
            .to_owned()
    })
    .unwrap()
}

fn install_source(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    app: &Entity<Prototype>,
    source: Arc<ScriptedHolidays>,
    load: bool,
) {
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.calendar_details.holiday_source = Some(source);
            if load {
                app.load_calendar_holidays(window, cx);
            } else {
                cx.notify();
            }
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
}

fn preview(cx: &mut TestAppContext, app: &Entity<Prototype>, date: &str) -> Option<String> {
    cx.update(|cx| {
        app.read(cx)
            .calendar_details
            .previews
            .values
            .get(date)
            .map(|value| value.clone().unwrap())
    })
}

fn selected_journal(cx: &mut TestAppContext, app: &Entity<Prototype>) -> String {
    cx.update(|cx| {
        app.read(cx)
            .workspace
            .as_ref()
            .unwrap()
            .journal
            .as_ref()
            .unwrap()
            .edit_target()
            .0
    })
}

#[gpui_kit::test]
fn lunar_and_holiday_labels_update_without_changing_the_open_journal_or_range(
    cx: &mut TestAppContext,
) {
    let original = "# 春节日报\r\n\r\n保留中文和 emoji 📝\r\n";
    let folder = fixture(original);
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    assert!(label(cx, window, "20260217").contains("正月"));
    assert!(label(cx, window, "20260214").contains("补班"));

    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.select_calendar_date(
                NaiveDate::from_ymd_opt(2026, 2, 19).unwrap(),
                true,
                window,
                cx,
            );
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    let before = cx.update(|cx| {
        let app = app.read(cx);
        (
            app.calendar.selected_range(),
            app.workspace_range.as_ref().unwrap().source.clone(),
        )
    });
    let synced = state(&[
        ("2026-02-14", "调整后的假期", true),
        ("2026-02-15", "春节", false),
    ]);
    let source = Arc::new(ScriptedHolidays::new(synced.clone(), Vec::new()));
    install_source(cx, window, &app, source.clone(), true);

    let changed = label(cx, window, "20260214");
    assert!(changed.contains("调整后的假期"));
    assert!(!changed.contains("补班"));
    assert!(label(cx, window, "20260215").contains("补班"));
    let lunar_only = label(cx, window, "20260217");
    assert!(lunar_only.contains("正月"));
    assert!(!lunar_only.contains("春节"));
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.calendar_details.holidays.state(), &synced);
        assert_eq!(app.calendar.selected_range(), before.0);
        assert_eq!(app.workspace_range.as_ref().unwrap().source, before.1);
        assert!(!app.workspace_busy);
        assert!(app.workspace_edit.is_none());
    });
    assert_eq!(source.loads.load(Ordering::SeqCst), 1);
    assert_eq!(source.syncs.load(Ordering::SeqCst), 0);
    assert_eq!(selected_journal(cx, &app), "20260217");
    assert_eq!(
        fs::read(folder.path().join("2026/20260217.md")).unwrap(),
        original.as_bytes()
    );
}

#[gpui_kit::test]
fn manual_sync_deduplicates_clicks_keeps_cached_data_on_failure_and_allows_retry(
    cx: &mut TestAppContext,
) {
    let folder = fixture("# 日报\n\n原始工作内容\n");
    let cached = state(&[("2026-02-17", "缓存假期", true)]);
    let updated = state(&[("2026-02-17", "重试后的假期", true)]);
    let source = Arc::new(ScriptedHolidays::new(
        cached.clone(),
        vec![
            HolidayUpdate {
                state: Some(cached.clone()),
                error: Some("网络暂不可用，请重试".into()),
            },
            HolidayUpdate {
                state: Some(updated.clone()),
                error: None,
            },
        ],
    ));
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    install_source(cx, window, &app, source.clone(), true);
    let selected = cx.update(|cx| app.read(cx).calendar.selected_range());

    // No executor tick occurs between the two native clicks. The second
    // activation must observe the in-progress state and remain disabled.
    cx.update_window(window, |_, window, cx| {
        window.click("calendar-sync-holidays", cx);
        assert_eq!(window.find("calendar-sync-holidays").disabled(), Some(true));
        window.click("calendar-sync-holidays", cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(source.syncs.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.calendar_details.holidays.state(), &cached);
        assert!(
            app.calendar_details
                .holiday_error
                .as_ref()
                .unwrap()
                .contains("网络暂不可用")
        );
        assert!(!app.calendar_details.holiday_syncing);
        assert_eq!(app.calendar.selected_range(), selected);
        assert!(!app.workspace_busy);
    });

    click(cx, window, "calendar-sync-holidays");
    assert_eq!(source.syncs.load(Ordering::SeqCst), 2);
    assert_eq!(*source.requested_years.lock().unwrap(), [2026, 2026]);
    assert!(label(cx, window, "20260217").contains("重试后的假期"));
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.calendar_details.holidays.state(), &updated);
        assert!(app.calendar_details.holiday_error.is_none());
        assert_eq!(app.calendar.selected_range(), selected);
    });
    assert_eq!(selected_journal(cx, &app), "20260217");
}

#[gpui_kit::test]
fn a_pending_cache_load_cannot_replace_a_newer_manual_sync(cx: &mut TestAppContext) {
    let folder = fixture("日报正文\n");
    let cached = state(&[("2026-02-17", "旧缓存", true)]);
    let updated = state(&[("2026-02-17", "新同步结果", true)]);
    let source = Arc::new(ScriptedHolidays::new(
        cached,
        vec![HolidayUpdate {
            state: Some(updated.clone()),
            error: None,
        }],
    ));
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    install_source(cx, window, &app, source.clone(), false);

    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.load_calendar_holidays(window, cx);
            app.sync_calendar_holidays(window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(source.loads.load(Ordering::SeqCst), 1);
    assert_eq!(source.syncs.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.calendar_details.holidays.state(), &updated);
        assert!(!app.calendar_details.holiday_loading);
        assert!(!app.calendar_details.holiday_syncing);
        assert!(app.calendar_details.holiday_error.is_none());
    });
    assert!(label(cx, window, "20260217").contains("新同步结果"));
    assert_eq!(selected_journal(cx, &app), "20260217");
}

#[gpui_kit::test]
fn calendar_summaries_follow_the_workspace_visible_period_and_latest_saved_body(
    cx: &mut TestAppContext,
) {
    let first_body = "# 日报\r\n\r\n首份工作空间摘要 📝\r\n";
    let first = fixture(first_body);
    let second = fixture("# 日报\n\n另一工作空间摘要\n");
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, first.path());
    assert_eq!(
        preview(cx, &app, "20260217").as_deref(),
        Some("首份工作空间摘要 📝")
    );

    // Start another request against the old reader before changing workspaces.
    // Its completion must never populate the new workspace's summary cache.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.calendar_details.invalidate_previews();
            app.ensure_calendar_previews(window, cx);
            app.open_workspace(second.path().to_owned(), window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert_eq!(
        preview(cx, &app, "20260217").as_deref(),
        Some("另一工作空间摘要")
    );
    assert!(label(cx, window, "20260217").contains("另一工作空间摘要"));

    click(cx, window, "calendar-view-year");
    assert!(cx.update(|cx| app.read(cx).calendar_details.previews.values.is_empty()));
    click(cx, window, "calendar-view-month");
    assert_eq!(
        preview(cx, &app, "20260217").as_deref(),
        Some("另一工作空间摘要")
    );

    click(cx, window, "edit-journal");
    let saved = "# 日报\n\n保存后的摘要 📝\n";
    replace_editor_text(cx, window, &app, saved);
    advance_time(cx, 650);
    click(cx, window, "close-journal-editor");
    assert_eq!(
        preview(cx, &app, "20260217").as_deref(),
        Some("保存后的摘要 📝")
    );
    assert!(label(cx, window, "20260217").contains("保存后的摘要 📝"));
    assert_eq!(
        fs::read_to_string(second.path().join("2026/20260217.md")).unwrap(),
        saved
    );

    click(cx, window, "calendar-next-period");
    assert!(preview(cx, &app, "20260217").is_none());
    click(cx, window, "calendar-prev-period");
    assert_eq!(
        preview(cx, &app, "20260217").as_deref(),
        Some("保存后的摘要 📝")
    );
    assert_eq!(
        fs::read(first.path().join("2026/20260217.md")).unwrap(),
        first_body.as_bytes()
    );
}
