use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use gpui_kit::{
    Animation, AnimationExt, ClickEvent, ClipboardItem, Context, ElementId, FontWeight,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollHandle, SharedString, Window,
    base::{Button as FlatButton, SelectableText},
    component::ActiveTheme,
    div, ease_out_quint, point,
    prelude::*,
    px,
};
use workreport_core::workspace::{Entry, Format};

use crate::{
    Prototype,
    calendar::{CalendarDay, CalendarState, CalendarView},
    calendar_details::CalendarDetails,
    calendar_holidays::DayKind,
    icons::{PhIcon, ph_icon},
    lunar::lunar_label,
    markdown_preview,
    workspace_range::WorkspaceRange,
};

const YEAR_ROW_HEIGHT: f32 = 200.;
const YEAR_ROW_GAP: f32 = 12.;

pub(super) struct CalendarPresentation<'a> {
    pub(super) selection: &'a CalendarState,
    pub(super) year_scroll: &'a ScrollHandle,
    pub(super) details: &'a CalendarDetails,
}

/// The tentative range never changes the loaded document or writes a journal.
pub(super) struct CalendarDrag {
    pub(super) selection: CalendarState,
    visited_other_day: bool,
}

fn render_day(
    day: CalendarDay,
    compact: bool,
    calendar: &CalendarState,
    details: &CalendarDetails,
    entries: &[Entry],
    busy: bool,
    cx: &Context<Prototype>,
) -> FlatButton {
    let date = day.date;
    let key = date.format("%Y%m%d").to_string();
    let recorded = entries.iter().any(|entry| entry.date == key);
    let selected = calendar.is_selected(date);
    let is_range = calendar.is_range();
    let (range_start, range_end) = calendar.selected_range();
    let is_endpoint = !is_range || date == range_start || date == range_end;
    let draggable = day.in_month;
    let lunar = lunar_label(date);
    let holiday = details.holidays.metadata(date);
    let summary = (!compact && recorded).then(|| match details.previews.values.get(&key) {
        Some(Ok(value)) => value.clone(),
        Some(Err(error)) => format!("无法读取摘要：{error}"),
        None => "加载中…".to_owned(),
    });
    let mut label = format!(
        "{}，{}{}",
        date.format("%Y-%m-%d"),
        if recorded {
            "已有日报"
        } else {
            "暂无日报"
        },
        if day.in_month { "" } else { "，非本月" }
    );
    if let Some(lunar) = &lunar {
        label.push_str(&format!("，农历{lunar}"));
    }
    if let Some(holiday) = &holiday {
        label.push_str(&format!(
            "，{}{}",
            if holiday.kind == DayKind::Holiday {
                "休息："
            } else {
                ""
            },
            holiday.name
        ));
    } else if details.holidays.is_weekend(date) {
        label.push_str("，周末");
    }
    if let Some(summary) = &summary {
        label.push_str(&format!("，摘要：{summary}"));
    }

    let palette = crate::theme_skin::active_palette(cx);
    let danger = cx.theme().red;
    let primary_selected = selected && is_endpoint;
    let middle_range = selected && !is_endpoint;
    let is_today = date == details.today;
    let is_weekend = details.holidays.is_weekend(date);

    let mut button = FlatButton::new(format!("calendar-day-{key}"))
        .accessibility_label(label)
        .selected(selected)
        .disabled(busy)
        .flex_1()
        .min_w_0();

    button = if compact {
        let fg = if is_today {
            palette.on_accent
        } else if selected {
            palette.accent
        } else if is_weekend {
            palette.text_muted
        } else {
            palette.text_body
        };
        let bg = if is_today {
            palette.accent_fill
        } else if selected {
            palette.accent_soft
        } else {
            palette.surface_header
        };
        button
            .h(px(20.))
            .rounded(px(6.))
            .bg(bg)
            .hover(move |s| s.bg(palette.accent_soft))
            .text_size(px(10.5))
            .text_color(fg)
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(date.day().to_string())
            .when(recorded, |b| {
                b.child(
                    div()
                        .w(px(3.))
                        .h(px(3.))
                        .rounded(px(1.5))
                        .bg(if is_today {
                            palette.on_accent
                        } else {
                            palette.accent
                        }),
                )
            })
    } else {
        let cell_bg = if primary_selected {
            palette.accent_soft
        } else if middle_range {
            palette.canvas
        } else {
            palette.surface_header
        };
        let cell_border = if primary_selected {
            palette.accent
        } else if middle_range {
            palette.accent_border
        } else {
            palette.surface_header
        };
        let day_num_fg = if is_today {
            palette.on_accent
        } else if selected {
            palette.accent
        } else if is_weekend {
            palette.text_muted
        } else {
            palette.text
        };
        let lunar_text = lunar.clone().unwrap_or_default();
        let festival_text = holiday
            .as_ref()
            .filter(|h| h.kind == DayKind::Holiday && !h.name.is_empty())
            .map(|h| h.name.clone())
            .unwrap_or_default();
        let is_week_view = calendar.view() == CalendarView::Week;

        let mut cell_btn = button
            .h_full()
            .min_h(px(62.))
            .px_2()
            .pt(px(8.))
            .pb(px(6.))
            .border_1()
            .border_color(cell_border)
            .bg(cell_bg)
            .when(!selected, move |b| b.hover(move |s| s.bg(palette.surface)))
            .when(!day.in_month, |b| b.opacity(0.35))
            .flex()
            .flex_col()
            .justify_start()
            .items_stretch()
            .gap_1()
            .overflow_hidden();

        cell_btn = if is_range && selected {
            if date == range_start && date == range_end {
                cell_btn.rounded(px(10.))
            } else if date == range_start {
                cell_btn.rounded_l(px(10.)).rounded_r(px(0.))
            } else if date == range_end {
                cell_btn.rounded_r(px(10.)).rounded_l(px(0.))
            } else {
                cell_btn.rounded(px(0.))
            }
        } else {
            cell_btn.rounded(px(6.))
        };

        cell_btn
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .w_full()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .h(px(26.))
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w(px(26.))
                                            .h(px(26.))
                                            .rounded(px(13.))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(is_today, |d| d.bg(palette.accent_fill))
                                            .text_size(px(13.5))
                                            .font_weight(if is_today || selected {
                                                FontWeight::SEMIBOLD
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .text_color(day_num_fg)
                                            .child(date.day().to_string()),
                                    )
                                    .when(!lunar_text.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .text_size(px(10.5))
                                                .text_color(palette.text_muted)
                                                .truncate()
                                                .child(lunar_text),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .flex_shrink_0()
                                    .when_some(holiday.as_ref(), |row, holiday| {
                                        let is_rest = holiday.kind == DayKind::Holiday;
                                        let badge_bg = if is_rest {
                                            danger.opacity(0.14)
                                        } else {
                                            palette.subtle
                                        };
                                        let badge_fg = if is_rest {
                                            danger
                                        } else {
                                            palette.text_muted
                                        };
                                        row.child(
                                            div()
                                                .px_1()
                                                .py(px(1.))
                                                .rounded(px(4.))
                                                .bg(badge_bg)
                                                .text_size(px(9.5))
                                                .text_color(badge_fg)
                                                .child(if is_rest { "休" } else { "班" }),
                                        )
                                    }),
                            ),
                    )
                    .when(!festival_text.is_empty(), |top| {
                        top.child(
                            div()
                                .pl(px(4.))
                                .text_size(px(10.))
                                .text_color(danger)
                                .truncate()
                                .child(festival_text),
                        )
                    }),
            )
            .when_some(summary, |col, summary| {
                col.child(
                    div()
                        .mt_1()
                        .px(px(5.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .bg(palette.accent_soft)
                        .flex()
                        .items_start()
                        .gap_1()
                        .w_full()
                        .min_w_0()
                        .child(
                            div()
                                .pt(px(1.))
                                .flex_shrink_0()
                                .child(ph_icon(PhIcon::FileText, px(11.), palette.accent)),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(palette.accent)
                                .when(!is_week_view, |txt| txt.truncate())
                                .child(summary),
                        ),
                )
            })
    };
    button
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                this.calendar_suppress_click = false;
                if draggable {
                    this.begin_calendar_drag(date, window, cx);
                }
            }),
        )
        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                if draggable {
                    this.move_calendar_drag(date, cx);
                }
            } else {
                this.cancel_calendar_drag(cx);
            }
        }))
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            if matches!(event, ClickEvent::Mouse(_)) && this.calendar_suppress_click {
                this.calendar_suppress_click = false;
                return;
            }
            this.select_calendar_date(date, event.modifiers().shift, window, cx);
        }))
}

fn weekdays(compact: bool, cx: &Context<Prototype>) -> impl IntoElement {
    let labels: [&str; 7] = if compact {
        ["一", "二", "三", "四", "五", "六", "日"]
    } else {
        ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
    };
    div()
        .flex()
        .gap_1()
        .pb(px(if compact { 2. } else { 7. }))
        .children(labels.into_iter().map(|day| {
            div()
                .flex_1()
                .min_w_0()
                .text_center()
                .text_size(px(if compact { 10. } else { 12.5 }))
                .text_color(cx.theme().muted_foreground)
                .child(day)
        }))
}

fn render_grid(
    days: &[CalendarDay],
    compact: bool,
    calendar: &CalendarState,
    details: &CalendarDetails,
    entries: &[Entry],
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .when(!compact, |g| g.flex_1().min_h_0())
        .gap_1()
        .children(days.chunks(7).map(|week| {
            div()
                .flex()
                .when(!compact, |row| row.flex_1().min_h(px(62.)))
                .gap_1()
                .children(week.iter().map(|day| {
                    if compact && !day.in_month {
                        // Year grids keep weekday alignment without duplicate date identities.
                        div().flex_1().min_w_0().h(px(20.)).into_any_element()
                    } else {
                        render_day(*day, compact, calendar, details, entries, busy, cx)
                            .into_any_element()
                    }
                }))
        }))
}

#[inline(never)]
pub(super) fn render_calendar(
    presentation: &CalendarPresentation<'_>,
    entries: &[Entry],
    busy: bool,
    cx: &Context<Prototype>,
) -> gpui_kit::AnyElement {
    let calendar = presentation.selection;
    let details = presentation.details;
    let year_scroll = presentation.year_scroll;
    let palette = crate::theme_skin::active_palette(cx);

    let anim_key = ElementId::Name(
        format!("cal-grid-{:?}-{}", calendar.view(), calendar.period_label()).into(),
    );
    let body = if calendar.view() == CalendarView::Year {
        let months = calendar.months();
        div()
            .id("calendar-year-grid")
            .flex()
            .flex_col()
            .gap(px(YEAR_ROW_GAP))
            .flex_1()
            .min_h_0()
            .px_3()
            .pb_2()
            .overflow_y_scroll()
            .track_scroll(year_scroll)
            .children(months.chunks(2).map(|row| {
                div()
                    .flex()
                    .flex_shrink_0()
                    .h(px(YEAR_ROW_HEIGHT))
                    .items_start()
                    .gap_3()
                    .children(row.iter().map(|month| {
                        let month = *month;
                        let days = CalendarState::month_days(month);
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                FlatButton::new(format!("calendar-month-{}", month.format("%Y%m")))
                                    .disabled(busy)
                                    .w_full()
                                    .py_0p5()
                                    .rounded(px(4.))
                                    .justify_start()
                                    .hover(move |s| s.text_color(palette.accent))
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text_body)
                                    .child(format!("{} 月", month.month()))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.cancel_calendar_drag(cx);
                                        this.calendar.open_month(month);
                                        cx.notify();
                                    })),
                            )
                            .child(weekdays(true, cx))
                            .child(render_grid(
                                &days, true, calendar, details, entries, busy, cx,
                            ))
                    }))
            }))
            .with_animation(
                anim_key,
                Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                |el, t| el.opacity(0.35 + 0.65 * t).pt(px((1.0 - t) * 4.0)),
            )
            .into_any_element()
    } else {
        let days = calendar.days();
        div()
            .id("calendar-days-grid")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px_2p5()
            .pb_1p5()
            .overflow_y_scroll()
            .child(weekdays(false, cx))
            .child(render_grid(
                &days, false, calendar, details, entries, busy, cx,
            ))
            .with_animation(
                anim_key,
                Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                |el, t| el.opacity(0.35 + 0.65 * t).pt(px((1.0 - t) * 4.0)),
            )
            .into_any_element()
    };
    div()
        .id("calendar-panel")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(palette.surface_header)
        .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, window, cx| {
            if event.button == MouseButton::Left {
                this.finish_calendar_drag(window, cx);
            }
        }))
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.finish_calendar_drag(window, cx);
            }),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_2p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(14.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(palette.text)
                                .child(calendar.period_label()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .child(
                                    FlatButton::new("calendar-prev-period")
                                        .accessibility_label("上一时段")
                                        .disabled(busy)
                                        .w(px(26.))
                                        .h(px(26.))
                                        .rounded(px(6.))
                                        .hover(move |s| s.bg(palette.tab_active()))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(ph_icon(
                                            PhIcon::CaretLeft,
                                            px(14.),
                                            palette.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.cancel_calendar_drag(cx);
                                            if this.calendar.previous_period() {
                                                this.clear_calendar_range();
                                                this.reveal_calendar_year_month();
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    FlatButton::new("calendar-next-period")
                                        .accessibility_label("下一时段")
                                        .disabled(busy)
                                        .w(px(26.))
                                        .h(px(26.))
                                        .rounded(px(6.))
                                        .hover(move |s| s.bg(palette.tab_active()))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(ph_icon(
                                            PhIcon::CaretRight,
                                            px(14.),
                                            palette.text_muted,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.cancel_calendar_drag(cx);
                                            if this.calendar.next_period() {
                                                this.clear_calendar_range();
                                                this.reveal_calendar_year_month();
                                            }
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            FlatButton::new("calendar-today")
                                .disabled(busy)
                                .h(px(26.))
                                .px_2p5()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .hover(move |s| s.border_color(palette.accent_border))
                                .text_size(px(12.))
                                .text_color(palette.text_body)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("今天")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_calendar_drag(cx);
                                    let mut selection = this.calendar.clone();
                                    selection.show_today(chrono::Local::now().date_naive());
                                    this.open_calendar_selection(selection, window, cx);
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .p_0p5()
                                .rounded(px(6.))
                                .children(
                                    [
                                        (CalendarView::Week, "week", "周"),
                                        (CalendarView::Month, "month", "月"),
                                        (CalendarView::Year, "year", "年"),
                                    ]
                                    .into_iter()
                                    .map(|(view, id, label)| {
                                        let active = calendar.view() == view;
                                        FlatButton::new(format!("calendar-view-{id}"))
                                            .disabled(busy)
                                            .selected(active)
                                            .h(px(26.))
                                            .px_2()
                                            .rounded(px(6.))
                                            .bg(if active {
                                                palette.tab_active()
                                            } else {
                                                palette.surface_header
                                            })
                                            .hover(move |s| s.bg(palette.tab_active()))
                                            .text_size(px(12.5))
                                            .text_color(if active {
                                                palette.accent
                                            } else {
                                                palette.text_body
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.cancel_calendar_drag(cx);
                                                this.calendar.set_view(view);
                                                this.reveal_calendar_year_month();
                                                cx.notify();
                                            }))
                                    }),
                                ),
                        ),
                ),
        )
        .child(body)
        .child(render_calendar_footer(calendar, details, cx))
        .into_any_element()
}

fn render_calendar_footer(
    calendar: &CalendarState,
    details: &CalendarDetails,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let palette = crate::theme_skin::active_palette(cx);
    let year = calendar.display_date().year();
    div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(palette.border)
        .px_3()
        .py_1p5()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(11.5))
                .text_color(palette.text_muted)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .w(px(7.))
                                .h(px(7.))
                                .rounded(px(99.))
                                .bg(palette.accent),
                        )
                        .child("已记录日报")
                        .when(cfg!(test), |row| {
                            row.child(
                                FlatButton::new("calendar-sync-holidays")
                                    .px_1()
                                    .rounded(px(3.))
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .hover(move |s| s.text_color(palette.accent))
                                    .disabled(
                                        details.holiday_syncing
                                            || details.holiday_source.is_none()
                                            || !(2007..=2100).contains(&year),
                                    )
                                    .child(if details.holiday_syncing {
                                        "· 同步节假日中…".to_string()
                                    } else {
                                        format!("· 同步{year}节假日")
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.sync_calendar_holidays(window, cx)
                                    })),
                            )
                            .child(
                                FlatButton::new("calendar-reload-holidays")
                                    .px_1()
                                    .rounded(px(3.))
                                    .text_size(px(11.))
                                    .text_color(palette.text_muted)
                                    .hover(move |s| s.text_color(palette.accent))
                                    .disabled(
                                        details.holiday_loading
                                            || details.holiday_syncing
                                            || details.holiday_source.is_none(),
                                    )
                                    .child("读取本地")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.load_calendar_holidays(window, cx)
                                    })),
                            )
                        }),
                )
                .child(
                    div()
                        .text_color(if calendar.is_range() {
                            palette.accent
                        } else {
                            palette.text_muted
                        })
                        .child(if calendar.is_range() {
                            let (start, end) = calendar.selected_range();
                            format!("已选择 {} 天 · Esc 退出", (end - start).num_days() + 1)
                        } else {
                            "点击打开 · 拖拽框选".to_string()
                        }),
                ),
        )
        .when_some(details.holiday_error.as_ref(), |panel, error| {
            panel.child(
                div()
                    .id("calendar-holiday-error")
                    .max_h(px(48.))
                    .overflow_y_scroll()
                    .text_size(px(11.))
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            )
        })
}

pub(super) fn render_range(
    range: &WorkspaceRange,
    source: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let palette = crate::theme_skin::active_palette(cx);
    let total_chars: usize = range
        .documents
        .iter()
        .map(|doc| doc.content.chars().filter(|ch| !ch.is_whitespace()).count())
        .sum();
    let (start, end) = (range.start, range.end);
    let day_count = (end - start).num_days().unsigned_abs() + 1;

    let documents = range.documents.iter().map(|document| {
        let element = match document.format {
            Format::Md => markdown_preview(
                format!("range-md-{}", document.date),
                document.content.clone().into(),
            )
            .into_any_element(),
            Format::Txt => SelectableText::new(
                format!("range-txt-{}", document.date),
                document.content.clone(),
            )
            .into_any_element(),
        };
        let date_heading = chrono::NaiveDate::parse_from_str(&document.date, "%Y%m%d")
            .ok()
            .map(|d| {
                let weekdays = [
                    "星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日",
                ];
                let wd = weekdays[d.weekday().num_days_from_monday() as usize];
                format!("{} 年 {} 月 {} 日 {}", d.year(), d.month(), d.day(), wd)
            })
            .unwrap_or_else(|| document.date.clone());

        div()
            .flex()
            .flex_col()
            .gap_2()
            .pb_4()
            .border_b_1()
            .border_color(palette.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child(date_heading),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(palette.text_muted)
                            .child(document.filename.clone()),
                    ),
            )
            .child(element)
    });

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(palette.surface)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .px_5()
                .pt_4()
                .pb_2p5()
                .border_b_1()
                .border_color(palette.border)
                .child(
                    div()
                        .text_size(px(17.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text)
                        .child(range.label()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            FlatButton::new("range-preview")
                                .selected(!source)
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(if !source {
                                    palette.accent_border
                                } else {
                                    palette.border
                                })
                                .bg(if !source {
                                    palette.accent_soft
                                } else {
                                    palette.surface
                                })
                                .text_size(px(11.5))
                                .text_color(if !source {
                                    palette.accent
                                } else {
                                    palette.text_body
                                })
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(ph_icon(
                                    PhIcon::Columns,
                                    px(13.),
                                    if !source {
                                        palette.accent
                                    } else {
                                        palette.text_muted
                                    },
                                ))
                                .child("预览")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.range_source = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            FlatButton::new("range-source")
                                .selected(source)
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(if source {
                                    palette.accent_border
                                } else {
                                    palette.border
                                })
                                .bg(if source {
                                    palette.accent_soft
                                } else {
                                    palette.surface
                                })
                                .text_size(px(11.5))
                                .text_color(if source {
                                    palette.accent
                                } else {
                                    palette.text_body
                                })
                                .flex()
                                .items_center()
                                .child("原文")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.range_source = true;
                                    cx.notify();
                                })),
                        )
                        .child(
                            FlatButton::new("copy-range")
                                .accessibility_label("复制")
                                .disabled(range.documents.is_empty())
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .hover(move |s| s.border_color(palette.accent_border))
                                .text_size(px(11.5))
                                .text_color(palette.text_body)
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(ph_icon(PhIcon::Copy, px(13.), palette.text_muted))
                                .child("复制")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(range) = &this.workspace_range {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            range.source.clone(),
                                        ));
                                        this.status = "已复制所选日期的原文".into();
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            FlatButton::new("close-range")
                                .h(px(26.))
                                .px_2()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.surface)
                                .hover(move |s| s.border_color(palette.accent_border))
                                .text_size(px(11.5))
                                .text_color(palette.text_muted)
                                .flex()
                                .items_center()
                                .child("退出范围")
                                .on_click(cx.listener(|_, _, window, cx| {
                                    window.dispatch_action(Box::new(crate::ClearRange), cx);
                                })),
                        ),
                ),
        )
        .child(
            div()
                .id("range-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_5()
                .py_4()
                .flex()
                .flex_col()
                .gap_4()
                .when(range.documents.is_empty(), |body| {
                    body.child(
                        div()
                            .text_size(px(13.))
                            .text_color(palette.text_muted)
                            .child("所选日期中没有可显示的日报。"),
                    )
                })
                .when(source && !range.documents.is_empty(), |body| {
                    body.child(SelectableText::new(
                        "range-source-text",
                        range.source.clone(),
                    ))
                })
                .when(!source, |body| body.children(documents)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(25.))
                .px_3()
                .bg(palette.surface_header)
                .border_t_1()
                .border_color(palette.border)
                .text_size(px(11.5))
                .text_color(palette.text_muted)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(ph_icon(PhIcon::FileText, px(13.), palette.text_muted))
                        .child(format!(
                            "已选择 {} 天 · {} 篇日报",
                            day_count,
                            range.documents.len()
                        )),
                )
                .child(format!("{total_chars} 字")),
        )
}

impl Prototype {
    pub(super) fn reveal_calendar_year_month(&self) {
        if self.calendar.view() == CalendarView::Year {
            let row = (self.calendar.display_date().month() - 1) / 2;
            // Fixed row geometry positions the initial view before layout.
            // ScrollHandle::scroll_to_top_of_item uses its previous viewport
            // bounds, which are still empty on the first year frame.
            self.calendar_year_scroll.set_offset(point(
                px(0.),
                px(-(row as f32) * (YEAR_ROW_HEIGHT + YEAR_ROW_GAP)),
            ));
        }
    }

    pub(super) fn cancel_calendar_drag(&mut self, cx: &mut Context<Self>) {
        if self.calendar_drag.take().is_some() {
            self.calendar_suppress_click = true;
            cx.notify();
        }
    }

    pub(super) fn clear_calendar_range(&mut self) {
        let date = self
            .range_single_date
            .take()
            .unwrap_or(self.calendar.selected_date());
        self.calendar.select(date, false);
        self.workspace_range = None;
    }

    fn begin_calendar_drag(
        &mut self,
        date: NaiveDate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy || self.workspace_edit.is_some() || !self.viewing_workspace {
            return;
        }
        let mut selection = self.calendar.clone();
        selection.select(date, false);
        self.calendar_drag = Some(CalendarDrag {
            selection,
            visited_other_day: false,
        });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn move_calendar_drag(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.calendar_drag else {
            return;
        };
        drag.visited_other_day |= date != drag.selection.selected_date();
        drag.selection.select(date, true);
        cx.notify();
    }

    fn finish_calendar_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.calendar_drag.take() else {
            return;
        };
        cx.notify();
        if drag.visited_other_day {
            self.calendar_suppress_click = true;
        }
        if drag.selection.is_range() {
            self.calendar_suppress_click = true;
            self.open_calendar_selection(drag.selection, window, cx);
        }
    }

    pub(super) fn select_calendar_date(
        &mut self,
        date: NaiveDate,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut selection = self.calendar.clone();
        if !extend
            && (selection.view() != CalendarView::Month
                || selection
                    .days()
                    .iter()
                    .any(|day| day.date == date && day.in_month))
        {
            selection.show_date(date);
        } else {
            selection.select(date, extend);
        }
        self.open_calendar_selection(selection, window, cx);
    }

    fn open_calendar_selection(
        &mut self,
        selection: CalendarState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_busy || self.workspace_edit.is_some() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        let (start, end) = selection.selected_range();
        if !selection.is_range() {
            let date = start;
            let key = date.format("%Y%m%d").to_string();
            match workspace.entry_format(&key) {
                Some(format) => self.open_journal_with_calendar(key, format, selection, window, cx),
                None => {
                    self.new_journal_date.update(cx, |input, cx| {
                        input.set_value(date.format("%Y-%m-%d").to_string(), window, cx)
                    });
                    self.open_calendar_journal_editor(
                        key,
                        self.new_journal_format,
                        None,
                        selection,
                        window,
                        cx,
                    );
                }
            }
            return;
        }
        let reader = workspace.reader.clone();
        let entries = workspace.entries.clone();
        self.workspace_busy = true;
        self.status = "正在读取日期范围…".into();
        cx.notify();
        let load = cx
            .background_executor()
            .spawn(async move { WorkspaceRange::read(&reader, &entries, start, end) });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                this.workspace_busy = false;
                this.status = match result {
                    Ok(range) => {
                        if this.workspace_range.is_none() {
                            this.range_single_date = Some(this.calendar.selected_date());
                        }
                        this.calendar = selection;
                        this.workspace_range = Some(range);
                        this.range_source = false;
                        SharedString::from("日期范围只读预览")
                    }
                    Err(error) => format!("无法读取日期范围：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }
}
