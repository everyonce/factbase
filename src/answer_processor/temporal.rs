//! Temporal tag extraction and formatting for answer processing.

use crate::patterns::{MONTH_NAME_REGEX, YEAR_REGEX};

/// Extracted date information from an answer
#[derive(Debug, Default)]
pub(crate) struct DateInfo {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub is_ongoing: bool,
}

/// Extract dates from answer text
pub(crate) fn extract_dates_from_answer(answer: &str) -> Option<DateInfo> {
    let answer_lower = answer.to_lowercase();
    let mut info = DateInfo::default();

    // Check for "still" or "current" indicating ongoing
    if answer_lower.contains("still")
        || answer_lower.contains("current")
        || answer_lower.contains("yes")
    {
        info.is_ongoing = true;
    }

    // Check for "no" or "left" or "ended" indicating end date
    let has_end_indicator = answer_lower.contains("no,")
        || answer_lower.contains("left")
        || answer_lower.contains("ended")
        || answer_lower.contains("until");

    // Extract month-year patterns first (more specific)
    for cap in MONTH_NAME_REGEX.captures_iter(answer) {
        let month_name = &cap[1];
        let year = &cap[2];
        let month_num = month_name_to_number(month_name);
        let date = format!("{}-{:02}", year, month_num);

        if has_end_indicator && info.end_date.is_none() {
            info.end_date = Some(date);
        } else if answer_lower.contains("started") || answer_lower.contains("from") {
            if info.start_date.is_none() {
                info.start_date = Some(date);
            } else if info.end_date.is_none() {
                info.end_date = Some(date);
            }
        } else if info.end_date.is_none() {
            info.end_date = Some(date);
        }
    }

    // If no month-year found, try year-only
    if info.start_date.is_none() && info.end_date.is_none() {
        let years: Vec<_> = YEAR_REGEX.find_iter(answer).map(|m| m.as_str()).collect();

        if years.len() == 1 {
            if has_end_indicator {
                info.end_date = Some(years[0].to_string());
            } else {
                info.start_date = Some(years[0].to_string());
            }
        } else if years.len() >= 2 {
            info.start_date = Some(years[0].to_string());
            info.end_date = Some(years[1].to_string());
        }
    }

    if info.start_date.is_some() || info.end_date.is_some() || info.is_ongoing {
        Some(info)
    } else {
        None
    }
}

/// Convert month name to number
fn month_name_to_number(name: &str) -> u32 {
    match name.to_lowercase().as_str() {
        "january" => 1,
        "february" => 2,
        "march" => 3,
        "april" => 4,
        "may" => 5,
        "june" => 6,
        "july" => 7,
        "august" => 8,
        "september" => 9,
        "october" => 10,
        "november" => 11,
        "december" => 12,
        _ => 1,
    }
}

/// Format a new temporal tag from date info
pub(crate) fn format_new_temporal_tag(dates: &DateInfo) -> String {
    match (&dates.start_date, &dates.end_date, dates.is_ongoing) {
        (Some(start), Some(end), _) => format!("@t[{}..{}]", start, end),
        (Some(start), None, true) => format!("@t[{}..]", start),
        (Some(start), None, false) => format!("@t[={}]", start),
        (None, Some(end), _) => format!("@t[..{}]", end),
        _ => "@t[?]".to_string(),
    }
}

/// Format updated temporal tag based on old tag and new dates
pub(crate) fn format_temporal_tag(dates: &DateInfo, old_tag: &str) -> String {
    // Parse old tag to understand its structure
    let old_content = old_tag
        .strip_prefix("@t[")
        .and_then(|s| s.strip_suffix("]"))
        .unwrap_or("");

    // If old tag is ongoing (ends with ..) and we have an end date, close it
    if old_content.ends_with("..") {
        if let Some(end) = &dates.end_date {
            let start = old_content.strip_suffix("..").unwrap_or("");
            return format!("@t[{}..{}]", start, end);
        }
    }

    // If we have both dates, create a range
    if let (Some(start), Some(end)) = (&dates.start_date, &dates.end_date) {
        return format!("@t[{}..{}]", start, end);
    }

    // If ongoing, keep or make it ongoing
    if dates.is_ongoing {
        if let Some(start) = &dates.start_date {
            return format!("@t[{}..]", start);
        }
        // Keep existing start if we're just confirming ongoing
        if old_content.contains("..") {
            return old_tag.to_string();
        }
    }

    // Default: return new tag based on available info
    format_new_temporal_tag(dates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dates_month_year() {
        let dates = extract_dates_from_answer("Left in March 2024").unwrap();
        assert_eq!(dates.end_date, Some("2024-03".to_string()));
    }

    #[test]
    fn test_extract_dates_year_only() {
        let dates = extract_dates_from_answer("Ended in 2023").unwrap();
        assert_eq!(dates.end_date, Some("2023".to_string()));
    }

    #[test]
    fn test_extract_dates_ongoing() {
        let dates = extract_dates_from_answer("Yes, still current").unwrap();
        assert!(dates.is_ongoing);
    }

    #[test]
    fn test_format_new_temporal_tag_range() {
        let dates = DateInfo {
            start_date: Some("2020".to_string()),
            end_date: Some("2022".to_string()),
            is_ongoing: false,
        };
        assert_eq!(format_new_temporal_tag(&dates), "@t[2020..2022]");
    }

    #[test]
    fn test_format_new_temporal_tag_ongoing() {
        let dates = DateInfo {
            start_date: Some("2020".to_string()),
            end_date: None,
            is_ongoing: true,
        };
        assert_eq!(format_new_temporal_tag(&dates), "@t[2020..]");
    }

    #[test]
    fn test_format_temporal_tag_close_ongoing() {
        let dates = DateInfo {
            start_date: None,
            end_date: Some("2024-03".to_string()),
            is_ongoing: false,
        };
        assert_eq!(
            format_temporal_tag(&dates, "@t[2022..]"),
            "@t[2022..2024-03]"
        );
    }
}
