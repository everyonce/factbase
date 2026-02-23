//! Temporal tag parsing.

use crate::models::{TemporalTag, TemporalTagType};
use crate::patterns::{pad_negative_year, MALFORMED_TAG_REGEX, TEMPORAL_TAG_FULL_REGEX};
use regex::Regex;
use std::sync::LazyLock;

/// Regex to find range tags where the end date is missing the year (e.g., @t[2025-Q3..Q4])
static SHORT_RANGE_END_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[((?:-\d{1,4}|\d{4}))(-(?:Q[1-4]|\d{2}(?:-\d{2})?))\.\.(Q[1-4]|\d{2}(?:-\d{2})?)\]")
        .expect("short range end regex should compile")
});

/// Regex to match @t[...] tags containing BCE notation for normalization.
static BCE_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[[^\]]*\d{1,4}\s*BCE[^\]]*\]").expect("bce tag regex should compile")
});

/// Regex to match year (with optional date suffix) followed by BCE.
static BCE_YEAR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)\s*BCE")
        .expect("bce year regex should compile")
});

/// Normalize shorthand temporal tags:
/// - Short range ends: `@t[2025-Q3..Q4]` → `@t[2025-Q3..2025-Q4]`
/// - BCE notation: `@t[=331 BCE]` → `@t[=-331]`, `@t[490 BCE..479 BCE]` → `@t[-490..-479]`
pub(crate) fn normalize_temporal_tags(line: &str) -> std::borrow::Cow<'_, str> {
    let result = SHORT_RANGE_END_REGEX.replace_all(line, "@t[$1$2..$1-$3]");
    // Convert BCE notation to negative years within @t[...] tags
    if !result.contains("BCE") {
        return result;
    }
    let converted = BCE_TAG_REGEX.replace_all(&result, |caps: &regex::Captures| {
        let tag = caps.get(0).unwrap().as_str();
        BCE_YEAR_REGEX
            .replace_all(tag, |inner: &regex::Captures| {
                format!("-{}", inner.get(1).unwrap().as_str())
            })
            .to_string()
    });
    std::borrow::Cow::Owned(converted.into_owned())
}

/// Check if a line contains at least one valid temporal tag (including BCE notation).
///
/// This is the single source of truth for "does this line have a temporal tag?"
/// All consumers (coverage counting, question generation, etc.) should use this
/// instead of matching `TEMPORAL_TAG_FULL_REGEX` directly, which misses BCE tags.
pub(crate) fn line_has_temporal_tag(line: &str) -> bool {
    let normalized = normalize_temporal_tags(line);
    TEMPORAL_TAG_FULL_REGEX.is_match(&normalized)
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
            Some(pad_negative_year(end_date.as_str())),
        );
    }

    let prefix = cap.get(1).map(|m| m.as_str());
    let start_date = cap.get(2).map(|m| pad_negative_year(m.as_str()));
    let has_range = cap.get(3).is_some();
    let end_date = cap.get(4).map(|m| pad_negative_year(m.as_str()));

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

    #[test]
    fn test_bce_point_in_time() {
        let content = "- Battle of Actium @t[=-0031]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::PointInTime);
        assert_eq!(tags[0].start_date, Some("-0031".to_string()));
        assert_eq!(tags[0].raw_text, "@t[=-0031]");
    }

    #[test]
    fn test_bce_range() {
        let content = "- Greco-Persian Wars @t[-0490..-0479]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("-0490".to_string()));
        assert_eq!(tags[0].end_date, Some("-0479".to_string()));
    }

    #[test]
    fn test_bce_to_ce_range() {
        let content = "- Augustus reign @t[-0031..0014]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("-0031".to_string()));
        assert_eq!(tags[0].end_date, Some("0014".to_string()));
    }

    #[test]
    fn test_bce_last_seen() {
        let content = "- State as of @t[~-0031]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::LastSeen);
        assert_eq!(tags[0].start_date, Some("-0031".to_string()));
    }

    #[test]
    fn test_bce_ongoing() {
        let content = "- Ongoing since @t[-0490..]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Ongoing);
        assert_eq!(tags[0].start_date, Some("-0490".to_string()));
    }

    #[test]
    fn test_bce_historical() {
        let content = "- Ended by @t[..-0479]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Historical);
        assert_eq!(tags[0].end_date, Some("-0479".to_string()));
    }

    #[test]
    fn test_bce_with_month() {
        let content = "- Event @t[=-0490-03]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].start_date, Some("-0490-03".to_string()));
    }

    // --- Unpadded negative year tests ---

    #[test]
    fn test_unpadded_negative_year_point() {
        let content = "- Battle of Gaugamela @t[=-331]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::PointInTime);
        assert_eq!(tags[0].start_date, Some("-0331".to_string()));
    }

    #[test]
    fn test_unpadded_negative_year_range() {
        let content = "- Wars @t[-490..-479]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("-0490".to_string()));
        assert_eq!(tags[0].end_date, Some("-0479".to_string()));
    }

    #[test]
    fn test_unpadded_negative_year_with_month() {
        let content = "- Event @t[=-490-03]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].start_date, Some("-0490-03".to_string()));
    }

    #[test]
    fn test_unpadded_negative_year_ongoing() {
        let content = "- Since @t[-5..]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Ongoing);
        assert_eq!(tags[0].start_date, Some("-0005".to_string()));
    }

    #[test]
    fn test_unpadded_negative_year_historical() {
        let content = "- Before @t[..-479]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Historical);
        assert_eq!(tags[0].end_date, Some("-0479".to_string()));
    }

    // --- BCE notation tests ---

    #[test]
    fn test_bce_notation_point() {
        let content = "- Battle of Gaugamela @t[=331 BCE]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::PointInTime);
        assert_eq!(tags[0].start_date, Some("-0331".to_string()));
    }

    #[test]
    fn test_bce_notation_no_space() {
        let content = "- Event @t[=331BCE]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].start_date, Some("-0331".to_string()));
    }

    #[test]
    fn test_bce_notation_range() {
        let content = "- Wars @t[490 BCE..479 BCE]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag_type, TemporalTagType::Range);
        assert_eq!(tags[0].start_date, Some("-0490".to_string()));
        assert_eq!(tags[0].end_date, Some("-0479".to_string()));
    }

    #[test]
    fn test_bce_notation_with_month() {
        let content = "- Battle @t[=490-03 BCE]";
        let tags = parse_temporal_tags(content);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].start_date, Some("-0490-03".to_string()));
    }

    #[test]
    fn test_bce_not_flagged_malformed() {
        let content = "- Event @t[=331 BCE]";
        let malformed = find_malformed_tags(content);
        assert!(malformed.is_empty());
    }

    #[test]
    fn test_bce_normalize_preserves_non_bce() {
        let line = "- Modern event @t[=2024-03]";
        let normalized = normalize_temporal_tags(line);
        assert_eq!(normalized, "- Modern event @t[=2024-03]");
    }

    // --- line_has_temporal_tag tests ---

    #[test]
    fn test_line_has_temporal_tag_ce() {
        assert!(line_has_temporal_tag("- Fact @t[2024]"));
        assert!(line_has_temporal_tag("- Fact @t[=2024-03]"));
        assert!(line_has_temporal_tag("- Fact @t[2020..2022]"));
        assert!(line_has_temporal_tag("- Fact @t[?]"));
    }

    #[test]
    fn test_line_has_temporal_tag_bce() {
        assert!(line_has_temporal_tag("- Battle @t[=331 BCE]"));
        assert!(line_has_temporal_tag("- Wars @t[336 BCE..323 BCE]"));
        assert!(line_has_temporal_tag("- Event @t[=-0490]"));
        assert!(line_has_temporal_tag("- Range @t[-490..-479]"));
    }

    #[test]
    fn test_line_has_temporal_tag_rejects_malformed() {
        assert!(!line_has_temporal_tag("- Fact @t[traditional..modern]"));
        assert!(!line_has_temporal_tag("- Fact @t[static]"));
        assert!(!line_has_temporal_tag("- No tag here"));
    }
}
