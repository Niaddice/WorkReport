use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use chrono::{Datelike, Days, NaiveDate};
use gpui_kit::{
    Context, FontWeight, Task, Window,
    base::Button as FlatButton,
    component::ActiveTheme,
    div, prelude::*, px,
};
use workreport_core::workspace::{Entry, Format};

use crate::{
    Prototype,
    calendar_holidays::{DayKind, HolidayCalendar},
    workspace_view::WorkspaceView,
};

pub(crate) const HEATMAP_WINDOWS: [usize; 5] = [365, 90, 30, 7, 1];

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActivityKey {
    workspace: usize,
    entries_len: usize,
    today: NaiveDate,
    revision: u64,
}

pub(crate) struct OverviewState {
    pub(crate) window_days: usize,
    pub(crate) word_counts: BTreeMap<String, Option<usize>>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) selected_info: Option<String>,
    activity_key: Option<ActivityKey>,
    activity_revision: u64,
    request_id: u64,
    task: Option<Task<()>>,
}

impl Default for OverviewState {
    fn default() -> Self {
        Self {
            window_days: 365,
            word_counts: BTreeMap::new(),
            loading: false,
            error: None,
            selected_info: None,
            activity_key: None,
            activity_revision: 0,
            request_id: 0,
            task: None,
        }
    }
}

impl OverviewState {
    pub(crate) fn reset(&mut self) {
        self.word_counts.clear();
        self.loading = false;
        self.error = None;
        self.selected_info = None;
        self.activity_key = None;
        self.activity_revision += 1;
        self.request_id += 1;
        self.task = None;
    }

    pub(crate) fn invalidate(&mut self) {
        self.activity_key = None;
        self.activity_revision += 1;
    }

    pub(crate) fn set_window_days(&mut self, days: usize) {
        if HEATMAP_WINDOWS.contains(&days) {
            self.window_days = days;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OverviewMetrics {
    pub(crate) today_recorded: bool,
    pub(crate) today_format: Option<Format>,
    pub(crate) holiday_label: Option<String>,
    pub(crate) month_count: usize,
    pub(crate) year_count: usize,
    pub(crate) week_count: usize,
    pub(crate) streak_days: usize,
}

pub(crate) fn compute_metrics(
    entries: &[Entry],
    today: NaiveDate,
    holidays: &HolidayCalendar,
) -> OverviewMetrics {
    let mut record_dates = BTreeSet::new();
    let mut today_format = None;
    let mut month_count = 0;
    let mut year_count = 0;
    let mut week_count = 0;

    let week_offset = i64::from(today.weekday().num_days_from_monday());
    let week_start = today
        .checked_sub_days(Days::new(week_offset as u64))
        .unwrap_or(today);
    let week_end = week_start.checked_add_days(Days::new(6)).unwrap_or(today);

    for entry in entries {
        let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y%m%d") else {
            continue;
        };
        record_dates.insert(date);
        if date == today {
            today_format = Some(entry.format);
        }
        if date.year() == today.year() {
            year_count += 1;
            if date.month() == today.month() {
                month_count += 1;
            }
        }
        if date >= week_start && date <= week_end {
            week_count += 1;
        }
    }

    let mut streak_days = 0;
    let mut cursor = if record_dates.contains(&today) {
        Some(today)
    } else {
        today.checked_sub_days(Days::new(1))
    };
    while let Some(day) = cursor {
        if !record_dates.contains(&day) {
            break;
        }
        streak_days += 1;
        cursor = day.checked_sub_days(Days::new(1));
    }

    let holiday_label = holidays.metadata(today).map(|meta| match meta.kind {
        DayKind::Holiday => format!("{} · 放假", meta.name),
        DayKind::Workday => format!("{} · 调休上班", meta.name),
    });

    OverviewMetrics {
        today_recorded: today_format.is_some(),
        today_format,
        holiday_label,
        month_count,
        year_count,
        week_count,
        streak_days,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeatmapColumn {
    pub(crate) label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeatmapRow {
    pub(crate) label: String,
    pub(crate) days: Vec<Option<NaiveDate>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeatmapGrid {
    pub(crate) days: usize,
    pub(crate) start: NaiveDate,
    pub(crate) end: NaiveDate,
    pub(crate) row_labels: bool,
    pub(crate) columns: Vec<HeatmapColumn>,
    pub(crate) rows: Vec<HeatmapRow>,
}

const WEEKDAY_LABELS: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

pub(crate) fn build_heatmap_grid(today: NaiveDate, days: usize) -> HeatmapGrid {
    let days = days.clamp(1, 365);
    let start = today
        .checked_sub_days(Days::new((days - 1) as u64))
        .unwrap_or(today);

    if days <= 7 {
        let mut columns = Vec::with_capacity(days);
        let mut row_days = Vec::with_capacity(days);
        let mut cursor = Some(start);
        while let Some(day) = cursor {
            if day > today {
                break;
            }
            let weekday_idx = day.weekday().num_days_from_monday() as usize;
            columns.push(HeatmapColumn {
                label: WEEKDAY_LABELS[weekday_idx].to_string(),
            });
            row_days.push(Some(day));
            cursor = day.checked_add_days(Days::new(1));
        }
        let label = if days == 1 {
            "今天".to_string()
        } else {
            format!("近 {days} 天")
        };
        return HeatmapGrid {
            days,
            start,
            end: today,
            row_labels: false,
            columns,
            rows: vec![HeatmapRow {
                label,
                days: row_days,
            }],
        };
    }

    let offset = u64::from(start.weekday().num_days_from_monday());
    let first = start.checked_sub_days(Days::new(offset)).unwrap_or(start);
    let span = (today - first).num_days().max(0) as usize + 1;
    let week_count = span.div_ceil(7);

    let mut weeks: Vec<(NaiveDate, Vec<Option<NaiveDate>>, String)> =
        Vec::with_capacity(week_count);
    for col in 0..week_count {
        let week_start = first
            .checked_add_days(Days::new((col * 7) as u64))
            .unwrap_or(today);
        let mut values = Vec::with_capacity(7);
        let mut month_label = String::new();
        for row in 0..7 {
            let day = week_start
                .checked_add_days(Days::new(row as u64))
                .unwrap_or(today);
            if day < start || day > today {
                values.push(None);
            } else {
                if month_label.is_empty() && day.day() == 1 {
                    month_label = format!("{}月", day.month());
                }
                values.push(Some(day));
            }
        }
        weeks.push((week_start, values, month_label));
    }

    if days == 30 {
        let columns = WEEKDAY_LABELS
            .iter()
            .map(|label| HeatmapColumn {
                label: (*label).to_string(),
            })
            .collect();
        let rows = weeks
            .into_iter()
            .map(|(week_start, values, _)| HeatmapRow {
                label: format!("{}月{}日", week_start.month(), week_start.day()),
                days: values,
            })
            .collect();
        return HeatmapGrid {
            days,
            start,
            end: today,
            row_labels: false,
            columns,
            rows,
        };
    }

    let columns = weeks
        .iter()
        .map(|(_, _, label)| HeatmapColumn {
            label: label.clone(),
        })
        .collect();
    let rows = WEEKDAY_LABELS
        .iter()
        .enumerate()
        .map(|(row_idx, label)| HeatmapRow {
            label: (*label).to_string(),
            days: weeks.iter().map(|(_, values, _)| values[row_idx]).collect(),
        })
        .collect();

    HeatmapGrid {
        days,
        start,
        end: today,
        row_labels: true,
        columns,
        rows,
    }
}

pub(crate) fn activity_level(recorded: bool, word_count: Option<usize>) -> u8 {
    if !recorded {
        return 0;
    }
    match word_count.unwrap_or(0) {
        1000.. => 4,
        500.. => 3,
        200.. => 2,
        _ => 1,
    }
}

pub(crate) fn format_heatmap_day_label(
    date: NaiveDate,
    today: NaiveDate,
    recorded: bool,
    word_count: Option<usize>,
    loading: bool,
) -> String {
    let weekday = WEEKDAY_LABELS[date.weekday().num_days_from_monday() as usize];
    let today_suffix = if date == today { "，今天" } else { "" };
    let prefix = format!(
        "{}年{}月{}日 星期{}{}",
        date.year(),
        date.month(),
        date.day(),
        weekday,
        today_suffix
    );
    if !recorded {
        return format!("{prefix}，未记录");
    }
    let detail = match word_count {
        Some(count) => format!("{count} 字"),
        None if loading => "字数统计中".to_string(),
        None => "字数暂不可用".to_string(),
    };
    format!("{prefix}，已记录，{detail}")
}

impl Prototype {
    pub(crate) fn ensure_overview_activity(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        let today = self.calendar_details.today;
        let key = ActivityKey {
            workspace: Arc::as_ptr(&workspace.reader) as usize,
            entries_len: workspace.entries.len(),
            today,
            revision: self.overview.activity_revision,
        };
        if self.overview.activity_key.as_ref() == Some(&key) {
            return;
        }
        self.overview.activity_key = Some(key.clone());
        self.overview.request_id += 1;
        let request_id = self.overview.request_id;
        self.overview.task = None;
        self.overview.error = None;

        let start = today.checked_sub_days(Days::new(364)).unwrap_or(today);
        let start_key = start.format("%Y%m%d").to_string();
        let end_key = today.format("%Y%m%d").to_string();
        let requested_dates: Vec<String> = workspace
            .entries
            .iter()
            .filter(|entry| entry.date >= start_key && entry.date <= end_key)
            .map(|entry| entry.date.clone())
            .collect();

        if requested_dates.is_empty() {
            self.overview.loading = false;
            self.overview.word_counts.clear();
            return;
        }

        self.overview.loading = true;
        let reader = workspace.reader.clone();
        let load = cx
            .background_executor()
            .spawn(async move { reader.daily_activity(&requested_dates) });
        self.overview.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.overview.request_id != request_id
                    || this.overview.activity_key.as_ref() != Some(&key)
                {
                    return;
                }
                this.overview.loading = false;
                match result {
                    Ok(items) => {
                        this.overview.word_counts = items
                            .into_iter()
                            .map(|item| (item.date, item.word_count))
                            .collect();
                        this.overview.error = None;
                    }
                    Err(error) => {
                        this.overview.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }
}

pub(crate) fn render_overview_module(
    app: &Prototype,
    workspace: &WorkspaceView,
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let accent = theme.primary;
    let secondary = theme.secondary;
    let foreground = theme.foreground;

    let today = app.calendar_details.today;
    let metrics = compute_metrics(&workspace.entries, today, &app.calendar_details.holidays);
    let recorded_set: BTreeSet<String> = workspace
        .entries
        .iter()
        .map(|entry| entry.date.clone())
        .collect();
    let grid = build_heatmap_grid(today, app.overview.window_days);

    let today_state = match metrics.today_format {
        Some(Format::Md) => "已记录 · MD",
        Some(Format::Txt) => "已记录 · TXT",
        None => "待记录",
    };

    let visible_days: Vec<NaiveDate> = grid
        .rows
        .iter()
        .flat_map(|row| row.days.iter().flatten().copied())
        .collect();
    let recorded_in_window = visible_days
        .iter()
        .filter(|date| recorded_set.contains(&date.format("%Y%m%d").to_string()))
        .count();

    let period_label = if grid.days == 1 {
        "今天".to_string()
    } else {
        format!("近 {} 天", grid.days)
    };
    let range_label = if grid.days == 1 {
        grid.end.format("%Y.%m.%d").to_string()
    } else {
        format!(
            "{} – {}",
            grid.start.format("%Y.%m.%d"),
            grid.end.format("%Y.%m.%d")
        )
    };

    let cell_size = match grid.days {
        1 => 28.0,
        7 => 22.0,
        30 => 16.0,
        90 => 13.0,
        _ => 10.0,
    };

    let metric_cards = [
        ("本月日报", metrics.month_count),
        ("本年日报", metrics.year_count),
        ("本周日报", metrics.week_count),
        ("连续记录天数", metrics.streak_days),
    ];

    div()
        .id("overview-module")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2p5()
        .p_1()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(div().text_size(px(13.)).child("今日日报"))
                .child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(if metrics.today_recorded { accent } else { border })
                        .bg(if metrics.today_recorded {
                            accent.opacity(0.14)
                        } else {
                            secondary
                        })
                        .text_size(px(11.))
                        .text_color(if metrics.today_recorded {
                            accent
                        } else {
                            muted
                        })
                        .child(today_state),
                )
                .when_some(metrics.holiday_label.clone(), |bar, label| {
                    bar.child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded(px(4.))
                            .bg(secondary)
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(label),
                    )
                }),
        )
        .child(if workspace.entries.is_empty() {
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child("从今天的第一篇日报开始")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .border_t_1()
                .border_b_1()
                .border_color(border)
                .py_1()
                .children(metric_cards.into_iter().map(|(label, value)| {
                    div()
                        .flex_1()
                        .min_w(px(85.))
                        .px_2p5()
                        .py_1p5()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_size(px(18.))
                                .text_color(foreground)
                                .child(value.to_string()),
                        )
                        .child(div().text_size(px(11.)).text_color(muted).child(label))
                }))
                .into_any_element()
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .pt_1()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(foreground)
                                                .child("每日热力图"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(muted)
                                                .child(format!("{period_label} · 已记录 {recorded_in_window} 天")),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(muted)
                                        .child(range_label),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .children(HEATMAP_WINDOWS.into_iter().map(|days| {
                                    let active = app.overview.window_days == days;
                                    let label = if days == 1 {
                                        "今日".to_string()
                                    } else {
                                        format!("{days}天")
                                    };
                                    FlatButton::new(format!("overview-window-{days}"))
                                        .selected(active)
                                        .h(px(22.))
                                        .px_2()
                                        .rounded(px(5.))
                                        .bg(if active {
                                            accent.opacity(0.14)
                                        } else {
                                            secondary.opacity(0.35)
                                        })
                                        .text_size(px(11.))
                                        .text_color(if active { accent } else { muted })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.overview.set_window_days(days);
                                            cx.notify();
                                        }))
                                })),
                        ),
                )
                .child(
                    div()
                        .id("overview-heatmap-scroll")
                        .overflow_x_scroll()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .when(!grid.columns.is_empty() && grid.days > 1, |root| {
                                    root.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .when(grid.row_labels, |row| {
                                                row.child(div().w(px(16.)))
                                            })
                                            .children(grid.columns.iter().map(|col| {
                                                div()
                                                    .w(px(cell_size))
                                                    .text_size(px(9.))
                                                    .text_color(muted)
                                                    .child(col.label.clone())
                                            })),
                                    )
                                })
                                .children(grid.rows.iter().map(|row| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .when(grid.row_labels, |line| {
                                            line.child(
                                                div()
                                                    .w(px(16.))
                                                    .text_size(px(9.))
                                                    .text_color(muted)
                                                    .child(row.label.clone()),
                                            )
                                        })
                                        .children(row.days.iter().map(|maybe_day| {
                                            let Some(day) = maybe_day else {
                                                return div()
                                                    .w(px(cell_size))
                                                    .h(px(cell_size))
                                                    .into_any_element();
                                            };
                                            let key = day.format("%Y%m%d").to_string();
                                            let recorded = recorded_set.contains(&key);
                                            let count = app
                                                .overview
                                                .word_counts
                                                .get(&key)
                                                .copied()
                                                .flatten();
                                            let lvl = activity_level(recorded, count);
                                            let tooltip = format_heatmap_day_label(
                                                *day,
                                                today,
                                                recorded,
                                                count,
                                                app.overview.loading,
                                            );
                                            let fill = match lvl {
                                                0 => secondary.opacity(0.65),
                                                1 => accent.opacity(0.28),
                                                2 => accent.opacity(0.50),
                                                3 => accent.opacity(0.75),
                                                _ => accent,
                                            };
                                            let is_selected =
                                                app.calendar.selected_date() == *day;
                                            let date_copy = *day;
                                            let info_copy = tooltip.clone();
                                            FlatButton::new(format!("overview-heatmap-day-{key}"))
                                                .accessibility_label(tooltip)
                                                .disabled(busy)
                                                .w(px(cell_size))
                                                .h(px(cell_size))
                                                .rounded(px(2.5))
                                                .when(is_selected || *day == today, |b| {
                                                    b.border_1().border_color(accent)
                                                })
                                                .bg(fill)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.overview.selected_info =
                                                            Some(info_copy.clone());
                                                        this.select_calendar_date(
                                                            date_copy, false, window, cx,
                                                        );
                                                    },
                                                ))
                                                .into_any_element()
                                        }))
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_size(px(11.)).text_color(muted).child(
                            app.overview
                                .error
                                .clone()
                                .or_else(|| app.overview.selected_info.clone())
                                .unwrap_or_else(|| {
                                    if app.overview.loading {
                                        "正在统计字数…".to_string()
                                    } else {
                                        "按日报字数".to_string()
                                    }
                                }),
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().text_size(px(10.)).text_color(muted).child("少"))
                                .children([0u8, 1, 2, 3, 4].into_iter().map(|lvl| {
                                    let fill = match lvl {
                                        0 => secondary.opacity(0.65),
                                        1 => accent.opacity(0.28),
                                        2 => accent.opacity(0.50),
                                        3 => accent.opacity(0.75),
                                        _ => accent,
                                    };
                                    div()
                                        .w(px(9.))
                                        .h(px(9.))
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(border.opacity(0.5))
                                        .bg(fill)
                                }))
                                .child(div().text_size(px(10.)).text_color(muted).child("多")),
                        ),
                ),
        )
}
