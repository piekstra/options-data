use chrono::{Datelike, NaiveDate, Weekday};

/// Returns all weekdays (Mon-Fri) in the inclusive date range [start, end].
pub fn weekdays(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let mut current = start;
    while current <= end {
        match current.weekday() {
            Weekday::Sat | Weekday::Sun => {}
            _ => dates.push(current),
        }
        current = current.succ_opt().unwrap_or(current);
        if current == start && current > end {
            break;
        }
    }
    dates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn weekdays_skips_weekends() {
        let result = weekdays(date(2025, 1, 13), date(2025, 1, 19));
        assert_eq!(
            result,
            vec![
                date(2025, 1, 13),
                date(2025, 1, 14),
                date(2025, 1, 15),
                date(2025, 1, 16),
                date(2025, 1, 17),
            ]
        );
    }

    #[test]
    fn weekdays_single_day_weekday() {
        let result = weekdays(date(2025, 1, 15), date(2025, 1, 15));
        assert_eq!(result, vec![date(2025, 1, 15)]);
    }

    #[test]
    fn weekdays_single_day_weekend() {
        let result = weekdays(date(2025, 1, 18), date(2025, 1, 18));
        assert!(result.is_empty());
    }

    #[test]
    fn weekdays_start_after_end() {
        let result = weekdays(date(2025, 1, 20), date(2025, 1, 15));
        assert!(result.is_empty());
    }
}
