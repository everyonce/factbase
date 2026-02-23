//! Date range operations and overlap detection.

use crate::models::{TemporalTag, TemporalTagType};
use crate::patterns::{normalize_date_for_comparison, normalize_date_to_end};

/// Check if a temporal tag overlaps with a specific point in time.
pub fn overlaps_point(tag: &TemporalTag, query_date: &str) -> bool {
    let query_norm = normalize_date_for_comparison(query_date);

    match tag.tag_type {
        TemporalTagType::Unknown => false,

        TemporalTagType::PointInTime => {
            if let Some(ref date) = tag.start_date {
                let date_norm = normalize_date_for_comparison(date);
                dates_match_at_granularity(&date_norm, &query_norm)
            } else {
                false
            }
        }

        TemporalTagType::LastSeen => {
            if let Some(ref date) = tag.start_date {
                let date_norm = normalize_date_for_comparison(date);
                query_norm <= date_norm
            } else {
                false
            }
        }

        TemporalTagType::Range => match (&tag.start_date, &tag.end_date) {
            (Some(start), Some(end)) => {
                let start_norm = normalize_date_for_comparison(start);
                let end_norm = normalize_date_to_end(end);
                query_norm >= start_norm && query_norm <= end_norm
            }
            _ => false,
        },

        TemporalTagType::Ongoing => {
            if let Some(ref start) = tag.start_date {
                let start_norm = normalize_date_for_comparison(start);
                query_norm >= start_norm
            } else {
                false
            }
        }

        TemporalTagType::Historical => {
            if let Some(ref end) = tag.end_date {
                let end_norm = normalize_date_to_end(end);
                query_norm <= end_norm
            } else {
                false
            }
        }
    }
}

/// Check if a temporal tag overlaps with a date range.
pub fn overlaps_range(tag: &TemporalTag, query_start: &str, query_end: &str) -> bool {
    let query_start_norm = normalize_date_for_comparison(query_start);
    let query_end_norm = normalize_date_to_end(query_end);

    match tag.tag_type {
        TemporalTagType::Unknown => false,

        TemporalTagType::PointInTime => {
            if let Some(ref date) = tag.start_date {
                let date_norm = normalize_date_for_comparison(date);
                date_norm >= query_start_norm && date_norm <= query_end_norm
            } else {
                false
            }
        }

        TemporalTagType::LastSeen => {
            if let Some(ref date) = tag.start_date {
                let date_norm = normalize_date_for_comparison(date);
                query_start_norm <= date_norm
            } else {
                false
            }
        }

        TemporalTagType::Range => match (&tag.start_date, &tag.end_date) {
            (Some(start), Some(end)) => {
                let tag_start_norm = normalize_date_for_comparison(start);
                let tag_end_norm = normalize_date_to_end(end);
                tag_start_norm <= query_end_norm && query_start_norm <= tag_end_norm
            }
            _ => false,
        },

        TemporalTagType::Ongoing => {
            if let Some(ref start) = tag.start_date {
                let start_norm = normalize_date_for_comparison(start);
                query_end_norm >= start_norm
            } else {
                false
            }
        }

        TemporalTagType::Historical => {
            if let Some(ref end) = tag.end_date {
                let end_norm = normalize_date_to_end(end);
                query_start_norm <= end_norm
            } else {
                false
            }
        }
    }
}

fn dates_match_at_granularity(date1: &str, date2: &str) -> bool {
    date1 == date2
}

/// Check if two date ranges overlap.
pub(crate) fn ranges_overlap(start1: &str, end1: &str, start2: &str, end2: &str) -> bool {
    let s1 = normalize_date_for_comparison(start1);
    let e1 = normalize_date_for_comparison(end1);
    let s2 = normalize_date_for_comparison(start2);
    let e2 = normalize_date_for_comparison(end2);
    s1 <= e2 && s2 <= e1
}

/// Calculate recency boost for a document based on its LastSeen temporal tags.
pub fn calculate_recency_boost(tags: &[TemporalTag], window_days: u32, boost_factor: f32) -> f32 {
    use chrono::{NaiveDate, Utc};

    if window_days == 0 || boost_factor <= 0.0 {
        return 1.0;
    }

    let today = Utc::now().date_naive();
    let mut best_boost = 1.0_f32;

    for tag in tags {
        if tag.tag_type != TemporalTagType::LastSeen {
            continue;
        }

        if let Some(ref date_str) = tag.start_date {
            let date_norm = normalize_date_for_comparison(date_str);
            if let Ok(tag_date) = NaiveDate::parse_from_str(&date_norm, "%Y-%m-%d") {
                let days_ago = (today - tag_date).num_days();
                if days_ago >= 0 && days_ago < window_days as i64 {
                    let days_remaining = window_days as f32 - days_ago as f32;
                    let boost = 1.0 + (days_remaining / window_days as f32) * boost_factor;
                    if boost > best_boost {
                        best_boost = boost;
                    }
                }
            }
        }
    }

    best_boost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlaps_point_range_within() {
        let tag = TemporalTag {
            tag_type: TemporalTagType::Range,
            start_date: Some("2020".to_string()),
            end_date: Some("2022".to_string()),
            line_number: 1,
            raw_text: "@t[2020..2022]".to_string(),
        };
        assert!(overlaps_point(&tag, "2021"));
        assert!(!overlaps_point(&tag, "2019"));
        assert!(!overlaps_point(&tag, "2023"));
    }

    #[test]
    fn test_overlaps_point_unknown_never_matches() {
        let tag = TemporalTag {
            tag_type: TemporalTagType::Unknown,
            start_date: None,
            end_date: None,
            line_number: 1,
            raw_text: "@t[?]".to_string(),
        };
        assert!(!overlaps_point(&tag, "2021"));
    }

    #[test]
    fn test_overlaps_range_range_overlap() {
        let tag = TemporalTag {
            tag_type: TemporalTagType::Range,
            start_date: Some("2020".to_string()),
            end_date: Some("2022".to_string()),
            line_number: 1,
            raw_text: "@t[2020..2022]".to_string(),
        };
        assert!(overlaps_range(&tag, "2021", "2023"));
        assert!(!overlaps_range(&tag, "2023", "2025"));
    }

    #[test]
    fn test_recency_boost_no_last_seen_tags() {
        let tags = vec![TemporalTag {
            tag_type: TemporalTagType::Range,
            start_date: Some("2020".to_string()),
            end_date: Some("2022".to_string()),
            line_number: 1,
            raw_text: "@t[2020..2022]".to_string(),
        }];
        assert_eq!(calculate_recency_boost(&tags, 180, 0.2), 1.0);
    }

    #[test]
    fn test_recency_boost_empty_tags() {
        let tags: Vec<TemporalTag> = vec![];
        assert_eq!(calculate_recency_boost(&tags, 180, 0.2), 1.0);
    }

    #[test]
    fn test_recency_boost_zero_window() {
        let tags = vec![TemporalTag {
            tag_type: TemporalTagType::LastSeen,
            start_date: Some("2025-01".to_string()),
            end_date: None,
            line_number: 1,
            raw_text: "@t[~2025-01]".to_string(),
        }];
        assert_eq!(calculate_recency_boost(&tags, 0, 0.2), 1.0);
    }
}
