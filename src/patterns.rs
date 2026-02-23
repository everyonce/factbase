//! Shared regex patterns for factbase.
//!
//! Consolidates all regex patterns used across modules to ensure consistency
//! and avoid duplication.

use regex::Regex;
use std::sync::LazyLock;

// =============================================================================
// Document ID patterns
// =============================================================================

/// Matches factbase document header: `<!-- factbase:a1cb2b -->`
pub(crate) static ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^<!-- factbase:([a-f0-9]{6}) -->").expect("factbase header regex should be valid")
});

/// Validates a bare 6-character hex document ID (e.g., `a1cb2b`).
pub(crate) static DOC_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-f0-9]{6}$").expect("doc id regex should be valid"));

// =============================================================================
// Temporal tag patterns
// =============================================================================

/// Full temporal tag regex with capture groups for parsing.
/// Matches all valid @t[...] formats and captures components.
///
/// Capture groups:
/// - Group 1: prefix (`=` or `~`)
/// - Group 2: start date (YYYY, YYYY-QN, YYYY-MM, YYYY-MM-DD)
/// - Group 3: range separator + end date (if present)
/// - Group 4: end date only (for `DATE..DATE` format)
/// - Group 5: end date (for `..DATE` historical format)
pub(crate) static TEMPORAL_TAG_FULL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"@t\[(?:([=~])?(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)(\.\.(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)?)?|\.\.(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)|\?)\]"
    ).expect("temporal tag regex should be valid")
});

/// Simple temporal tag detection regex (no capture groups).
/// Use for checking if a line contains any temporal tag.
pub(crate) static TEMPORAL_TAG_DETECT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[[^\]]+\]").expect("temporal tag detect regex should compile")
});

/// Regex to detect malformed tags that look like temporal tags but don't match valid format.
pub(crate) static MALFORMED_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@t\[[^\]]*\]").expect("malformed tag regex should be valid"));

/// Regex to detect ongoing temporal tags like @t[2020..] or @t[2020-03..]
pub(crate) static ONGOING_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[(\d{4}(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)\.\.\]")
        .expect("ongoing tag regex should compile")
});

/// Regex to extract temporal tag content (captures the content inside brackets).
pub(crate) static TEMPORAL_TAG_CONTENT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[([^\]]+)\]").expect("temporal tag content regex should compile")
});

// =============================================================================
// Source footnote patterns
// =============================================================================

/// Source reference regex with capture group for footnote number.
/// Matches `[^N]` inline footnote references.
pub(crate) static SOURCE_REF_CAPTURE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\^(\d+)\]").expect("source reference regex should be valid"));

/// Simple source reference detection regex (no capture groups).
/// Use for checking if a line contains any source reference.
pub(crate) static SOURCE_REF_DETECT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\^\d+\]").expect("source ref detect regex should compile"));

/// Source definition regex - matches `[^N]: ...` footnote definitions.
/// Captures: group 1 = number, group 2 = definition text.
pub(crate) static SOURCE_DEF_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[\^(\d+)\]:\s*(.+)").expect("source definition regex should be valid")
});

// =============================================================================
// Fact/list item patterns
// =============================================================================

/// Regex for detecting list items (facts).
/// Matches: `- text`, `* text`, `1. text`, `1) text` (with optional leading whitespace).
pub(crate) static FACT_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:[-*]|\d+[.\)])\s+\S").expect("fact line regex should be valid")
});

// =============================================================================
// Date extraction patterns
// =============================================================================

/// Date extraction regex - matches YYYY-MM-DD, YYYY-MM, or YYYY in various contexts.
pub(crate) static DATE_EXTRACT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{4}-\d{2}-\d{2}|\d{4}-\d{2}|\d{4})")
        .expect("date extraction regex should be valid")
});

/// Regex to extract month names from text (e.g., "March 2024").
pub(crate) static MONTH_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{4})")
        .expect("month name regex should compile")
});

/// Regex to extract standalone years (19xx or 20xx).
pub(crate) static YEAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(19|20)\d{2}\b").expect("year regex should compile"));

// =============================================================================
// Review system patterns
// =============================================================================

/// Review question regex - matches: `- [ ] `@q\[type\]` description` or `- \[x\] `@q\[type\]` description`
pub(crate) static REVIEW_QUESTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-\s+\[([ xX])\]\s+`@q\[(\w+)\]`\s+(.+)$")
        .expect("review question regex should be valid")
});

/// Regex to extract quoted text from questions.
pub(crate) static QUOTED_TEXT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([^"]+)""#).expect("quoted text regex should compile"));

// =============================================================================
// Document structure patterns
// =============================================================================

/// Regex to match section headings (## Heading).
pub(crate) static SECTION_HEADING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s+(.+)$").expect("section heading regex should compile"));

/// Regex to match field: value patterns in list items.
pub(crate) static FIELD_VALUE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-*]\s+([^:]+):\s+").expect("field value regex should compile"));

// =============================================================================
// Link detection patterns
// =============================================================================

/// Manual link regex - matches `[[id]]` references.
pub static MANUAL_LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([a-f0-9]{6})\]\]").expect("manual link regex should be valid")
});

/// Review Queue marker comment.
pub(crate) const REVIEW_QUEUE_MARKER: &str = "<!-- factbase:review -->";

// =============================================================================
// Orphan review patterns
// =============================================================================

/// Regex for orphan entry with optional checkbox and answer.
/// Format: `- [x] content @r[orphan] <!-- from doc_id line N --> → answer`
/// Or: `- [ ] content @r[orphan] <!-- from doc_id line N -->`
pub(crate) static ORPHAN_ENTRY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^-\s+\[([ xX])\]\s+(.+?)\s+@r\[orphan\]\s*(?:<!--\s*from\s+(\w+)\s+line\s+(\d+)\s*-->)?\s*(?:→\s*(.+))?$"
    ).expect("orphan entry regex should be valid")
});

/// Regex for simple orphan entry (no checkbox, original format).
/// Format: `- content @r[orphan] <!-- from doc_id line N -->`
pub(crate) static SIMPLE_ORPHAN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-\s+(.+?)\s+@r\[orphan\]\s*(?:<!--\s*from\s+(\w+)\s+line\s+(\d+)\s*-->)?$")
        .expect("simple orphan regex should be valid")
});

// =============================================================================
// Date normalization functions
// =============================================================================

/// Normalize a date string for comparison by padding to YYYY-MM-DD format (start of period).
///
/// - YYYY -> YYYY-01-01
/// - YYYY-QN -> YYYY-MM-01 (Q1=01, Q2=04, Q3=07, Q4=10)
/// - YYYY-MM -> YYYY-MM-01
/// - YYYY-MM-DD -> as-is
pub(crate) fn normalize_date_for_comparison(date: &str) -> String {
    // Handle quarter format: YYYY-QN -> YYYY-MM (Q1=01, Q2=04, Q3=07, Q4=10)
    if date.len() == 7 && date.chars().nth(5) == Some('Q') {
        let year = &date[0..4];
        let quarter = &date[6..7];
        let month = match quarter {
            "2" => "04",
            "3" => "07",
            "4" => "10",
            // Q1 and any unrecognized quarter default to January
            _ => "01",
        };
        return format!("{year}-{month}-01");
    }

    match date.len() {
        4 => format!("{date}-01-01"), // YYYY -> YYYY-01-01
        7 => format!("{date}-01"),    // YYYY-MM -> YYYY-MM-01
        // YYYY-MM-DD and unknown formats returned as-is
        _ => date.to_string(),
    }
}

/// Normalize a date string to end of period for range comparisons.
///
/// - YYYY -> YYYY-12-31
/// - YYYY-QN -> end of quarter
/// - YYYY-MM -> YYYY-MM-{last day}
/// - YYYY-MM-DD -> as-is
pub(crate) fn normalize_date_to_end(date: &str) -> String {
    // Handle quarter format: YYYY-QN -> end of quarter
    if date.len() == 7 && date.chars().nth(5) == Some('Q') {
        let year = &date[0..4];
        let quarter = &date[6..7];
        let (month, day) = match quarter {
            "1" => ("03", "31"), // Q1 ends March 31
            "2" => ("06", "30"), // Q2 ends June 30
            "3" => ("09", "30"), // Q3 ends September 30
            // Q4 and any unrecognized quarter default to December 31
            _ => ("12", "31"),
        };
        return format!("{year}-{month}-{day}");
    }

    match date.len() {
        4 => format!("{date}-12-31"), // YYYY -> YYYY-12-31
        7 => {
            // YYYY-MM -> YYYY-MM-{last day}
            let year: i32 = date[0..4].parse().unwrap_or(2000);
            let month: u32 = date[5..7].parse().unwrap_or(1);
            let last_day = match month {
                4 | 6 | 9 | 11 => 30,
                2 => {
                    // Leap year check
                    if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                        29
                    } else {
                        28
                    }
                }
                // Months with 31 days and any unrecognized month
                _ => 31,
            };
            format!("{date}-{last_day:02}")
        }
        // YYYY-MM-DD and unknown formats returned as-is
        _ => date.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_regex() {
        assert!(ID_REGEX.is_match("<!-- factbase:a1cb2b -->"));
        assert!(!ID_REGEX.is_match("<!-- factbase:invalid -->"));
    }

    #[test]
    fn test_doc_id_regex() {
        assert!(DOC_ID_REGEX.is_match("a1cb2b"));
        assert!(DOC_ID_REGEX.is_match("000000"));
        assert!(!DOC_ID_REGEX.is_match("a1cb2b0")); // too long
        assert!(!DOC_ID_REGEX.is_match("a1cb2")); // too short
        assert!(!DOC_ID_REGEX.is_match("ABCDEF")); // uppercase
        assert!(!DOC_ID_REGEX.is_match("ghijkl")); // non-hex
    }

    #[test]
    fn test_temporal_tag_full_regex() {
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[2024]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[=2024-03]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[~2024-03]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[2020..2022]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[2020..]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[..2022]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[?]"));
        // Should not match empty or invalid
        assert!(!TEMPORAL_TAG_FULL_REGEX.is_match("@t[]"));
        assert!(!TEMPORAL_TAG_FULL_REGEX.is_match("@t[..]"));
    }

    #[test]
    fn test_temporal_tag_detect_regex() {
        assert!(TEMPORAL_TAG_DETECT_REGEX.is_match("fact @t[2024] here"));
        assert!(TEMPORAL_TAG_DETECT_REGEX.is_match("@t[?]"));
        assert!(!TEMPORAL_TAG_DETECT_REGEX.is_match("no tags here"));
    }

    #[test]
    fn test_source_ref_capture_regex() {
        let caps = SOURCE_REF_CAPTURE_REGEX.captures("fact [^1] here").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "1");
    }

    #[test]
    fn test_source_ref_detect_regex() {
        assert!(SOURCE_REF_DETECT_REGEX.is_match("fact [^1] here"));
        assert!(SOURCE_REF_DETECT_REGEX.is_match("[^99]"));
        assert!(!SOURCE_REF_DETECT_REGEX.is_match("no refs"));
    }

    #[test]
    fn test_fact_line_regex() {
        assert!(FACT_LINE_REGEX.is_match("- fact"));
        assert!(FACT_LINE_REGEX.is_match("* fact"));
        assert!(FACT_LINE_REGEX.is_match("1. fact"));
        assert!(FACT_LINE_REGEX.is_match("1) fact"));
        assert!(FACT_LINE_REGEX.is_match("  - indented"));
        assert!(!FACT_LINE_REGEX.is_match("not a list"));
    }

    #[test]
    fn test_date_extract_regex() {
        let caps = DATE_EXTRACT_REGEX.captures("scraped 2024-01-15").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "2024-01-15");
    }

    #[test]
    fn test_review_question_regex() {
        let caps = REVIEW_QUESTION_REGEX
            .captures("- [ ] `@q[temporal]` Line 5: description")
            .unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), " ");
        assert_eq!(caps.get(2).unwrap().as_str(), "temporal");
        assert_eq!(caps.get(3).unwrap().as_str(), "Line 5: description");
    }

    // Date normalization tests
    #[test]
    fn test_normalize_date_for_comparison_year() {
        assert_eq!(normalize_date_for_comparison("2024"), "2024-01-01");
    }

    #[test]
    fn test_normalize_date_for_comparison_year_month() {
        assert_eq!(normalize_date_for_comparison("2024-03"), "2024-03-01");
    }

    #[test]
    fn test_normalize_date_for_comparison_full() {
        assert_eq!(normalize_date_for_comparison("2024-03-15"), "2024-03-15");
    }

    #[test]
    fn test_normalize_date_for_comparison_quarter() {
        assert_eq!(normalize_date_for_comparison("2024-Q1"), "2024-01-01");
        assert_eq!(normalize_date_for_comparison("2024-Q2"), "2024-04-01");
        assert_eq!(normalize_date_for_comparison("2024-Q3"), "2024-07-01");
        assert_eq!(normalize_date_for_comparison("2024-Q4"), "2024-10-01");
    }

    #[test]
    fn test_normalize_date_to_end_year() {
        assert_eq!(normalize_date_to_end("2024"), "2024-12-31");
    }

    #[test]
    fn test_normalize_date_to_end_year_month() {
        assert_eq!(normalize_date_to_end("2024-01"), "2024-01-31");
        assert_eq!(normalize_date_to_end("2024-04"), "2024-04-30");
        assert_eq!(normalize_date_to_end("2024-02"), "2024-02-29"); // Leap year
        assert_eq!(normalize_date_to_end("2023-02"), "2023-02-28"); // Non-leap year
    }

    #[test]
    fn test_normalize_date_to_end_full() {
        assert_eq!(normalize_date_to_end("2024-03-15"), "2024-03-15");
    }

    #[test]
    fn test_normalize_date_to_end_quarter() {
        assert_eq!(normalize_date_to_end("2024-Q1"), "2024-03-31");
        assert_eq!(normalize_date_to_end("2024-Q2"), "2024-06-30");
        assert_eq!(normalize_date_to_end("2024-Q3"), "2024-09-30");
        assert_eq!(normalize_date_to_end("2024-Q4"), "2024-12-31");
    }
}
