use std::{fs, time::Duration};

use chrono::{Days, Local, NaiveDate};
use gpui_kit::{AppContext, Modifiers, TestAppContext, test::TestWindowExt};
use workreport_core::workspace::Format;

use crate::workspace_edit_tests::{
    advance_time, click, drain_ready_tasks, editor_text, open_window, open_workspace,
    replace_editor_text,
};

#[gpui_kit::test]
fn selecting_an_existing_day_preserves_its_body_and_adjacent_month_navigation(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    fs::create_dir(folder.join("2027")).unwrap();
    let selected = folder.join("2026/20261230.md");
    let latest = folder.join("2027/20270103.txt");
    let original = "# 跨年日报\r\n保留 **原文** 📝\r\n";
    fs::write(&selected, original).unwrap();
    fs::write(&latest, "最新纯文本日报\n").unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "calendar-day-20261230");
    // The preceding-month cell changes the selected journal while keeping the
    // current January view, as the existing desktop calendar does.
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20270103").is_some());
    })
    .unwrap();

    click(cx, window, "edit-journal");
    assert_eq!(editor_text(cx, &prototype), original);
    click(cx, window, "close-journal-editor");

    click(cx, window, "calendar-prev-period");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20261215").is_some());
        assert!(window.try_find("calendar-day-20270115").is_none());
    })
    .unwrap();
    click(cx, window, "calendar-next-period");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20270115").is_some());
    })
    .unwrap();

    assert_eq!(fs::read(&selected).unwrap(), original.as_bytes());
    assert_eq!(fs::read_to_string(&latest).unwrap(), "最新纯文本日报\n");
    assert_eq!(fs::read_dir(folder).unwrap().count(), 2);

    click(cx, window, "workspace-date-20261230");
    assert_eq!(
        cx.update(|cx| prototype.read(cx).calendar.month_label()),
        "2026 年 12 月"
    );
    click(cx, window, "calendar-day-20261231");
    click(cx, window, "close-journal-editor");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.workspace.as_ref().unwrap().journal.is_none());
        assert_eq!(
            app.calendar.selected_date(),
            NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
        );
    });
    assert!(!folder.join("2026/20261231.txt").exists());
    assert!(!folder.join("2026/20261231.md").exists());
}

#[gpui_kit::test]
fn selecting_an_empty_calendar_day_creates_only_after_the_first_content_is_saved(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::write(
        folder.join("workreport.json"),
        r#"{"database":{"defaultFormat":"md"},"files":{"journal":{"directory":"日报","filenameTemplate":"工作_{date}"}}}"#,
    )
    .unwrap();
    let today = Local::now().date_naive();
    let date = today.format("%Y%m%d").to_string();
    let path = folder.join(format!("日报/{}/工作_{date}.md", today.format("%Y")));
    let content = "# 日历创建的日报 📝\n\n**工作进度**\n";
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    click(cx, window, format!("calendar-day-{date}"));
    assert_eq!(editor_text(cx, &prototype), "");
    assert!(!path.exists());
    assert_eq!(fs::read_dir(folder).unwrap().count(), 1);

    replace_editor_text(cx, window, &prototype, content);
    advance_time(cx, 649);
    assert!(!path.exists());
    advance_time(cx, 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), content);
    click(cx, window, "close-journal-editor");
    click(cx, window, format!("calendar-day-{date}"));
    click(cx, window, "edit-journal");
    assert_eq!(editor_text(cx, &prototype), content);
}

#[gpui_kit::test]
fn shift_click_selects_an_inclusive_cross_year_range_without_creating_missing_days(
    cx: &mut TestAppContext,
) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    fs::create_dir(folder.join("2027")).unwrap();
    let markdown = "# 年末工作\n\n**完成**\n";
    let plain = "## 这是 TXT 原文\n*保留星号* 📝\n";
    fs::write(folder.join("2026/20261230.md"), markdown).unwrap();
    fs::write(folder.join("2027/20270101.txt"), plain).unwrap();
    fs::write(folder.join("2027/20270103.md"), "# 范围外\n").unwrap();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);

    click(cx, window, "calendar-day-20261230");
    cx.update_window(window, |_, window, cx| {
        window.click_with_modifiers(
            "calendar-day-20270102",
            Modifiers {
                shift: true,
                ..Default::default()
            },
            cx,
        );
    })
    .unwrap();
    drain_ready_tasks(cx);

    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.workspace_edit.is_none());
        let range = app.workspace_range.as_ref().unwrap();
        assert_eq!(
            range
                .documents
                .iter()
                .map(|document| document.date.as_str())
                .collect::<Vec<_>>(),
            ["20270101", "20261230"]
        );
        assert_eq!(range.documents[0].format, Format::Txt);
        assert_eq!(range.documents[0].content, plain);
        assert_eq!(range.documents[1].format, Format::Md);
        assert_eq!(range.documents[1].content, markdown);
        assert_eq!(
            app.calendar.selected_range(),
            (
                chrono::NaiveDate::from_ymd_opt(2026, 12, 30).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2027, 1, 2).unwrap()
            )
        );
    });
    assert!(!folder.join("2027/20270102.md").exists());
    assert!(!folder.join("2027/20270102.txt").exists());
    assert_eq!(
        fs::read_to_string(folder.join("2027/20270101.txt")).unwrap(),
        plain
    );
    assert_eq!(fs::read_dir(folder).unwrap().count(), 2);
    // Advancing timers cannot turn the read-only range into saved empty days.
    cx.executor().advance_clock(Duration::from_secs(1));
    drain_ready_tasks(cx);
    assert_eq!(fs::read_dir(folder.join("2027")).unwrap().count(), 2);

    click(cx, window, "range-source");
    assert!(cx.update(|cx| prototype.read(cx).range_source));
    click(cx, window, "copy-range");
    assert_eq!(
        cx.update(|cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        format!("## 2027-01-01\n\n{plain}\n\n---\n\n## 2026-12-30\n\n{markdown}")
    );
    click(cx, window, "range-preview");
    assert!(!cx.update(|cx| prototype.read(cx).range_source));
    click(cx, window, "close-range");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.workspace_range.is_none());
        assert!(!app.calendar.is_range());
        assert_eq!(
            app.workspace
                .as_ref()
                .unwrap()
                .journal
                .as_ref()
                .unwrap()
                .edit_target()
                .0,
            "20261230"
        );
    });
}

#[gpui_kit::test]
fn escape_clears_an_empty_range_and_does_not_leave_the_sample_editor(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, temporary.path());
    let today = Local::now().date_naive();
    let tomorrow = today.checked_add_days(Days::new(1)).unwrap();
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.select_calendar_date(tomorrow, true, window, cx);
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert!(cx.update(|cx| prototype.read(cx).workspace_range.is_some()));
    click(cx, window, "scratch-view");
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(!app.viewing_workspace);
        assert!(app.workspace_range.is_some());
        assert!(app.workspace_edit.is_none());
    });
    click(cx, window, "workspace-view");
    cx.update_window(window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.workspace_range.is_none());
        assert!(app.workspace_edit.is_none());
        assert_eq!(app.calendar.selected_range(), (today, today));
    });
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 0);
}

#[gpui_kit::test]
fn failed_calendar_reads_preserve_the_previous_selection_and_range(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let today = Local::now().date_naive();
    let previous = today.checked_sub_days(Days::new(1)).unwrap();
    let first = today.checked_sub_days(Days::new(2)).unwrap();
    for date in [first, previous, today] {
        let path = temporary.path().join(format!(
            "{}/{}.txt",
            date.format("%Y"),
            date.format("%Y%m%d")
        ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, date.to_string()).unwrap();
    }
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, temporary.path());
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.select_calendar_date(first, false, window, cx)
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.select_calendar_date(previous, true, window, cx)
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    let source = cx.update(|cx| {
        prototype
            .read(cx)
            .workspace_range
            .as_ref()
            .unwrap()
            .source
            .clone()
    });
    fs::remove_file(temporary.path().join(format!(
        "{}/{}.txt",
        today.format("%Y"),
        today.format("%Y%m%d")
    )))
    .unwrap();

    click(cx, window, "calendar-today");
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.status.contains("无法读取日报"));
        assert_eq!(app.calendar.selected_range(), (first, previous));
        assert_eq!(app.workspace_range.as_ref().unwrap().source, source);
    });
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.select_calendar_date(today, true, window, cx)
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.status.contains("无法读取日期范围"));
        assert_eq!(app.calendar.selected_range(), (first, previous));
        assert_eq!(app.workspace_range.as_ref().unwrap().source, source);
    });
    // Clearing remains possible even if the anchor has become unreadable.
    fs::remove_file(temporary.path().join(format!(
        "{}/{}.txt",
        first.format("%Y"),
        first.format("%Y%m%d")
    )))
    .unwrap();
    click(cx, window, "close-range");
    assert!(cx.update(|cx| prototype.read(cx).workspace_range.is_none()));
}

#[gpui_kit::test]
fn opening_an_empty_workspace_resets_the_old_calendar_and_range(cx: &mut TestAppContext) {
    let first = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    fs::create_dir(first.path().join("2020")).unwrap();
    fs::write(first.path().join("2020/20200101.md"), "# 旧日报").unwrap();
    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, first.path());
    cx.update_window(window, |_, window, cx| {
        prototype.update(cx, |app, cx| {
            app.select_calendar_date(
                NaiveDate::from_ymd_opt(2020, 1, 2).unwrap(),
                true,
                window,
                cx,
            )
        });
    })
    .unwrap();
    drain_ready_tasks(cx);
    assert!(cx.update(|cx| prototype.read(cx).workspace_range.is_some()));
    open_workspace(cx, window, &prototype, empty.path());
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(app.workspace_range.is_none());
        assert!(app.workspace.as_ref().unwrap().journal.is_none());
        assert_eq!(app.calendar.selected_date(), Local::now().date_naive());
    });
    assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0);
}
