use chrono::{Datelike, Days, Duration, Months, NaiveDate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CalendarDay {
    pub(crate) date: NaiveDate,
    /// All dates in a week belong to the displayed period. Month grids mark
    /// neighboring dates as outside, including the grids used by a year view.
    pub(crate) in_month: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalendarView {
    Week,
    Month,
    Year,
}

/// Period navigation and an inclusive selection, independent of stored journals.
#[derive(Clone)]
pub(crate) struct CalendarState {
    current: NaiveDate,
    view: CalendarView,
    anchor: NaiveDate,
    end: NaiveDate,
}

impl CalendarState {
    pub(crate) fn new(today: NaiveDate) -> Self {
        Self {
            current: today,
            view: CalendarView::Month,
            anchor: today,
            end: today,
        }
    }

    pub(crate) fn month_label(&self) -> String {
        self.current.format("%Y 年 %-m 月").to_string()
    }

    pub(crate) fn view(&self) -> CalendarView {
        self.view
    }

    pub(crate) fn display_date(&self) -> NaiveDate {
        self.current
    }

    /// View changes retain the displayed date and the entire selection.
    pub(crate) fn set_view(&mut self, view: CalendarView) {
        self.view = view;
    }

    pub(crate) fn period_label(&self) -> String {
        match self.view {
            CalendarView::Year => self.current.format("%Y 年").to_string(),
            CalendarView::Week | CalendarView::Month => self.month_label(),
        }
    }

    pub(crate) fn days(&self) -> Vec<CalendarDay> {
        if self.view != CalendarView::Week {
            return Self::month_days(self.current);
        }
        let leading = self.current.weekday().num_days_from_monday();
        (0..7)
            .filter_map(|index| {
                let offset = i64::from(index) - i64::from(leading);
                let date = self.current.checked_add_signed(Duration::days(offset))?;
                Some(CalendarDay {
                    date,
                    in_month: true,
                })
            })
            .collect()
    }

    /// A Monday-first month grid, with only representable neighboring dates.
    pub(crate) fn month_days(month: NaiveDate) -> Vec<CalendarDay> {
        let month = month_start(month);
        let leading = month.weekday().num_days_from_monday();
        let last = month
            .checked_add_months(Months::new(1))
            .and_then(|month| month.pred_opt())
            .unwrap_or(NaiveDate::MAX);
        let count = (leading + last.day()).div_ceil(7) * 7;

        (0..count)
            .filter_map(|index| {
                let offset = i64::from(index) - i64::from(leading);
                let date = month.checked_add_signed(Duration::days(offset))?;
                Some(CalendarDay {
                    date,
                    in_month: date.year() == month.year() && date.month() == month.month(),
                })
            })
            .collect()
    }

    pub(crate) fn previous_month(&mut self) -> bool {
        self.move_to(self.current.checked_sub_months(Months::new(1)))
    }

    pub(crate) fn next_month(&mut self) -> bool {
        self.move_to(self.current.checked_add_months(Months::new(1)))
    }

    pub(crate) fn previous_period(&mut self) -> bool {
        let date = match self.view {
            CalendarView::Week => self.current.checked_sub_days(Days::new(7)),
            CalendarView::Month => return self.previous_month(),
            CalendarView::Year => self.current.checked_sub_months(Months::new(12)),
        };
        self.move_to(date)
    }

    pub(crate) fn next_period(&mut self) -> bool {
        let date = match self.view {
            CalendarView::Week => self.current.checked_add_days(Days::new(7)),
            CalendarView::Month => return self.next_month(),
            CalendarView::Year => self.current.checked_add_months(Months::new(12)),
        };
        self.move_to(date)
    }

    fn move_to(&mut self, date: Option<NaiveDate>) -> bool {
        let Some(date) = date else {
            return false;
        };
        self.current = date;
        self.clear_range();
        true
    }

    pub(crate) fn months(&self) -> Vec<NaiveDate> {
        (1..=12)
            .filter_map(|month| NaiveDate::from_ymd_opt(self.current.year(), month, 1))
            .collect()
    }

    /// Opening a miniature month retains the single day or range selection.
    pub(crate) fn open_month(&mut self, month: NaiveDate) {
        self.current = month_start(month);
        self.view = CalendarView::Month;
    }

    pub(crate) fn show_today(&mut self, today: NaiveDate) {
        self.show_date(today);
    }

    /// An external selection reveals its period without changing the view mode.
    pub(crate) fn show_date(&mut self, date: NaiveDate) {
        self.current = date;
        self.select(date, false);
    }

    /// Clicking a day leaves the displayed period in place, including neighbors.
    pub(crate) fn select(&mut self, date: NaiveDate, extend: bool) {
        if !extend {
            self.anchor = date;
        }
        self.end = date;
    }

    pub(crate) fn clear_range(&mut self) {
        self.end = self.anchor;
    }

    pub(crate) fn selected_date(&self) -> NaiveDate {
        self.anchor
    }

    pub(crate) fn selected_range(&self) -> (NaiveDate, NaiveDate) {
        (self.anchor.min(self.end), self.anchor.max(self.end))
    }

    pub(crate) fn is_range(&self) -> bool {
        self.anchor != self.end
    }

    pub(crate) fn is_selected(&self, date: NaiveDate) -> bool {
        let (start, end) = self.selected_range();
        (start..=end).contains(&date)
    }
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(u64::from(date.day() - 1)))
        .unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn month_grid_starts_monday_and_includes_complete_neighboring_weeks() {
        let calendar = CalendarState::new(date(2026, 7, 15));
        let days = calendar.days();
        assert_eq!(days.len(), 35);
        assert_eq!(days.first().unwrap().date, date(2026, 6, 29));
        assert_eq!(days.last().unwrap().date, date(2026, 8, 2));
        assert_eq!(days.first().unwrap().date.weekday(), Weekday::Mon);
        assert_eq!(days.iter().filter(|day| day.in_month).count(), 31);

        let february = CalendarState::new(date(2027, 2, 15));
        assert_eq!(february.days().len(), 28);
        let leap_year = CalendarState::new(date(2028, 2, 15));
        assert!(
            leap_year
                .days()
                .iter()
                .any(|day| day.date == date(2028, 2, 29) && day.in_month)
        );
    }

    #[test]
    fn reverse_and_cross_year_ranges_are_inclusive_and_keep_the_anchor() {
        let mut calendar = CalendarState::new(date(2027, 1, 2));
        calendar.select(date(2026, 12, 30), true);
        assert_eq!(
            calendar.selected_range(),
            (date(2026, 12, 30), date(2027, 1, 2))
        );
        assert!(calendar.is_range());
        assert!(calendar.is_selected(date(2026, 12, 31)));
        assert!(calendar.is_selected(date(2027, 1, 1)));
        assert!(!calendar.is_selected(date(2027, 1, 3)));
        assert_eq!(calendar.selected_date(), date(2027, 1, 2));

        calendar.select(date(2027, 1, 4), true);
        assert_eq!(
            calendar.selected_range(),
            (date(2027, 1, 2), date(2027, 1, 4))
        );
        calendar.select(date(2027, 1, 5), false);
        assert_eq!(
            calendar.selected_range(),
            (date(2027, 1, 5), date(2027, 1, 5))
        );
        assert!(!calendar.is_range());
    }

    #[test]
    fn month_navigation_clears_the_range_without_changing_the_selected_day() {
        let mut calendar = CalendarState::new(date(2026, 12, 31));
        calendar.select(date(2027, 1, 2), true);
        assert!(calendar.next_month());
        assert_eq!(calendar.month_label(), "2027 年 1 月");
        assert_eq!(
            calendar.selected_range(),
            (date(2026, 12, 31), date(2026, 12, 31))
        );
        assert!(calendar.previous_month());
        assert_eq!(calendar.month_label(), "2026 年 12 月");

        calendar.select(date(2027, 1, 2), false);
        assert_eq!(calendar.month_label(), "2026 年 12 月");
        calendar.show_date(date(2027, 1, 2));
        assert_eq!(calendar.month_label(), "2027 年 1 月");
        calendar.show_today(date(2026, 11, 20));
        assert_eq!(calendar.month_label(), "2026 年 11 月");
        assert_eq!(calendar.selected_date(), date(2026, 11, 20));
    }

    #[test]
    fn week_grid_and_period_navigation_cross_the_year_on_monday() {
        let mut calendar = CalendarState::new(date(2027, 1, 3));
        calendar.set_view(CalendarView::Week);
        let days = calendar.days();
        assert_eq!(days.len(), 7);
        assert_eq!(days[0].date, date(2026, 12, 28));
        assert_eq!(days[6].date, date(2027, 1, 3));
        assert!(days.iter().all(|day| day.in_month));
        assert_eq!(calendar.period_label(), "2027 年 1 月");

        calendar.select(date(2026, 12, 30), true);
        assert!(calendar.previous_period());
        assert_eq!(calendar.days()[0].date, date(2026, 12, 21));
        assert_eq!(calendar.selected_date(), date(2027, 1, 3));
        assert!(!calendar.is_range());
        assert!(calendar.next_period());
        assert!(calendar.next_period());
        assert_eq!(calendar.days()[0].date, date(2027, 1, 4));
        assert_eq!(calendar.days()[6].date, date(2027, 1, 10));
    }

    #[test]
    fn view_changes_and_opening_a_month_preserve_the_entire_selection() {
        let mut calendar = CalendarState::new(date(2028, 2, 29));
        calendar.select(date(2028, 3, 4), true);
        let selection = calendar.selected_range();
        for view in [CalendarView::Week, CalendarView::Year, CalendarView::Month] {
            calendar.set_view(view);
            assert_eq!(calendar.view(), view);
            assert_eq!(calendar.selected_range(), selection);
            assert_eq!(calendar.month_label(), "2028 年 2 月");
        }

        calendar.set_view(CalendarView::Year);
        assert_eq!(calendar.period_label(), "2028 年");
        let months = calendar.months();
        assert_eq!(months.len(), 12);
        assert_eq!(months[0], date(2028, 1, 1));
        assert_eq!(months[11], date(2028, 12, 1));
        for month in &months {
            let days = CalendarState::month_days(*month);
            assert_eq!(days[0].date.weekday(), Weekday::Mon);
            assert_eq!(days.len() % 7, 0);
            assert!(days.iter().any(|day| day.date == *month && day.in_month));
        }
        assert_eq!(
            CalendarState::month_days(months[1])
                .iter()
                .filter(|day| day.in_month)
                .count(),
            29
        );

        calendar.open_month(date(2028, 8, 20));
        assert_eq!(calendar.view(), CalendarView::Month);
        assert_eq!(calendar.month_label(), "2028 年 8 月");
        assert_eq!(calendar.selected_range(), selection);
        assert_eq!(calendar.days()[0].date, date(2028, 7, 31));
    }

    #[test]
    fn month_and_year_navigation_clamp_short_months_without_changing_selection() {
        let mut calendar = CalendarState::new(date(2027, 1, 31));
        calendar.select(date(2027, 2, 2), true);
        assert!(calendar.next_period());
        assert_eq!(calendar.period_label(), "2027 年 2 月");
        assert!(!calendar.is_range());
        assert_eq!(calendar.selected_date(), date(2027, 1, 31));
        calendar.set_view(CalendarView::Week);
        assert_eq!(calendar.days()[6].date, date(2027, 2, 28));

        calendar.show_date(date(2028, 2, 29));
        calendar.set_view(CalendarView::Year);
        calendar.select(date(2028, 3, 2), true);
        assert!(calendar.next_period());
        assert_eq!(calendar.period_label(), "2029 年");
        assert!(!calendar.is_range());
        assert_eq!(calendar.selected_date(), date(2028, 2, 29));
        calendar.set_view(CalendarView::Week);
        assert_eq!(calendar.days()[2].date, date(2029, 2, 28));

        calendar.set_view(CalendarView::Year);
        assert!(calendar.previous_period());
        assert_eq!(calendar.period_label(), "2028 年");
        assert_eq!(calendar.selected_date(), date(2028, 2, 29));
    }

    #[test]
    fn external_dates_reveal_their_period_but_day_clicks_keep_it_in_every_view() {
        let mut calendar = CalendarState::new(date(2026, 7, 15));
        for view in [CalendarView::Week, CalendarView::Month, CalendarView::Year] {
            calendar.set_view(view);
            calendar.show_date(date(2027, 1, 3));
            assert_eq!(calendar.view(), view);
            assert_eq!(calendar.selected_date(), date(2027, 1, 3));
            let displayed_days = calendar.days();
            calendar.select(date(2026, 12, 30), false);
            assert_eq!(calendar.days(), displayed_days);
            assert_eq!(calendar.month_label(), "2027 年 1 月");

            calendar.show_today(date(2028, 2, 29));
            assert_eq!(calendar.view(), view);
            assert_eq!(calendar.month_label(), "2028 年 2 月");
            assert_eq!(calendar.selected_date(), date(2028, 2, 29));
            assert!(!calendar.is_range());
        }
    }

    #[test]
    fn chrono_limits_do_not_overflow_grid_or_navigation() {
        let mut first = CalendarState::new(NaiveDate::MIN);
        assert!(!first.previous_month());
        assert!(first.days().iter().any(|day| day.date == NaiveDate::MIN));
        let mut last = CalendarState::new(NaiveDate::MAX);
        assert!(!last.next_month());
        assert!(last.days().iter().any(|day| day.date == NaiveDate::MAX));
    }

    #[test]
    fn every_period_handles_chrono_limits_and_failed_moves_preserve_selection() {
        for view in [CalendarView::Week, CalendarView::Month, CalendarView::Year] {
            let mut first = CalendarState::new(NaiveDate::MIN);
            first.set_view(view);
            first.select(NaiveDate::MIN.succ_opt().unwrap(), true);
            let first_selection = first.selected_range();
            assert!(!first.previous_period());
            assert_eq!(first.selected_range(), first_selection);
            assert!(first.days().iter().any(|day| day.date == NaiveDate::MIN));
            assert_eq!(first.months().len(), 12);
            for month in first.months() {
                assert!(!CalendarState::month_days(month).is_empty());
            }

            let mut last = CalendarState::new(NaiveDate::MAX);
            last.set_view(view);
            last.select(NaiveDate::MAX.pred_opt().unwrap(), true);
            let last_selection = last.selected_range();
            assert!(!last.next_period());
            assert_eq!(last.selected_range(), last_selection);
            assert!(last.days().iter().any(|day| day.date == NaiveDate::MAX));
            assert_eq!(last.months().len(), 12);
            for month in last.months() {
                assert!(!CalendarState::month_days(month).is_empty());
            }
        }
    }
}
