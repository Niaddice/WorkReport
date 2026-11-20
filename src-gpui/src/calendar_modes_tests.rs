use std::{collections::BTreeMap, fs, path::Path};

use chrono::NaiveDate;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, MouseButton, ScrollDelta, TestAppContext, point, px,
    test::{TestEventExt, TestWindowExt},
};
use workreport_core::workspace::Format;

use crate::{
    Prototype,
    calendar::CalendarView,
    workspace_edit_tests::{click, drain_ready_tasks, open_window, open_workspace},
};

const PLAIN: &str = "## 这是纯文本 📝\r\n保留 **字面标记**\r\n";
const MARKDOWN: &str = "---\r\ntitle: 新日报\r\n---\r\n\r\n# 完成工作\r\n<div>保留 HTML</div>\r\n";

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y%m%d").unwrap()
}

fn write_journal(root: &Path, key: &str, extension: &str, content: &str) {
    let path = root.join(format!("{}/{key}.{extension}", &key[..4]));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    write_journal(folder.path(), "20260121", "txt", PLAIN);
    write_journal(folder.path(), "20260123", "md", MARKDOWN);
    folder
}

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn drag_days(cx: &mut TestAppContext, window: AnyWindowHandle, from: &str, to: &str) {
    cx.update_window(window, |_, window, cx| {
        window.drag_to(
            format!("calendar-day-{from}"),
            format!("calendar-day-{to}"),
            cx,
        );
    })
    .unwrap();
    drain_ready_tasks(cx);
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
fn week_navigation_moves_seven_days_across_the_year_without_changing_the_open_journal(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    write_journal(folder.path(), "20270103", "md", MARKDOWN);
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    click(cx, window, "calendar-view-week");

    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20261228").is_some());
        assert!(window.try_find("calendar-day-20270103").is_some());
        assert!(window.try_find("calendar-day-20261227").is_none());
        assert!(window.try_find("calendar-day-20270104").is_none());
    })
    .unwrap();
    click(cx, window, "calendar-next-period");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20270104").is_some());
        assert!(window.try_find("calendar-day-20270110").is_some());
        assert!(window.try_find("calendar-day-20270103").is_none());
    })
    .unwrap();
    click(cx, window, "calendar-prev-period");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20261228").is_some());
        assert!(window.try_find("calendar-day-20270103").is_some());
    })
    .unwrap();
    assert_eq!(selected_journal(cx, &app), "20270103");
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn year_navigation_and_month_heading_reveal_the_requested_month_without_writing(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    click(cx, window, "calendar-view-year");
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find_all("calendar-day-20260121").len(), 1);
        assert!(window.try_find("calendar-month-202601").is_some());
        assert!(window.try_find("calendar-month-202612").is_some());
    })
    .unwrap();
    click(cx, window, "calendar-next-period");
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-month-202701").is_some());
        assert!(window.try_find("calendar-month-202601").is_none());
    })
    .unwrap();
    click(cx, window, "calendar-month-202702");
    cx.update(|cx| {
        let calendar = &app.read(cx).calendar;
        assert_eq!(calendar.view(), CalendarView::Month);
        assert_eq!(calendar.month_label(), "2027 年 2 月");
        assert_eq!(calendar.selected_date(), date("20260123"));
    });
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20270201").is_some());
        assert!(window.try_find("calendar-day-20270228").is_some());
        assert!(window.try_find("calendar-month-202701").is_none());
    })
    .unwrap();
    assert_eq!(selected_journal(cx, &app), "20260123");
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn selecting_another_month_in_year_view_repositions_the_following_week_view(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    write_journal(folder.path(), "20260221", "md", "# 二月最新日报\n");
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    assert_eq!(selected_journal(cx, &app), "20260221");
    click(cx, window, "calendar-view-year");
    click(cx, window, "calendar-day-20260121");
    assert_eq!(selected_journal(cx, &app), "20260121");
    click(cx, window, "calendar-view-week");

    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("calendar-day-20260119").is_some());
        assert!(window.try_find("calendar-day-20260121").is_some());
        assert!(window.try_find("calendar-day-20260125").is_some());
        assert!(window.try_find("calendar-day-20260221").is_none());
    })
    .unwrap();
    cx.update(|cx| {
        let calendar = &app.read(cx).calendar;
        assert_eq!(calendar.view(), CalendarView::Week);
        assert_eq!(calendar.selected_date(), date("20260121"));
    });
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn dragging_in_each_view_reads_one_inclusive_range_and_replaces_it_without_creating_empty_days(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    for mode in ["week", "month", "year"] {
        click(cx, window, format!("calendar-view-{mode}"));
        drag_days(cx, window, "20260121", "20260123");
        cx.update(|cx| {
            let state = app.read(cx);
            assert!(state.calendar_drag.is_none());
            assert!(
                state.workspace_edit.is_none(),
                "drag must not open an editor in {mode}"
            );
            let range = state.workspace_range.as_ref().unwrap();
            assert_eq!(
                (range.start, range.end),
                (date("20260121"), date("20260123"))
            );
            assert_eq!(range.documents.len(), 2);
            assert_eq!(range.documents[0].date, "20260123");
            assert_eq!(range.documents[0].format, Format::Md);
            assert_eq!(range.documents[0].content, MARKDOWN);
            assert_eq!(range.documents[1].date, "20260121");
            assert_eq!(range.documents[1].format, Format::Txt);
            assert_eq!(range.documents[1].content, PLAIN);
        });
        // Reversed pointer movement starts a fresh range and includes empty dates.
        drag_days(cx, window, "20260124", "20260122");
        cx.update(|cx| {
            let state = app.read(cx);
            assert!(state.workspace_edit.is_none());
            let range = state.workspace_range.as_ref().unwrap();
            assert_eq!(
                (range.start, range.end),
                (date("20260122"), date("20260124"))
            );
            assert_eq!(range.documents.len(), 1);
            assert_eq!(range.documents[0].date, "20260123");
        });
        assert_eq!(files(folder.path()), before);
        // A subsequent real click must still open the selected journal.
        click(cx, window, "calendar-day-20260121");
        cx.update(|cx| {
            assert!(app.read(cx).workspace_range.is_none());
            assert!(app.read(cx).workspace_edit.is_none());
        });
        assert_eq!(selected_journal(cx, &app), "20260121");
    }
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn scrolling_the_year_grid_reaches_december_without_changing_the_open_journal(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    click(cx, window, "calendar-view-year");
    cx.update_window(window, |_, window, cx| {
        window.scroll(
            "calendar-day-20260121",
            ScrollDelta::Pixels(point(px(0.), px(-3000.))),
            cx,
        );
        assert!(window.find("calendar-month-202612").visible());
    })
    .unwrap();
    click(cx, window, "calendar-month-202612");
    cx.update(|cx| {
        let calendar = &app.read(cx).calendar;
        assert_eq!(calendar.view(), CalendarView::Month);
        assert_eq!(calendar.month_label(), "2026 年 12 月");
        assert_eq!(calendar.selected_date(), date("20260123"));
    });
    assert_eq!(selected_journal(cx, &app), "20260123");
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn returning_to_a_drag_start_keeps_the_previous_single_day_or_range_without_opening_an_editor(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    for had_range in [false, true] {
        if had_range {
            click(cx, window, "calendar-day-20260123");
            drag_days(cx, window, "20260121", "20260123");
        }
        let (selection, source) = cx.update(|cx| {
            let state = app.read(cx);
            (
                state.calendar.selected_range(),
                state
                    .workspace_range
                    .as_ref()
                    .map(|range| range.source.clone()),
            )
        });
        cx.update_window(window, |_, window, cx| {
            let from = window.find("calendar-day-20260122").bounds().center();
            let to = window.find("calendar-day-20260123").bounds().center();
            window.pointer_move(from, None, cx);
            window.pointer_down(from, MouseButton::Left, cx);
            window.pointer_move(to, Some(MouseButton::Left), cx);
            window.pointer_move(from, Some(MouseButton::Left), cx);
            window.pointer_up(from, MouseButton::Left, cx);
        })
        .unwrap();
        drain_ready_tasks(cx);
        cx.update(|cx| {
            let state = app.read(cx);
            assert!(state.calendar_drag.is_none());
            assert!(state.workspace_edit.is_none());
            assert_eq!(state.calendar.selected_range(), selection);
            assert_eq!(
                state
                    .workspace_range
                    .as_ref()
                    .map(|range| range.source.clone()),
                source
            );
        });
        assert_eq!(selected_journal(cx, &app), "20260123");
        assert_eq!(files(folder.path()), before);
        click(cx, window, "calendar-day-20260121");
        assert_eq!(selected_journal(cx, &app), "20260121");
        assert!(cx.update(|cx| app.read(cx).workspace_range.is_none()));
    }
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn year_view_scrolls_to_the_current_month_and_other_views_keep_that_date_visible(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    write_journal(folder.path(), "20261119", "md", MARKDOWN);
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    click(cx, window, "calendar-view-year");
    cx.update_window(window, |_, window, _| {
        assert!(window.find("calendar-day-20261119").visible());
    })
    .unwrap();

    click(cx, window, "calendar-view-month");
    cx.update_window(window, |_, window, _| {
        assert!(window.find("calendar-day-20261119").visible());
        assert!(window.try_find("calendar-day-20261019").is_none());
    })
    .unwrap();
    click(cx, window, "calendar-view-week");
    cx.update_window(window, |_, window, _| {
        assert!(window.find("calendar-day-20261119").visible());
        assert!(window.try_find("calendar-day-20261116").is_some());
        assert!(window.try_find("calendar-day-20261122").is_some());
        assert!(window.try_find("calendar-day-20261123").is_none());
    })
    .unwrap();
    assert_eq!(selected_journal(cx, &app), "20261119");
    assert_eq!(
        cx.update(|cx| app.read(cx).calendar.selected_date()),
        date("20261119")
    );
    assert_eq!(files(folder.path()), before);
}

#[gpui_kit::test]
fn escape_cancels_an_in_progress_drag_and_its_mouse_release_cannot_open_a_day(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    cx.update_window(window, |_, window, cx| {
        let from = window.find("calendar-day-20260121").bounds().center();
        let to = window.find("calendar-day-20260123").bounds().center();
        window.pointer_move(from, None, cx);
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
        window.press("escape", cx);
        window.pointer_move(from, Some(MouseButton::Left), cx);
        window.pointer_up(from, MouseButton::Left, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let state = app.read(cx);
        assert!(state.calendar_drag.is_none());
        assert!(state.workspace_range.is_none());
        assert!(state.workspace_edit.is_none());
        assert!(!state.calendar.is_range());
        assert_eq!(state.calendar.selected_date(), date("20260123"));
    });
    assert_eq!(selected_journal(cx, &app), "20260123");
    assert_eq!(files(folder.path()), before);
    click(cx, window, "calendar-day-20260121");
    assert_eq!(selected_journal(cx, &app), "20260121");
}

#[gpui_kit::test]
fn releasing_a_drag_outside_the_calendar_keeps_the_last_valid_day_and_does_not_stick(
    cx: &mut TestAppContext,
) {
    let folder = fixture();
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    cx.update_window(window, |_, window, cx| {
        let from = window.find("calendar-day-20260121").bounds().center();
        let to = window.find("calendar-day-20260123").bounds().center();
        let outside = window.find("choose-folder").bounds().center();
        window.pointer_move(from, None, cx);
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
        window.pointer_move(outside, Some(MouseButton::Left), cx);
        window.pointer_up(outside, MouseButton::Left, cx);
    })
    .unwrap();
    drain_ready_tasks(cx);
    cx.update(|cx| {
        let state = app.read(cx);
        assert!(state.calendar_drag.is_none());
        assert!(!state.workspace_busy);
        assert!(state.workspace_edit.is_none());
        let range = state.workspace_range.as_ref().unwrap();
        assert_eq!(
            (range.start, range.end),
            (date("20260121"), date("20260123"))
        );
    });
    assert!(!cx.has_pending_prompt());
    assert_eq!(files(folder.path()), before);
    click(cx, window, "close-range");
    cx.update(|cx| {
        let state = app.read(cx);
        assert!(state.workspace_range.is_none());
        assert_eq!(state.calendar.selected_date(), date("20260123"));
    });
    drag_days(cx, window, "20260123", "20260121");
    assert!(cx.update(|cx| app.read(cx).workspace_range.is_some()));
}

#[gpui_kit::test]
fn month_view_neighboring_days_remain_clickable_but_cannot_start_or_extend_a_drag(
    cx: &mut TestAppContext,
) {
    let folder = tempfile::tempdir().unwrap();
    write_journal(folder.path(), "20261230", "txt", PLAIN);
    write_journal(folder.path(), "20270103", "md", MARKDOWN);
    let before = files(folder.path());
    let (window, app) = open_window(cx);
    open_workspace(cx, window, &app, folder.path());
    drag_days(cx, window, "20270101", "20261230");
    cx.update(|cx| {
        let state = app.read(cx);
        assert!(state.workspace_range.is_none());
        assert!(state.workspace_edit.is_none());
    });
    drag_days(cx, window, "20261230", "20270101");
    assert!(cx.update(|cx| app.read(cx).workspace_range.is_none()));
    assert!(cx.update(|cx| app.read(cx).workspace_edit.is_none()));
    assert_eq!(selected_journal(cx, &app), "20270103");
    click(cx, window, "calendar-day-20261230");
    assert_eq!(selected_journal(cx, &app), "20261230");
    assert_eq!(
        cx.update(|cx| app.read(cx).calendar.month_label()),
        "2027 年 1 月"
    );
    assert_eq!(files(folder.path()), before);
}
