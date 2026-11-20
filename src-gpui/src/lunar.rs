use chinese_lunisolar_calendar::{ChineseVariant, LunarDay, SolarDate};
use chrono::NaiveDate;

/// Match the calendar's existing Chinese month/day labels without network I/O.
/// The pinned conversion library supports 1901-02-19 through 2101-01-28.
pub(crate) fn lunar_label(date: NaiveDate) -> Option<String> {
    let lunar = SolarDate::try_from(date).ok()?.to_lunisolar_date().ok()?;
    let day = lunar.to_lunar_day();
    let label = if day == LunarDay::First {
        lunar.to_lunar_month().to_str(ChineseVariant::Simple)
    } else {
        day.to_str()
    };
    Some(label.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    // These dates also match the existing Vue Intl Chinese-calendar labels.
    #[test]
    fn spring_festival_changes_from_the_last_day_to_the_first_month() {
        for (solar, label) in [
            ("2024-02-09", "三十"),
            ("2024-02-10", "正月"),
            ("2024-02-11", "初二"),
            ("2024-02-19", "初十"),
            ("2024-02-29", "二十"),
            ("2024-03-09", "廿九"),
        ] {
            assert_eq!(lunar_label(date(solar)).as_deref(), Some(label), "{solar}");
        }
    }

    #[test]
    fn leap_month_names_are_simplified_and_only_replace_the_first_day() {
        for (solar, label) in [
            ("2023-03-21", "三十"),
            ("2023-03-22", "闰二月"),
            ("2023-04-19", "廿九"),
            ("2023-04-20", "三月"),
            ("2025-07-24", "三十"),
            ("2025-07-25", "闰六月"),
            ("2025-07-26", "初二"),
        ] {
            assert_eq!(lunar_label(date(solar)).as_deref(), Some(label), "{solar}");
        }
    }

    #[test]
    fn the_lunar_month_continues_across_the_gregorian_year() {
        for (solar, label) in [
            ("2024-12-30", "三十"),
            ("2024-12-31", "腊月"),
            ("2025-01-01", "初二"),
        ] {
            assert_eq!(lunar_label(date(solar)).as_deref(), Some(label), "{solar}");
        }
    }

    #[test]
    fn supported_limits_are_inclusive_and_other_dates_have_no_label() {
        assert_eq!(lunar_label(date("1901-02-19")).as_deref(), Some("正月"));
        assert_eq!(lunar_label(date("2101-01-28")).as_deref(), Some("廿九"));
        for outside in [
            date("1901-02-18"),
            date("2101-01-29"),
            date("0000-01-01"),
            date("3000-01-01"),
            NaiveDate::MIN,
            NaiveDate::MAX,
        ] {
            assert_eq!(lunar_label(outside), None, "{outside}");
        }
    }
}
