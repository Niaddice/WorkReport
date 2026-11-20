use std::{sync::Arc, time::Duration};

use chrono::{Datelike, NaiveDate};
use gpui_kit::{Context, Task, Window};

use crate::{
    Prototype,
    calendar::CalendarView,
    calendar_holidays::{HolidayCalendar, HolidaySource},
    calendar_previews::CalendarPreviews,
};

#[derive(Clone, PartialEq, Eq)]
struct PreviewKey {
    workspace: usize,
    dates: Vec<String>,
    revision: u64,
}

pub(super) struct CalendarDetails {
    pub(super) holidays: HolidayCalendar,
    pub(super) holiday_source: Option<Arc<dyn HolidaySource>>,
    pub(super) holiday_loading: bool,
    pub(super) holiday_syncing: bool,
    pub(super) holiday_error: Option<String>,
    pub(super) previews: CalendarPreviews,
    pub(super) today: NaiveDate,
    holiday_request: u64,
    preview_key: Option<PreviewKey>,
    preview_request: u64,
    preview_revision: u64,
    preview_task: Option<Task<()>>,
}

impl Default for CalendarDetails {
    fn default() -> Self {
        Self {
            holidays: HolidayCalendar::from_state(Default::default()),
            holiday_source: None,
            holiday_loading: false,
            holiday_syncing: false,
            holiday_error: None,
            previews: CalendarPreviews::default(),
            today: chrono::Local::now().date_naive(),
            holiday_request: 0,
            preview_key: None,
            preview_request: 0,
            preview_revision: 0,
            preview_task: None,
        }
    }
}

impl CalendarDetails {
    pub(super) fn invalidate_previews(&mut self) {
        self.preview_revision += 1;
        self.preview_request += 1;
        self.preview_key = None;
        self.preview_task = None;
        self.previews = CalendarPreviews::default();
    }
}

impl Prototype {
    pub(super) fn ensure_calendar_previews(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.viewing_workspace || self.workspace_edit.is_some() {
            return;
        }
        let Some(workspace) = &self.workspace else {
            return;
        };
        let dates = if self.calendar.view() == CalendarView::Year {
            Vec::new()
        } else {
            self.calendar
                .days()
                .iter()
                .map(|day| day.date.format("%Y%m%d").to_string())
                .collect()
        };
        let key = PreviewKey {
            workspace: Arc::as_ptr(&workspace.reader) as usize,
            dates,
            revision: self.calendar_details.preview_revision,
        };
        if self.calendar_details.preview_key.as_ref() == Some(&key) {
            return;
        }
        self.calendar_details.preview_request += 1;
        let request = self.calendar_details.preview_request;
        self.calendar_details.preview_key = Some(key.clone());
        self.calendar_details.preview_task = None;
        self.calendar_details.previews = CalendarPreviews::default();
        if key.dates.is_empty() {
            return;
        }
        let reader = workspace.reader.clone();
        let entries = workspace.entries.clone();
        let dates = key.dates.clone();
        let load = cx
            .background_executor()
            .spawn(async move { CalendarPreviews::read(&reader, &entries, &dates) });
        self.calendar_details.preview_task = Some(cx.spawn_in(window, async move |this, cx| {
            let previews = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.calendar_details.preview_request == request
                    && this.calendar_details.preview_key.as_ref() == Some(&key)
                {
                    this.calendar_details.previews = previews;
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn load_calendar_holidays(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(source) = self.calendar_details.holiday_source.clone() else {
            return;
        };
        if self.calendar_details.holiday_syncing || self.calendar_details.holiday_loading {
            return;
        }
        self.calendar_details.holiday_request += 1;
        let request = self.calendar_details.holiday_request;
        self.calendar_details.holiday_loading = true;
        self.calendar_details.holiday_error = None;
        cx.notify();
        let load = cx.background_executor().spawn(async move { source.load() });
        cx.spawn_in(window, async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.calendar_details.holiday_request != request {
                    return;
                }
                this.calendar_details.holiday_loading = false;
                match result {
                    Ok(state) => {
                        this.calendar_details.holidays = HolidayCalendar::from_state(state)
                    }
                    Err(error) => this.calendar_details.holiday_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn sync_calendar_holidays(&mut self, window: &Window, cx: &mut Context<Self>) {
        let year = self.calendar.display_date().year();
        if self.calendar_details.holiday_syncing || !(2007..=2100).contains(&year) {
            return;
        }
        let Some(source) = self.calendar_details.holiday_source.clone() else {
            return;
        };
        self.cancel_calendar_drag(cx);
        self.calendar_details.holiday_request += 1;
        let request = self.calendar_details.holiday_request;
        self.calendar_details.holiday_loading = false;
        self.calendar_details.holiday_syncing = true;
        self.calendar_details.holiday_error = None;
        cx.notify();
        let sync = cx
            .background_executor()
            .spawn(async move { source.sync(year) });
        cx.spawn_in(window, async move |this, cx| {
            let update = sync.await;
            let _ = this.update(cx, |this, cx| {
                if this.calendar_details.holiday_request != request {
                    return;
                }
                this.calendar_details.holiday_syncing = false;
                if let Some(state) = update.state {
                    this.calendar_details.holidays = HolidayCalendar::from_state(state);
                }
                this.calendar_details.holiday_error = update.error;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn watch_calendar_date(window: &Window, cx: &Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        let today = chrono::Local::now().date_naive();
                        if this.calendar_details.today != today {
                            this.calendar_details.today = today;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }
}
