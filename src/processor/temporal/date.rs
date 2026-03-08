//! Date validation and utility functions.

use crate::patterns::pad_negative_year;

/// Validate a date string for semantic correctness.
/// Accepts formats: YYYY, YYYY-QN, YYYY-MM, YYYY-MM-DD (with optional leading `-` for BCE).
/// Negative years may be 1-4 digits (e.g., `-330`, `-0330`); they are zero-padded internally.
/// Returns None if valid, Some(error_message) if invalid.
pub fn validate_date(date: &str) -> Option<String> {
    let date = pad_negative_year(date);
    let (is_negative, rest) = if let Some(stripped) = date.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, date.as_str())
    };

    if rest.len() == 4 {
        if let Ok(year) = rest.parse::<u32>() {
            if !is_negative && !(1900..=2100).contains(&year) {
                return Some(format!(
                    "year {year} is outside reasonable range (1900-2100)"
                ));
            }
            if is_negative && year == 0 {
                return Some("year -0000 is not valid".to_string());
            }
            return None;
        }
        return Some(format!("invalid year format: {date}"));
    }

    if rest.len() == 7 && rest.chars().nth(5) == Some('Q') {
        let year_str = &rest[0..4];
        let quarter_str = &rest[6..7];
        if let Ok(year) = year_str.parse::<u32>() {
            if !is_negative && !(1900..=2100).contains(&year) {
                return Some(format!(
                    "year {year} is outside reasonable range (1900-2100)"
                ));
            }
            if let Ok(quarter) = quarter_str.parse::<u32>() {
                if !(1..=4).contains(&quarter) {
                    return Some(format!("invalid quarter Q{quarter} (must be Q1-Q4)"));
                }
                return None;
            }
        }
        return Some(format!("invalid quarter format: {date}"));
    }

    if rest.len() == 7 {
        let year_str = &rest[0..4];
        let month_str = &rest[5..7];
        if let (Ok(year), Ok(month)) = (year_str.parse::<u32>(), month_str.parse::<u32>()) {
            if !is_negative && !(1900..=2100).contains(&year) {
                return Some(format!(
                    "year {year} is outside reasonable range (1900-2100)"
                ));
            }
            if !(1..=12).contains(&month) {
                return Some(format!("invalid month {month} (must be 01-12)"));
            }
            return None;
        }
        return Some(format!("invalid year-month format: {date}"));
    }

    if rest.len() == 10 {
        let year_str = &rest[0..4];
        let month_str = &rest[5..7];
        let day_str = &rest[8..10];
        if let (Ok(year), Ok(month), Ok(day)) = (
            year_str.parse::<u32>(),
            month_str.parse::<u32>(),
            day_str.parse::<u32>(),
        ) {
            if !is_negative && !(1900..=2100).contains(&year) {
                return Some(format!(
                    "year {year} is outside reasonable range (1900-2100)"
                ));
            }
            if !(1..=12).contains(&month) {
                return Some(format!("invalid month {month} (must be 01-12)"));
            }
            let max_day = days_in_month(year, month);
            if day < 1 || day > max_day {
                return Some(format!(
                    "invalid day {day} for month {month} (must be 01-{max_day})"
                ));
            }
            return None;
        }
        return Some(format!("invalid date format: {date}"));
    }

    Some(format!("unrecognized date format: {date}"))
}

pub(crate) fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub(crate) fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_date_year_valid() {
        assert!(validate_date("2024").is_none());
        assert!(validate_date("1900").is_none());
        assert!(validate_date("2100").is_none());
    }

    #[test]
    fn test_validate_date_year_out_of_range() {
        assert!(validate_date("1899").is_some());
        assert!(validate_date("2101").is_some());
    }

    #[test]
    fn test_validate_date_quarter_valid() {
        assert!(validate_date("2024-Q1").is_none());
        assert!(validate_date("2024-Q4").is_none());
    }

    #[test]
    fn test_validate_date_month_valid() {
        assert!(validate_date("2024-01").is_none());
        assert!(validate_date("2024-12").is_none());
    }

    #[test]
    fn test_validate_date_month_invalid() {
        assert!(validate_date("2024-00").is_some());
        assert!(validate_date("2024-13").is_some());
    }

    #[test]
    fn test_validate_date_day_valid() {
        assert!(validate_date("2024-01-01").is_none());
        assert!(validate_date("2024-02-29").is_none()); // Leap year
    }

    #[test]
    fn test_validate_date_day_invalid() {
        assert!(validate_date("2024-01-32").is_some());
        assert!(validate_date("2023-02-29").is_some()); // Not a leap year
    }

    #[test]
    fn test_validate_date_bce_year() {
        assert!(validate_date("-0031").is_none());
        assert!(validate_date("-0490").is_none());
        assert!(validate_date("-9999").is_none());
        assert!(validate_date("-0000").is_some()); // -0000 is not valid
    }

    #[test]
    fn test_validate_date_bce_month() {
        assert!(validate_date("-0490-03").is_none());
        assert!(validate_date("-0490-00").is_some());
        assert!(validate_date("-0490-13").is_some());
    }

    #[test]
    fn test_validate_date_bce_quarter() {
        assert!(validate_date("-0490-Q1").is_none());
        assert!(validate_date("-0490-Q4").is_none());
    }

    #[test]
    fn test_validate_date_bce_day() {
        assert!(validate_date("-0490-03-15").is_none());
        assert!(validate_date("-0490-02-30").is_some());
    }

    #[test]
    fn test_validate_date_unpadded_bce_year() {
        assert!(validate_date("-330").is_none());
        assert!(validate_date("-31").is_none());
        assert!(validate_date("-5").is_none());
    }

    #[test]
    fn test_validate_date_unpadded_bce_with_month() {
        assert!(validate_date("-490-03").is_none());
        assert!(validate_date("-31-06").is_none());
    }
}
