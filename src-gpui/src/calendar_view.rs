use chrono::{Datelike, NaiveDate};
use gpui_kit::{
    ClickEvent, ClipboardItem, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ScrollHandle, SharedString, Window,
    base::{Disableable, SelectableText},
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
    },
    div, point,
    prelude::*,
    px,
};
use workreport_core::workspace::{Entry, Format};

use crate::{
    Prototype,
    calendar::{CalendarDay, CalendarState, CalendarView},
    calendar_details::CalendarDetails,
    calendar_holidays::DayKind,
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
) -> Button {
    let date = day.date;
    let key = date.format("%Y%m%d").to_string();
    let recorded = entries.iter().any(|entry| entry.date == key);
    let selected = calendar.is_selected(date);
    let draggable = day.in_month;
    let lunar = lunar_label(date);
    let holiday = details.holidays.metadata(date);
    let summary = (!compact && recorded).then(|| match details.previews.values.get(&key) {
        Some(Ok(value)) => value.clone(),
        Some(Err(error)) => format!("无法读取摘要：{error}"),
        None => "正在读取摘要…".to_owned(),
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
    let secondary = if selected {
        cx.theme().primary_foreground
    } else {
        cx.theme().muted_foreground
    };
    let mut button = Button::new(format!("calendar-day-{key}"))
        .accessibility_label(label.clone())
        .tooltip(label)
        .toggled(selected)
        .when(selected, Button::primary)
        .when(!day.in_month && !selected, |button| {
            button.text_color(cx.theme().muted_foreground)
        })
        .when(
            day.in_month && details.holidays.is_weekend(date) && !selected,
            |button| button.text_color(cx.theme().red),
        )
        .when(recorded && compact && !selected, |button| {
            button
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().primary)
        })
        .when(compact && holiday.is_some() && !selected, |button| {
            button.bg(cx.theme().accent)
        })
        .when(date == details.today, |button| button.border_2())
        .disabled(busy)
        .flex_1()
        .min_w_0()
        .p_0()
        .text_size(px(if compact { 10. } else { 14. }))
        .h(px(if compact { 19. } else { 46. }));
    button = if compact {
        button.label(date.day().to_string())
    } else {
        button.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(format!("{}{}", date.day(), if recorded { "·" } else { "" }))
                        .when_some(holiday.as_ref(), |row, holiday| {
                            row.child(div().text_size(px(9.)).child(
                                if holiday.kind == DayKind::Holiday {
                                    "休"
                                } else {
                                    "班"
                                },
                            ))
                        }),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(secondary)
                        .child(lunar.unwrap_or_default()),
                ),
        )
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
    div()
        .flex()
        .gap_1()
        .children(
            ["一", "二", "三", "四", "五", "六", "日"]
                .into_iter()
                .map(|day| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_center()
                        .text_size(px(if compact { 10. } else { 14. }))
                        .text_color(cx.theme().muted_foreground)
                        .child(day)
                }),
        )
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
        .gap_1()
        .children(days.chunks(7).map(|week| {
            div().flex().gap_1().children(week.iter().map(|day| {
                if compact && !day.in_month {
                    // Year grids keep weekday alignment without duplicate date identities.
                    div().flex_1().min_w_0().h(px(19.)).into_any_element()
                } else {
                    render_day(*day, compact, calendar, details, entries, busy, cx)
                        .into_any_element()
                }
            }))
        }))
}

pub(super) fn render_calendar(
    presentation: &CalendarPresentation<'_>,
    entries: &[Entry],
    busy: bool,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let calendar = presentation.selection;
    let details = presentation.details;
    let year_scroll = presentation.year_scroll;
    let body = if calendar.view() == CalendarView::Year {
        let months = calendar.months();
        div()
            .id("calendar-year-grid")
            .flex()
            .flex_col()
            .gap(px(YEAR_ROW_GAP))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(year_scroll)
            .children(months.chunks(2).map(|row| {
                div()
                    .flex()
                    .flex_shrink_0()
                    .h(px(YEAR_ROW_HEIGHT))
                    .items_start()
                    .gap_2()
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
                                Button::new(format!("calendar-month-{}", month.format("%Y%m")))
                                    .label(format!("{} 月", month.month()))
                                    .disabled(busy)
                                    .w_full()
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
            .into_any_element()
    } else {
        let days = calendar.days();
        div()
            .id("calendar-days-grid")
            .flex()
            .flex_col()
            .gap_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(weekdays(false, cx))
            .child(render_grid(
                &days, false, calendar, details, entries, busy, cx,
            ))
            .into_any_element()
    };
    div()
        .id("calendar-panel")
        .flex()
        .flex_col()
        .min_h_0()
        .max_h(px(450.))
        .gap_1()
        .when(calendar.view() == CalendarView::Year, |panel| {
            panel.h(px(390.))
        })
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
                .items_center()
                .gap_1()
                .child(
                    Button::new("calendar-prev-period")
                        .label("‹")
                        .accessibility_label("上一时段")
                        .disabled(busy)
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
                    div()
                        .flex_1()
                        .text_sm()
                        .text_center()
                        .child(calendar.period_label()),
                )
                .child(
                    Button::new("calendar-next-period")
                        .label("›")
                        .accessibility_label("下一时段")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_calendar_drag(cx);
                            if this.calendar.next_period() {
                                this.clear_calendar_range();
                                this.reveal_calendar_year_month();
                            }
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .gap_1()
                .items_center()
                .child(
                    Button::new("calendar-today")
                        .label("今天")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_calendar_drag(cx);
                            let mut selection = this.calendar.clone();
                            selection.show_today(chrono::Local::now().date_naive());
                            this.open_calendar_selection(selection, window, cx);
                        })),
                )
                .children(
                    [
                        (CalendarView::Week, "week", "周"),
                        (CalendarView::Month, "month", "月"),
                        (CalendarView::Year, "year", "年"),
                    ]
                    .into_iter()
                    .map(|(view, id, label)| {
                        Button::new(format!("calendar-view-{id}"))
                            .label(label)
                            .disabled(busy)
                            .toggled(calendar.view() == view)
                            .when(calendar.view() == view, Button::primary)
                            .flex_1()
                            .min_w_0()
                            .p_0()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cancel_calendar_drag(cx);
                                this.calendar.set_view(view);
                                this.reveal_calendar_year_month();
                                cx.notify();
                            }))
                    }),
                ),
        )
        .child(body)
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(if calendar.is_range() {
                    let (start, end) = calendar.selected_range();
                    format!("已选择 {} 天 · Esc 退出", (end - start).num_days() + 1)
                } else {
                    "点击打开 · 拖动或 Shift 选择范围".to_string()
                }),
        )
        .child(render_holiday_controls(calendar, details, cx))
}

fn render_holiday_controls(
    calendar: &CalendarState,
    details: &CalendarDetails,
    cx: &Context<Prototype>,
) -> impl IntoElement {
    let year = calendar.display_date().year();
    let synced = details
        .holidays
        .state()
        .years
        .iter()
        .any(|item| item.year == year);
    let label = if details.holiday_loading {
        "正在读取放假信息"
    } else if synced {
        "已同步"
    } else if year == 2026 {
        "内置安排"
    } else {
        "尚未同步"
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{year} · {label}")),
                )
                .child(
                    Button::new("calendar-reload-holidays")
                        .label("读取本地")
                        .h(px(24.))
                        .px_1()
                        .text_size(px(11.))
                        .disabled(
                            details.holiday_loading
                                || details.holiday_syncing
                                || details.holiday_source.is_none(),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.load_calendar_holidays(window, cx)
                        })),
                )
                .child(
                    Button::new("calendar-sync-holidays")
                        .label(if details.holiday_syncing {
                            "同步中…".to_string()
                        } else {
                            format!("同步 {year}")
                        })
                        .h(px(24.))
                        .px_1()
                        .text_size(px(11.))
                        .disabled(
                            details.holiday_syncing
                                || details.holiday_source.is_none()
                                || !(2007..=2100).contains(&year),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.sync_calendar_holidays(window, cx)
                        })),
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
        div()
            .flex()
            .flex_col()
            .gap_2()
            .pb_4()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(document.date.clone()),
            )
            .child(element)
    });
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .items_center()
                .child(div().w_full().text_sm().child(format!(
                    "{} · {} 篇日报",
                    range.label(),
                    range.documents.len()
                )))
                .child(
                    Button::new("range-preview")
                        .label("预览")
                        .when(!source, Button::primary)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.range_source = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("range-source")
                        .label("原文")
                        .when(source, Button::primary)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.range_source = true;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("copy-range")
                        .label("复制")
                        .disabled(range.documents.is_empty())
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
                    Button::new("close-range")
                        .label("退出范围")
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::ClearRange), cx);
                        })),
                ),
        )
        .child(
            div()
                .id("range-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .p_3()
                .when(range.documents.is_empty(), |body| {
                    body.child("所选日期内暂无日报")
                })
                .when(source && !range.documents.is_empty(), |body| {
                    body.child(SelectableText::new(
                        "range-source-text",
                        range.source.clone(),
                    ))
                })
                .when(!source, |body| body.children(documents)),
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
