//! Temporal tag parsing.

use crate::models::{TemporalTag, TemporalTagType};
use crate::patterns::{MALFORMED_TAG_REGEX, TEMPORAL_TAG_FULL_REGEX};
use regex::Regex;
use std::sync::LazyLock;

/// Regex to find range tags where the end date is missing the year (e.g., @t[2025-Q3..Q4])
static SHORT_RANGE_END_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[(\d{4})(-(?:Q[1-4]|\d{2}(?:-\d{2})?))\.\.(Q[1-4]|\d{2}(?:-\d{2})?)\]")
        .expect("short range end regex should compile")
});

/// Normalize shorthand range end dates by inheriting the year from the start date.
/// e.g., `@t[2025-Q3..Q4]` → `@t[2025-Q3..2025-Q4]`
fn normalize_temporal_tags(line: &str) -> std::borrow::Cow<'_, str> {
    SHORT_RANGE_END_REGEX.replace_all(line, "@t[$1$2..$1-$3]")
}

/// Parse all temporal tags from document content.
/// Returns a Vec of TemporalTag with line numbers (1-indexed).
pub fn parse_temporal_tags(content: &str) -> Vec<TemporalTag> {
    let mut tags = Vec::with_capacity(8);

    for (line_idx, line) in content.lines().enumerate() {
        let line_number = line_idx + 1;
        let normalized = normalize_temporal_tags(line);

        for cap in TEMPORAL_TAG_FULL_REGEX.captures_iter(&normalized) {
            let raw_text = cap.get(0).map_or("", |m| m.as_str()).to_string();
            let (tag_type, start_date, end_date) = parse_tag_components(&cap);

            tags.push(TemporalTag {
                tag_type,
                start_date,
                end_date,
                line_number,
                raw_text,
            });
        }
    }

    tags
}

/// Find malformed temporal tags — things that look like `@t[...]` but don't parse.
/// Returns `(line_number, raw_text)` pairs.
pub fn find_malformed_tags(content: &str) -> Vec<(usize, String)> {
    let mut malformed = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let normalized = normalize_temporal_tags(line);
        for m in MALFORMED_TAG_REGEX.find_iter(&normalized) {
            let text = m.as_str();
            if !TEMPORAL_TAG_FULL_REGEX.is_match(text) {
                malformed.push((line_idx + 1, text.to_string()));
            }
        }
    }
    malformed
}

/// Parse capture groups into tag type and dates
fn parse_tag_components(
    cap: &regex::Captures,
) -> (TemporalTagType, Option<String>, Option<String>) {
    let full_match = cap.get(0).map_or("", |m| m.as_str());

    if full_match == "@t[?]" {
        return (TemporalTagType::Unknown, None, None);
    }

    if let Some(end_date) = cap.get(5) {
        return (
            TemporalTagType::Historical,
            None,
            Some(end_date.as_str().to_string()),
        );
    }

    let prefix = cap.get(1).map(|m| m.as_str());
    let start_date = cap.get(2).map(|m| m.as_str().to_string());
    let has_range = cap.get(3).is_some();
    let end_date = cap.get(4).map(|m| m.as_str().to_string());

    let tag_type = match (prefix, has_range, &end_date) {
        (Some("~"), false, None) => TemporalTagType::LastSeen,
        (None, true, Some(_)) => TemporalTagType::Range,
        (None, true, None) => TemporalTagType::Ongoing,
        // (Some("="), false, None), (None, false, None), and any unrecognized combination
        _ => TemporalTagType::PointInTime,
    };

    (tag_type, start_date, end_date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_tag_point_in_time() {
        let content = "- Joined company @t[=2024-03]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::PointInTime);
        assert_eq!(tags[0].start_date, Some("2024-03".to_string()));
        assert_eq!(tags[0].end_date, None);
        assert_eq!(tags[0].line_number, 1);
        assert_eq!(tags[0].raw_text, "@t[=2024-03]");
    }

    #[test]
    fn test_temporal_tag_last_seen() {
        let content = "- Lives in NYC @t[~2024-01-15]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::LastSeen);
        assert_eq!(tags[0].start_date, Some("2024-01-15".to_string()));
    }

    #[test]
    fn test_temporal_tag_range() {
        let content = "- Worked at Acme @t[2020..2022]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("2020".to_string()));
        assert_eq!(tags[0].end_date, Some("2022".to_string()));
    }

    #[test]
    fn test_temporal_tag_ongoing() {
        let content = "- CEO at Startup @t[2020..]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Ongoing);
        assert_eq!(tags[0].start_date, Some("2020".to_string()));
        assert_eq!(tags[0].end_date, None);
    }

    #[test]
    fn test_temporal_tag_historical() {
        let content = "- Previous role @t[..2022]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Historical);
        assert_eq!(tags[0].end_date, Some("2022".to_string()));
    }

    #[test]
    fn test_temporal_tag_unknown() {
        let content = "- Some fact @t[?]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Unknown);
    }

    #[test]
    fn test_temporal_tag_no_tags() {
        let content = "This document has no temporal tags.";
        let tags = parse_temporal_tags(content);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_normalize_short_range_end_quarter() {
        let line = "- Sprint planning @t[2025-Q3..Q4]";
        let normalized = normalize_temporal_tags(line);
        assert_eq!(normalized, "- Sprint planning @t[2025-Q3..2025-Q4]");
    }

    #[test]
    fn test_normalize_short_range_end_month() {
        let line = "- Project @t[2025-01..03]";
        let normalized = normalize_temporal_tags(line);
        assert_eq!(normalized, "- Project @t[2025-01..2025-03]");
    }

    #[test]
    fn test_short_range_end_parses_as_range() {
        let content = "- Sprint @t[2025-Q3..Q4]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("2025-Q3".to_string()));
        assert_eq!(tags[0].end_date, Some("2025-Q4".to_string()));
    }

    #[test]
    fn test_full_range_unaffected_by_normalization() {
        let line = "- Role @t[2020..2022]";
        let normalized = normalize_temporal_tags(line);
        assert_eq!(normalized, "- Role @t[2020..2022]");
    }
}
