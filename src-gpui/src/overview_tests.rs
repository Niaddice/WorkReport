use std::fs;

use chrono::NaiveDate;
use gpui_kit::{AppContext, TestAppContext, test::TestWindowExt};
use workreport_core::{
    holidays::{Day, HolidayState, Year},
    workspace::{Entry, Format},
};

use crate::{
    calendar_holidays::HolidayCalendar,
    overview::{activity_level, build_heatmap_grid, compute_metrics},
    workspace_edit_tests::{click, drain_ready_tasks, open_window, open_workspace},
};

fn entry(date: &str, format: Format) -> Entry {
    Entry {
        date: date.into(),
        format,
        filename: format!("{date}.{}", format.extension()),
    }
}

#[test]
fn computes_metrics_streak_and_holiday_labels_accurately() {
    let today = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap(); // Thursday
    let holidays = HolidayCalendar::from_state(HolidayState {
        years: vec![Year {
            year: 2026,
            days: vec![Day {
                date: "2026-11-19".into(),
                name: "测试节".into(),
                is_off_day: true,
            }],
            synced_at: String::new(),
            source_url: String::new(),
        }],
        failures: Vec::new(),
    });
    let entries = vec![
        entry("20261001", Format::Txt),
        entry("20261116", Format::Md),
        entry("20261117", Format::Md),
        entry("20261118", Format::Txt),
        entry("20261119", Format::Md),
    ];
    let metrics = compute_metrics(&entries, today, &holidays);
    assert!(metrics.today_recorded);
    assert_eq!(metrics.today_format, Some(Format::Md));
    assert_eq!(metrics.holiday_label.as_deref(), Some("测试节 · 放假"));
    assert_eq!(metrics.week_count, 4);
    assert_eq!(metrics.month_count, 4);
    assert_eq!(metrics.year_count, 5);
    assert_eq!(metrics.streak_days, 4);

    assert_eq!(activity_level(false, Some(2000)), 0);
    assert_eq!(activity_level(true, Some(50)), 1);
    assert_eq!(activity_level(true, Some(250)), 2);
    assert_eq!(activity_level(true, Some(600)), 3);
    assert_eq!(activity_level(true, Some(1000)), 4);
}

#[test]
fn builds_heatmap_grids_for_all_window_sizes() {
    let today = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
    let g365 = build_heatmap_grid(today, 365);
    assert!(g365.row_labels);
    assert_eq!(g365.rows.len(), 7);
    let count_365: usize = g365
        .rows
        .iter()
        .flat_map(|row| row.days.iter())
        .filter(|d| d.is_some())
        .count();
    assert_eq!(count_365, 365);

    let g30 = build_heatmap_grid(today, 30);
    assert!(!g30.row_labels);
    assert_eq!(g30.columns.len(), 7);

    let g7 = build_heatmap_grid(today, 7);
    assert_eq!(g7.rows.len(), 1);
    assert_eq!(g7.rows[0].days.len(), 7);

    let g1 = build_heatmap_grid(today, 1);
    assert_eq!(g1.rows[0].days, vec![Some(today)]);
}

#[gpui_kit::test]
fn overview_loads_word_counts_switches_windows_and_jumps_on_heatmap_click(cx: &mut TestAppContext) {
    let temporary = tempfile::tempdir().unwrap();
    let folder = temporary.path();
    fs::create_dir(folder.join("2026")).unwrap();
    let today = chrono::Local::now().date_naive();
    let d_yesterday = today.checked_sub_days(chrono::Days::new(1)).unwrap();
    let k_today = today.format("%Y%m%d").to_string();
    let k_yesterday = d_yesterday.format("%Y%m%d").to_string();
    let y_year = d_yesterday.format("%Y").to_string();
    let t_year = today.format("%Y").to_string();
    fs::create_dir_all(folder.join(&y_year)).unwrap();
    fs::create_dir_all(folder.join(&t_year)).unwrap();
    fs::write(
        folder.join(format!("{y_year}/{k_yesterday}.txt")),
        "昨天的日报内容记录了六项完成事项",
    )
    .unwrap();
    fs::write(
        folder.join(format!("{t_year}/{k_today}.md")),
        "# 今日工作\n完成概览与热力图回归测试\n",
    )
    .unwrap();

    let (window, prototype) = open_window(cx);
    open_workspace(cx, window, &prototype, folder);
    click(cx, window, "rail-module-overview");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    drain_ready_tasks(cx);

    cx.update(|cx| {
        let app = prototype.read(cx);
        assert!(!app.overview.loading);
        assert!(app.overview.word_counts.contains_key(&k_yesterday));
        assert!(app.overview.word_counts.contains_key(&k_today));
    });

    click(cx, window, "overview-window-30");
    cx.update(|cx| {
        assert_eq!(prototype.read(cx).overview.window_days, 30);
    });

    // Click yesterday on heatmap -> calendar and journal jump to yesterday.
    click(cx, window, format!("overview-heatmap-day-{k_yesterday}"));
    cx.update(|cx| {
        let app = prototype.read(cx);
        assert_eq!(app.calendar.selected_date(), d_yesterday);
        assert_eq!(
            app.workspace
                .as_ref()
                .unwrap()
                .journal
                .as_ref()
                .unwrap()
                .date(),
            k_yesterday
        );
    });
}
