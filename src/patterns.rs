//! Shared regex patterns for factbase.
//!
//! Consolidates all regex patterns used across modules to ensure consistency
//! and avoid duplication.

use regex::Regex;
use std::cmp::Ordering;
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
/// Year pattern: positive years require 4 digits; negative (BCE) years allow 1-4 digits.
/// e.g., `2024`, `-0330`, `-330`, `-5`
///
/// Capture groups:
/// - Group 1: prefix (`=` or `~`)
/// - Group 2: start date (YYYY, YYYY-QN, YYYY-MM, YYYY-MM-DD)
/// - Group 3: range separator + end date (if present)
/// - Group 4: end date only (for `DATE..DATE` format)
/// - Group 5: end date (for `..DATE` historical format)
pub(crate) static TEMPORAL_TAG_FULL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // YEAR = (?:-\d{1,4}|\d{4})  — negative years 1-4 digits, positive years exactly 4
    // DATE = YEAR(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?
    Regex::new(concat!(
        r"@t\[(?:",
            r"([=~])?",                                                    // G1: prefix
            r"((?:-\d{1,4}|\d{4})(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)",     // G2: start date
            r"(",                                                           // G3: range part
                r"\.\.",
                r"((?:-\d{1,4}|\d{4})(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)?", // G4: end date
            r")?",
        r"|",
            r"\.\.((?:-\d{1,4}|\d{4})(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)", // G5: historical
        r"|",
            r"\?",
        r")\]",
    )).expect("temporal tag regex should be valid")
});

/// Simple temporal tag detection regex (no capture groups).
/// Use for checking if a line contains any temporal tag.
pub(crate) static TEMPORAL_TAG_DETECT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[[^\]]+\]").expect("temporal tag detect regex should compile")
});

/// Regex to detect malformed tags that look like temporal tags but don't match valid format.
pub(crate) static MALFORMED_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@t\[[^\]]*\]").expect("malformed tag regex should be valid"));

/// Regex to detect ongoing temporal tags like @t[2020..] or @t[2020-03..] or @t[-330..]
pub(crate) static ONGOING_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@t\[((?:-\d{1,4}|\d{4})(?:-(?:Q[1-4]|\d{2}(?:-\d{2})?))?)\.\.\]")
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
// Title cleaning
// =============================================================================

/// Strip footnote references (`[^N]`) from an extracted title and trim whitespace.
pub fn clean_title(title: &str) -> String {
    SOURCE_REF_DETECT_REGEX.replace_all(title, "").trim().to_string()
}

/// Extract the first `# ` heading from content and clean it.
///
/// Returns `None` if no H1 heading is found.
pub fn extract_heading_title(content: &str) -> Option<String> {
    content
        .lines()
        .find(|l| l.starts_with("# ") && !l.starts_with("## "))
        .map(|l| clean_title(&l[2..]))
}

// =============================================================================
// Fact/list item patterns
// =============================================================================

/// Regex for detecting list items (facts).
/// Matches: `- text`, `* text`, `1. text`, `1) text` (with optional leading whitespace).
pub static FACT_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:[-*]|\d+[.\)])\s+\S").expect("fact line regex should be valid")
});

// =============================================================================
// LLM meta-commentary detection
// =============================================================================

/// Detects LLM self-referential meta-commentary artifacts that were erroneously
/// included in document content. These are not factual claims and should be
/// skipped during question generation.
///
/// Matches patterns like:
/// - "Rewrite ... as factual content"
/// - "I'll update the document..."
/// - "Let me clarify this section..."
/// - "Here is the updated version..."
/// - "Note: I've rephrased..."
pub static META_COMMENTARY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^[-*]\s+)?(?:(?:I'(?:ll|ve|m|d)|I (?:will|have|can|should|would|need to)|let me|here (?:is|are)|note(?::|\s+that))\s+.{0,60}(?:rewrit|rephras|clarif|updat|revis|summariz|merg|reorganiz|edit|modif|format|correct|adjust|document|section|content|entry|fact|the (?:above|below|following))|(?:rewrit|rephras|updat|revis|merg|reorganiz|edit|modif|format|correct|adjust)(?:e|ed|ing|ten)?\s+.{0,40}(?:as (?:if|though)|(?:factual|document|entry|section) content|this (?:document|section|entry|fact)))").expect("meta commentary regex should be valid")
});

/// Detects corruption artifacts from failed review application runs.
///
/// These are process/system phrases that should never appear in factual document
/// content. When multiple matches are found in a document, the content is likely
/// corrupted and should be flagged rather than checked for quality.
///
/// Matches phrases like:
/// - "apply_review_answers"
/// - "CHANGES specification"
/// - "logical impossibility"
/// - "corruption metadata"
/// - "the answer format"
/// - "the question format"
static CORRUPTION_ARTIFACT_PHRASES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:apply_review_answers|CHANGES\s+specification|logical\s+impossibility|corruption\s+(?:metadata|artifact)|the\s+(?:question|answer)\s+format\b|Changes\s+\d+[-–]\d+\s+ask\b)")
        .expect("corruption artifact regex should be valid")
});

/// Minimum number of corruption artifact matches to flag a document as corrupted.
const CORRUPTION_THRESHOLD: usize = 2;

/// Returns `true` if the document content contains corruption artifacts from a
/// failed review application run (e.g. meta-commentary about changes,
/// corruption metadata, format mismatches).
pub fn has_corruption_artifacts(content: &str) -> bool {
    CORRUPTION_ARTIFACT_PHRASES.find_iter(content).count() >= CORRUPTION_THRESHOLD
}

// =============================================================================
// Date extraction patterns
// =============================================================================

/// Date extraction regex - matches YYYY-MM-DD, YYYY-MM, or YYYY in various contexts.
pub(crate) static DATE_EXTRACT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(-?\d{4}-\d{2}-\d{2}|-?\d{4}-\d{2}|-?\d{4})")
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
    Regex::new(r"^-\s+\[([ xX])\]\s+`@q\[([\w-]+)\]`\s+(.+)$")
        .expect("review question regex should be valid")
});

/// Inline `@q[type]` marker (backtick-wrapped or bare) — for detecting orphaned markers outside review section.
pub(crate) static INLINE_QUESTION_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*`?@q\[[\w-]+\]`?").expect("inline question marker regex should be valid")
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

/// Wikilink regex - matches `[[Name]]` references (any non-bracket content).
pub static WIKILINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\[\]]+)\]\]").expect("wikilink regex should be valid")
});

/// Review Queue marker comment.
pub(crate) const REVIEW_QUEUE_MARKER: &str = "<!-- factbase:review -->";

/// Callout header used for Obsidian-format review sections.
pub(crate) const REVIEW_CALLOUT_HEADER: &str = "> [!info]- Review Queue";

/// Reference entity marker comment.
pub const REFERENCE_MARKER: &str = "<!-- factbase:reference -->";

/// Returns `true` if the document content contains the reference entity marker.
pub fn is_reference_doc(content: &str) -> bool {
    content.contains(REFERENCE_MARKER)
}

/// Return the byte offset where the document body ends (before any review queue
/// section). Checks for both the HTML marker comment and a bare `## Review Queue`
/// heading, returning whichever appears first. This prevents question generators
/// from treating review queue entries as document facts when the marker is missing.
pub(crate) fn body_end_offset(content: &str) -> usize {
    let marker = content.find(REVIEW_QUEUE_MARKER).map(|pos| {
        // If marker is inside a callout line (`> <!-- factbase:review -->`),
        // walk back to the start of that line.
        content[..pos]
            .rfind('\n')
            .map(|nl| nl + 1)
            .unwrap_or(0)
    });
    let heading = content
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1; // +1 for newline
            Some((start, line))
        })
        .find(|(_, line)| {
            let t = line.trim();
            t == "## Review Queue" || t == REVIEW_CALLOUT_HEADER
        })
        .map(|(pos, _)| pos);
    match (marker, heading) {
        (Some(m), Some(h)) => m.min(h),
        (Some(m), None) => m,
        (None, Some(h)) => h,
        (None, None) => content.len(),
    }
}

/// Return the document body without the review queue section.
///
/// Callers that pass content to question generators should use this to ensure
/// review queue entries are never analysed as document facts.
pub fn content_body(content: &str) -> &str {
    &content[..body_end_offset(content)]
}

/// Extract the review queue section from `content`, including any preceding
/// `---` separator. Returns `None` if no review queue marker is present.
pub(crate) fn extract_review_queue_section(content: &str) -> Option<&str> {
    if !content.contains(REVIEW_QUEUE_MARKER) {
        return None;
    }
    let mut offset = body_end_offset(content);
    // Walk backwards over blank lines and a `---` separator if present
    let before = &content[..offset];
    let trimmed = before.trim_end_matches('\n');
    if trimmed.ends_with("---") {
        offset = trimmed.len() - 3;
        // Also include a preceding newline if present
        if offset > 0 && content.as_bytes()[offset - 1] == b'\n' {
            offset -= 1;
        }
    }
    Some(&content[offset..])
}

/// Merge a review queue from `db_content` into `disk_content` when the disk
/// file is stale (lacks the review queue that the DB has).
///
/// Returns `Some(merged)` when the DB has a review queue and the disk does not.
/// Returns `None` in all other cases (no merge needed).
pub(crate) fn merge_review_queue(disk_content: &str, db_content: &str) -> Option<String> {
    // If disk already has a review queue, disk wins (explicit user edit or both have it)
    if disk_content.contains(REVIEW_QUEUE_MARKER) {
        return None;
    }
    // If DB doesn't have a review queue, nothing to preserve
    let review_section = extract_review_queue_section(db_content)?;
    let mut merged = disk_content.to_string();
    if !merged.ends_with('\n') {
        merged.push('\n');
    }
    merged.push_str(review_section);
    Some(merged)
}

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
// Reviewed-fact markers
// =============================================================================

/// Matches `<!-- reviewed:YYYY-MM-DD ... -->` markers on fact lines.
/// Allows optional explanation text after the date (e.g., `<!-- reviewed:2026-02-21 Not a conflict -->`).
pub(crate) static REVIEWED_MARKER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<!-- reviewed:(\d{4}-\d{2}-\d{2})\b.*?-->")
        .expect("reviewed marker regex should be valid")
});

/// Extract the reviewed date from a line containing a `<!-- reviewed:YYYY-MM-DD -->` marker.
pub fn extract_reviewed_date(line: &str) -> Option<chrono::NaiveDate> {
    let caps = REVIEWED_MARKER_REGEX.captures(line)?;
    chrono::NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()
}

/// Add or update a `<!-- reviewed:YYYY-MM-DD -->` marker on a line.
///
/// If the line already has a reviewed marker, replaces only the date
/// (preserving any explanation text). Otherwise appends the marker.
pub(crate) fn add_or_update_reviewed_marker(line: &str, date: &chrono::NaiveDate) -> String {
    if let Some(caps) = REVIEWED_MARKER_REGEX.captures(line) {
        let old_date = &caps[1];
        let new_date = date.format("%Y-%m-%d").to_string();
        line.replacen(old_date, &new_date, 1)
    } else {
        format!("{line} <!-- reviewed:{date} -->")
    }
}

/// Strip all `<!-- reviewed:YYYY-MM-DD ... -->` and `<!-- sequential ... -->` markers from content.
/// Used to measure how many questions would be generated without suppression.
pub fn strip_reviewed_markers(content: &str) -> String {
    let stripped = REVIEWED_MARKER_REGEX.replace_all(content, "");
    SEQUENTIAL_MARKER_REGEX.replace_all(&stripped, "").to_string()
}

/// Extract the `reviewed: YYYY-MM-DD` date from YAML frontmatter.
pub fn extract_frontmatter_reviewed_date(content: &str) -> Option<chrono::NaiveDate> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("reviewed:") {
            return chrono::NaiveDate::parse_from_str(val.trim(), "%Y-%m-%d").ok();
        }
    }
    None
}

/// Set or update the `reviewed: YYYY-MM-DD` field in YAML frontmatter.
///
/// If frontmatter exists, adds or updates the `reviewed:` field.
/// If no frontmatter exists, creates one with just the reviewed field.
pub fn set_frontmatter_reviewed_date(content: &str, date: &chrono::NaiveDate) -> String {
    let date_str = date.format("%Y-%m-%d").to_string();
    let lines: Vec<&str> = content.lines().collect();

    if lines.first().map(|l| l.trim()) == Some("---") {
        // Find closing ---
        let close = lines.iter().skip(1).position(|l| l.trim() == "---").map(|i| i + 1);
        if let Some(close_idx) = close {
            // Check if reviewed: already exists
            let mut found = false;
            let mut result = Vec::with_capacity(lines.len());
            for (i, line) in lines.iter().enumerate() {
                if i > 0 && i < close_idx && line.trim().starts_with("reviewed:") {
                    result.push(format!("reviewed: {date_str}"));
                    found = true;
                } else {
                    result.push(line.to_string());
                }
            }
            if !found {
                // Insert before closing ---
                result.insert(close_idx, format!("reviewed: {date_str}"));
            }
            return result.join("\n");
        }
    }

    // No frontmatter — create one
    format!("---\nreviewed: {date_str}\n---\n{content}")
}

/// Strip all inline `<!-- reviewed:... -->` markers from content and return
/// the cleaned content plus the latest reviewed date found (if any).
pub fn convert_inline_reviewed_to_frontmatter(content: &str) -> (String, Option<chrono::NaiveDate>) {
    let mut latest: Option<chrono::NaiveDate> = None;
    for line in content.lines() {
        if let Some(d) = extract_reviewed_date(line) {
            latest = Some(latest.map_or(d, |prev: chrono::NaiveDate| prev.max(d)));
        }
    }
    // Also consider existing frontmatter date
    if let Some(fm_date) = extract_frontmatter_reviewed_date(content) {
        latest = Some(latest.map_or(fm_date, |prev| prev.max(fm_date)));
    }
    let stripped = strip_reviewed_markers(content);
    // Clean up trailing whitespace left by marker removal on each line
    let cleaned: String = stripped.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    (cleaned, latest)
}

/// Matches `<!-- sequential ... -->` markers on fact lines.
static SEQUENTIAL_MARKER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<!-- sequential\b.*?-->").expect("sequential marker regex should be valid")
});

// =============================================================================
// =============================================================================
// Date normalization and comparison functions
// =============================================================================

/// Zero-pad a negative year to 4 digits. Positive dates pass through unchanged.
/// e.g., "-330" → "-0330", "-5" → "-0005", "-0490" → "-0490", "2024" → "2024"
/// For dates with suffixes (e.g., "-330-03"), only the year part is padded.
pub(crate) fn pad_negative_year(date: &str) -> String {
    if !date.starts_with('-') {
        return date.to_string();
    }
    // Find where the year digits end (first '-' after the leading '-')
    let rest = &date[1..];
    let (year_str, suffix) = match rest.find('-') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, ""),
    };
    if year_str.len() >= 4 {
        return date.to_string();
    }
    format!("-{:0>4}{suffix}", year_str)
}

/// Compare two normalized date strings, correctly handling negative (BCE) years.
///
/// For positive dates, lexicographic comparison works because they are zero-padded.
/// For negative dates, the absolute values must be compared in reverse order
/// (e.g., -0490 < -0031 numerically, but "-0490" > "-0031" lexicographically).
pub(crate) fn date_cmp(a: &str, b: &str) -> Ordering {
    let a_neg = a.starts_with('-');
    let b_neg = b.starts_with('-');
    match (a_neg, b_neg) {
        (false, false) => a.cmp(b),
        (true, true) => b[1..].cmp(&a[1..]),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    }
}

/// Normalize a date string for comparison by padding to YYYY-MM-DD format (start of period).
/// Handles negative (BCE) years like -0490, -0490-03, -0490-Q2.
///
/// - YYYY -> YYYY-01-01
/// - YYYY-QN -> YYYY-MM-01 (Q1=01, Q2=04, Q3=07, Q4=10)
/// - YYYY-MM -> YYYY-MM-01
/// - YYYY-MM-DD -> as-is
pub(crate) fn normalize_date_for_comparison(date: &str) -> String {
    let date = pad_negative_year(date);
    let (prefix, rest) = if let Some(stripped) = date.strip_prefix('-') {
        ("-", stripped)
    } else {
        ("", date.as_str())
    };

    // Handle quarter format: YYYY-QN -> YYYY-MM (Q1=01, Q2=04, Q3=07, Q4=10)
    if rest.len() == 7 && rest.chars().nth(5) == Some('Q') {
        let year = &rest[0..4];
        let quarter = &rest[6..7];
        let month = match quarter {
            "2" => "04",
            "3" => "07",
            "4" => "10",
            // Q1 and any unrecognized quarter default to January
            _ => "01",
        };
        return format!("{prefix}{year}-{month}-01");
    }

    match rest.len() {
        4 => format!("{prefix}{rest}-01-01"),  // YYYY -> YYYY-01-01
        7 => format!("{prefix}{rest}-01"),      // YYYY-MM -> YYYY-MM-01
        // YYYY-MM-DD and unknown formats returned as-is
        _ => date.to_string(),
    }
}

/// Normalize a date string to end of period for range comparisons.
/// Handles negative (BCE) years like -0490, -0490-03, -0490-Q2.
///
/// - YYYY -> YYYY-12-31
/// - YYYY-QN -> end of quarter
/// - YYYY-MM -> YYYY-MM-{last day}
/// - YYYY-MM-DD -> as-is
pub(crate) fn normalize_date_to_end(date: &str) -> String {
    let date = pad_negative_year(date);
    let (prefix, rest) = if let Some(stripped) = date.strip_prefix('-') {
        ("-", stripped)
    } else {
        ("", date.as_str())
    };

    // Handle quarter format: YYYY-QN -> end of quarter
    if rest.len() == 7 && rest.chars().nth(5) == Some('Q') {
        let year = &rest[0..4];
        let quarter = &rest[6..7];
        let (month, day) = match quarter {
            "1" => ("03", "31"), // Q1 ends March 31
            "2" => ("06", "30"), // Q2 ends June 30
            "3" => ("09", "30"), // Q3 ends September 30
            // Q4 and any unrecognized quarter default to December 31
            _ => ("12", "31"),
        };
        return format!("{prefix}{year}-{month}-{day}");
    }

    match rest.len() {
        4 => format!("{prefix}{rest}-12-31"), // YYYY -> YYYY-12-31
        7 => {
            // YYYY-MM -> YYYY-MM-{last day}
            let year: i32 = rest[0..4].parse().unwrap_or(2000);
            let month: u32 = rest[5..7].parse().unwrap_or(1);
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
            format!("{prefix}{rest}-{last_day:02}")
        }
        // YYYY-MM-DD and unknown formats returned as-is
        _ => date.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reference_doc() {
        assert!(is_reference_doc("<!-- factbase:reference -->\n# AWS Lambda\n"));
        assert!(is_reference_doc("<!-- factbase:abc123 -->\n<!-- factbase:reference -->\n# AWS Lambda\n"));
        assert!(!is_reference_doc("# Regular Doc\n\nContent"));
        assert!(!is_reference_doc("<!-- factbase:review -->\n# Doc\n"));
    }

    #[test]
    fn test_clean_title_strips_footnote_refs() {
        assert_eq!(clean_title("Joan Butters [^8] [^9]"), "Joan Butters");
        assert_eq!(clean_title("Title [^1]"), "Title");
        assert_eq!(clean_title("No refs here"), "No refs here");
        assert_eq!(clean_title("  Spaced [^3]  "), "Spaced");
        assert_eq!(clean_title("[^1] Leading ref"), "Leading ref");
    }

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
        // BCE (negative year) formats
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[=-0031]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[~-0031]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-0490..-0479]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-0031..0014]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[..-0479]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-0490..]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-0490-03]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-0490-Q2]"));
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

    #[test]
    fn test_review_question_regex_hyphenated_type() {
        let caps = REVIEW_QUESTION_REGEX
            .captures("- [ ] `@q[weak-source]` Line 3: Vague citation")
            .unwrap();
        assert_eq!(caps.get(2).unwrap().as_str(), "weak-source");
        assert_eq!(caps.get(3).unwrap().as_str(), "Line 3: Vague citation");
    }

    // Date normalization tests
    #[test]
    fn test_normalize_date_for_comparison() {
        assert_eq!(normalize_date_for_comparison("2024"), "2024-01-01");
        assert_eq!(normalize_date_for_comparison("2024-03"), "2024-03-01");
        assert_eq!(normalize_date_for_comparison("2024-03-15"), "2024-03-15");
        assert_eq!(normalize_date_for_comparison("2024-Q1"), "2024-01-01");
        assert_eq!(normalize_date_for_comparison("2024-Q2"), "2024-04-01");
        assert_eq!(normalize_date_for_comparison("2024-Q3"), "2024-07-01");
        assert_eq!(normalize_date_for_comparison("2024-Q4"), "2024-10-01");
        // BCE dates
        assert_eq!(normalize_date_for_comparison("-0490"), "-0490-01-01");
        assert_eq!(normalize_date_for_comparison("-0490-03"), "-0490-03-01");
        assert_eq!(normalize_date_for_comparison("-0490-Q2"), "-0490-04-01");
        assert_eq!(normalize_date_for_comparison("-0031-03-15"), "-0031-03-15");
    }

    #[test]
    fn test_normalize_date_to_end() {
        assert_eq!(normalize_date_to_end("2024"), "2024-12-31");
        assert_eq!(normalize_date_to_end("2024-01"), "2024-01-31");
        assert_eq!(normalize_date_to_end("2024-04"), "2024-04-30");
        assert_eq!(normalize_date_to_end("2024-02"), "2024-02-29"); // Leap year
        assert_eq!(normalize_date_to_end("2023-02"), "2023-02-28"); // Non-leap year
        assert_eq!(normalize_date_to_end("2024-03-15"), "2024-03-15");
        assert_eq!(normalize_date_to_end("2024-Q1"), "2024-03-31");
        assert_eq!(normalize_date_to_end("2024-Q2"), "2024-06-30");
        assert_eq!(normalize_date_to_end("2024-Q3"), "2024-09-30");
        assert_eq!(normalize_date_to_end("2024-Q4"), "2024-12-31");
        // BCE dates
        assert_eq!(normalize_date_to_end("-0490"), "-0490-12-31");
        assert_eq!(normalize_date_to_end("-0490-03"), "-0490-03-31");
        assert_eq!(normalize_date_to_end("-0490-Q1"), "-0490-03-31");
    }

    #[test]
    fn test_date_cmp() {
        // Positive vs positive
        assert_eq!(date_cmp("2020-01-01", "2022-01-01"), Ordering::Less);
        assert_eq!(date_cmp("2022-01-01", "2020-01-01"), Ordering::Greater);
        assert_eq!(date_cmp("2020-01-01", "2020-01-01"), Ordering::Equal);
        // Negative vs positive (BCE < CE)
        assert_eq!(date_cmp("-0031-01-01", "0014-01-01"), Ordering::Less);
        assert_eq!(date_cmp("0014-01-01", "-0031-01-01"), Ordering::Greater);
        // Negative vs negative (-0490 < -0031 numerically)
        assert_eq!(date_cmp("-0490-01-01", "-0031-01-01"), Ordering::Less);
        assert_eq!(date_cmp("-0031-01-01", "-0490-01-01"), Ordering::Greater);
        assert_eq!(date_cmp("-0031-01-01", "-0031-01-01"), Ordering::Equal);
    }

    #[test]
    fn test_extract_reviewed_date() {
        // Valid date
        let line = "- VP of Engineering @t[~2026-02] [^1] <!-- reviewed:2026-02-15 -->";
        assert_eq!(extract_reviewed_date(line).unwrap(), chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        // With explanation text
        let line2 = "- CTO @t[2020..] <!-- reviewed:2026-02-21 Not a conflict: advisory role -->";
        assert_eq!(extract_reviewed_date(line2).unwrap(), chrono::NaiveDate::from_ymd_opt(2026, 2, 21).unwrap());
        // No marker / invalid date
        assert!(extract_reviewed_date("- Some fact @t[~2026-02]").is_none());
        assert!(extract_reviewed_date("<!-- reviewed:2026-13-45 -->").is_none());
    }

    #[test]
    fn test_reviewed_marker_regex_captures() {
        let text = "fact text <!-- reviewed:2025-06-01 --> more text";
        let caps = REVIEWED_MARKER_REGEX.captures(text).unwrap();
        assert_eq!(&caps[1], "2025-06-01");
    }

    #[test]
    fn test_add_or_update_reviewed_marker() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        // New marker
        assert_eq!(
            add_or_update_reviewed_marker("- VP of Engineering @t[~2026-02] [^1]", &date),
            "- VP of Engineering @t[~2026-02] [^1] <!-- reviewed:2026-02-15 -->"
        );
        // Update existing
        assert_eq!(
            add_or_update_reviewed_marker("- VP of Engineering @t[~2026-02] <!-- reviewed:2025-01-01 -->", &date),
            "- VP of Engineering @t[~2026-02] <!-- reviewed:2026-02-15 -->"
        );
        // No existing tags
        assert_eq!(
            add_or_update_reviewed_marker("- Works at Acme Corp", &date),
            "- Works at Acme Corp <!-- reviewed:2026-02-15 -->"
        );
        // Preserves explanation
        assert_eq!(
            add_or_update_reviewed_marker("- CTO at TechCo @t[2020..] <!-- reviewed:2025-01-01 Not a conflict: advisory role -->", &date),
            "- CTO at TechCo @t[2020..] <!-- reviewed:2026-02-15 Not a conflict: advisory role -->"
        );
    }

    // =========================================================================
    // strip_reviewed_markers tests
    // =========================================================================

    #[test]
    fn test_strip_reviewed_markers_removes_markers() {
        let content = "- Fact one <!-- reviewed:2026-01-15 -->\n- Fact two";
        let result = strip_reviewed_markers(content);
        assert!(!result.contains("reviewed"));
        assert!(result.contains("- Fact one"));
        assert!(result.contains("- Fact two"));
    }

    #[test]
    fn test_strip_reviewed_markers_removes_sequential() {
        let content = "- Fact one <!-- sequential -->\n- Fact two <!-- reviewed:2026-01-15 -->";
        let result = strip_reviewed_markers(content);
        assert!(!result.contains("sequential"));
        assert!(!result.contains("reviewed"));
        assert!(result.contains("- Fact one"));
        assert!(result.contains("- Fact two"));
    }

    #[test]
    fn test_strip_reviewed_markers_preserves_other_comments() {
        let content = "- Fact <!-- factbase:abc123 -->\n- Other <!-- reviewed:2026-01-15 -->";
        let result = strip_reviewed_markers(content);
        assert!(result.contains("<!-- factbase:abc123 -->"));
        assert!(!result.contains("reviewed"));
    }

    // =========================================================================
    // META_COMMENTARY_REGEX tests
    // =========================================================================

    #[test]
    fn test_meta_commentary_matches() {
        // Positive cases: editing/meta language
        for text in [
            "- Rewrite my own clarification text as if it were factual content",
            "- I'll update the document with corrections",
            "- Let me clarify this section",
            "- Here is the updated content",
            "- I've revised the entry to correct the facts",
            "- Note: I've rephrased the document",
        ] {
            assert!(META_COMMENTARY_REGEX.is_match(text), "Should match: {}", text);
        }
        // Negative cases: real facts
        for text in [
            "- VP of Engineering at Acme Corp @t[2020..]",
            "- Lives in San Francisco @t[~2024]",
            "- Notable for pioneering work in AI",
        ] {
            assert!(!META_COMMENTARY_REGEX.is_match(text), "Should NOT match: {}", text);
        }
    }

    // =========================================================================
    // has_corruption_artifacts tests
    // =========================================================================

    #[test]
    fn test_corruption_artifacts_detected() {
        let content = "# Anupam Kumar\n\n\
            - Changes 1-3 ask when was this true\n\
            - The question format (when/what) does not match the answer format\n\
            - Senior Engineer at Acme Corp\n";
        assert!(has_corruption_artifacts(content));
    }

    #[test]
    fn test_corruption_artifacts_apply_review_and_changes_spec() {
        let content = "# Some Doc\n\n\
            - apply_review_answers produced corruption metadata\n\
            - CHANGES specification was malformed\n";
        assert!(has_corruption_artifacts(content));
    }

    #[test]
    fn test_corruption_artifacts_logical_impossibility() {
        let content = "# Doc\n\
            - This is a logical impossibility given the dates\n\
            - corruption artifact from previous run\n";
        assert!(has_corruption_artifacts(content));
    }

    #[test]
    fn test_no_corruption_in_normal_doc() {
        let content = "# Jane Smith\n\n\
            - VP of Engineering at Acme Corp @t[2020..]\n\
            - Lives in San Francisco @t[~2024]\n\
            - Previously at Google @t[2015..2020] [^1]\n";
        assert!(!has_corruption_artifacts(content));
    }

    #[test]
    fn test_single_match_below_threshold() {
        // One match alone shouldn't flag — could be a legitimate mention
        let content = "# Doc\n\n\
            - The apply_review_answers command was run\n\
            - Normal fact about a person\n";
        assert!(!has_corruption_artifacts(content));
    }

    // =========================================================================
    // body_end_offset tests
    // =========================================================================

    #[test]
    fn test_body_end_offset() {
        // With marker
        let c1 = "# Title\n\n- fact\n\n<!-- factbase:review -->\n- [ ] question";
        assert_eq!(body_end_offset(c1), c1.find("<!-- factbase:review -->").unwrap());
        // With heading only
        let c2 = "# Title\n\n- fact\n\n## Review Queue\n\n- [ ] question";
        assert_eq!(body_end_offset(c2), c2.find("## Review Queue").unwrap());
        // Heading before marker
        let c3 = "# Title\n\n- fact\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] q";
        assert_eq!(body_end_offset(c3), c3.find("## Review Queue").unwrap());
        // No review section
        let c4 = "# Title\n\n- fact one\n- fact two\n";
        assert_eq!(body_end_offset(c4), c4.len());
        // Callout format — body ends at callout header
        let c5 = "# Title\n\n- fact\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] q\n";
        assert_eq!(body_end_offset(c5), c5.find("> [!info]- Review Queue").unwrap());
        // Callout marker without header — body ends at start of marker line
        let c6 = "# Title\n\n- fact\n\n> <!-- factbase:review -->\n> - [ ] q\n";
        assert_eq!(body_end_offset(c6), c6.find("> <!-- factbase:review -->").unwrap());
    }

    #[test]
    fn test_content_body_strips_review_queue() {
        let c = "# Title\n\n- fact\n\n<!-- factbase:review -->\n- [ ] question";
        assert_eq!(content_body(c), "# Title\n\n- fact\n\n");
    }

    #[test]
    fn test_content_body_no_review_queue() {
        let c = "# Title\n\n- fact one\n";
        assert_eq!(content_body(c), c);
    }

    // =========================================================================
    // pad_negative_year tests
    // =========================================================================

    #[test]
    fn test_pad_negative_year() {
        assert_eq!(pad_negative_year("-330"), "-0330");
        assert_eq!(pad_negative_year("-31"), "-0031");
        assert_eq!(pad_negative_year("-5"), "-0005");
        assert_eq!(pad_negative_year("-0490"), "-0490");
        assert_eq!(pad_negative_year("2024"), "2024");
        assert_eq!(pad_negative_year("-330-03"), "-0330-03");
        assert_eq!(pad_negative_year("-5-Q2"), "-0005-Q2");
        assert_eq!(pad_negative_year("-490-03-15"), "-0490-03-15");
    }

    #[test]
    fn test_normalize_unpadded_bce_dates() {
        assert_eq!(normalize_date_for_comparison("-330"), "-0330-01-01");
        assert_eq!(normalize_date_for_comparison("-31-06"), "-0031-06-01");
        assert_eq!(normalize_date_to_end("-490"), "-0490-12-31");
        assert_eq!(normalize_date_to_end("-31-03"), "-0031-03-31");
    }

    #[test]
    fn test_temporal_tag_full_regex_unpadded_bce() {
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[=-330]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[~-31]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-490..-479]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-5..]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[..-479]"));
        assert!(TEMPORAL_TAG_FULL_REGEX.is_match("@t[-490-03]"));
    }

    // =========================================================================
    // extract_review_queue_section tests
    // =========================================================================

    #[test]
    fn test_extract_review_queue_section_present() {
        let content = "# Title\n\n- fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When?\n  > \n";
        let section = extract_review_queue_section(content).unwrap();
        assert!(section.starts_with("---") || section.starts_with("\n---"));
        assert!(section.contains("<!-- factbase:review -->"));
        assert!(section.contains("@q[temporal]"));
    }

    #[test]
    fn test_extract_review_queue_section_absent() {
        let content = "# Title\n\n- fact\n";
        assert!(extract_review_queue_section(content).is_none());
    }

    // =========================================================================
    // merge_review_queue tests
    // =========================================================================

    #[test]
    fn test_merge_review_queue_stale_disk_preserves_db_queue() {
        let disk = "<!-- factbase:abc123 -->\n# Title\n\n- fact one\n";
        let db = "<!-- factbase:abc123 -->\n# Title\n\n- fact one\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When?\n  > \n";
        let merged = merge_review_queue(disk, db).unwrap();
        assert!(merged.contains("<!-- factbase:review -->"));
        assert!(merged.contains("@q[temporal]"));
        assert!(merged.contains("- fact one"));
    }

    #[test]
    fn test_merge_review_queue_both_have_queue_disk_wins() {
        let disk = "# Title\n\n- fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` New q\n  > \n";
        let db = "# Title\n\n- fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Old q\n  > \n";
        assert!(merge_review_queue(disk, db).is_none());
    }

    #[test]
    fn test_merge_review_queue_neither_has_queue() {
        let disk = "# Title\n\n- fact\n";
        let db = "# Title\n\n- old fact\n";
        assert!(merge_review_queue(disk, db).is_none());
    }

    #[test]
    fn test_merge_review_queue_disk_has_queue_db_does_not() {
        let disk = "# Title\n\n<!-- factbase:review -->\n- [ ] q\n";
        let db = "# Title\n\n- fact\n";
        assert!(merge_review_queue(disk, db).is_none());
    }

    #[test]
    fn test_extract_heading_title_found() {
        assert_eq!(
            extract_heading_title("<!-- factbase:abc123 -->\n# My Title\n\n- fact"),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn test_extract_heading_title_none() {
        assert_eq!(extract_heading_title("No heading here\n- fact"), None);
    }

    #[test]
    fn test_extract_heading_title_skips_h2() {
        assert_eq!(extract_heading_title("## Subheading\n- fact"), None);
    }

    #[test]
    fn test_extract_heading_title_cleans_refs() {
        assert_eq!(
            extract_heading_title("# Title [^1]\n"),
            Some("Title".to_string())
        );
    }

    // =========================================================================
    // Frontmatter reviewed date tests
    // =========================================================================

    #[test]
    fn test_extract_frontmatter_reviewed_date_present() {
        let content = "---\nfactbase_id: abc123\nreviewed: 2026-02-20\n---\n# Title\n";
        let date = extract_frontmatter_reviewed_date(content).unwrap();
        assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2026, 2, 20).unwrap());
    }

    #[test]
    fn test_extract_frontmatter_reviewed_date_absent() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n";
        assert!(extract_frontmatter_reviewed_date(content).is_none());
    }

    #[test]
    fn test_extract_frontmatter_reviewed_date_no_frontmatter() {
        let content = "<!-- factbase:abc123 -->\n# Title\n";
        assert!(extract_frontmatter_reviewed_date(content).is_none());
    }

    #[test]
    fn test_set_frontmatter_reviewed_date_existing_frontmatter() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let result = set_frontmatter_reviewed_date(content, &date);
        assert!(result.contains("reviewed: 2026-03-10"));
        assert!(result.contains("factbase_id: abc123"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_set_frontmatter_reviewed_date_update_existing() {
        let content = "---\nfactbase_id: abc123\nreviewed: 2025-01-01\n---\n# Title\n";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let result = set_frontmatter_reviewed_date(content, &date);
        assert!(result.contains("reviewed: 2026-03-10"));
        assert!(!result.contains("2025-01-01"));
    }

    #[test]
    fn test_set_frontmatter_reviewed_date_no_frontmatter() {
        let content = "# Title\n\n- fact\n";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let result = set_frontmatter_reviewed_date(content, &date);
        assert!(result.starts_with("---\nreviewed: 2026-03-10\n---\n"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_convert_inline_reviewed_to_frontmatter() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n\n- Fact one <!-- reviewed:2026-02-15 -->\n- Fact two <!-- reviewed:2026-02-20 -->\n";
        let (cleaned, latest) = convert_inline_reviewed_to_frontmatter(content);
        assert!(!cleaned.contains("<!-- reviewed:"));
        assert!(cleaned.contains("- Fact one"));
        assert!(cleaned.contains("- Fact two"));
        assert_eq!(latest, Some(chrono::NaiveDate::from_ymd_opt(2026, 2, 20).unwrap()));
    }

    #[test]
    fn test_convert_inline_reviewed_no_markers() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n\n- Fact one\n";
        let (cleaned, latest) = convert_inline_reviewed_to_frontmatter(content);
        // Trailing newline may be trimmed by line iteration — content is equivalent
        assert!(cleaned.contains("- Fact one"));
        assert!(cleaned.contains("factbase_id: abc123"));
        assert!(latest.is_none());
    }

    #[test]
    fn test_convert_inline_reviewed_preserves_frontmatter_date() {
        let content = "---\nfactbase_id: abc123\nreviewed: 2026-03-01\n---\n# Title\n\n- Fact <!-- reviewed:2026-02-15 -->\n";
        let (cleaned, latest) = convert_inline_reviewed_to_frontmatter(content);
        assert!(!cleaned.contains("<!-- reviewed:"));
        // Frontmatter date (March) is later than inline (Feb), so it wins
        assert_eq!(latest, Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()));
    }

    // --- Robustness / edge case tests ---

    #[test]
    fn test_content_body_empty_string() {
        assert_eq!(content_body(""), "");
    }

    #[test]
    fn test_content_body_only_review_marker() {
        let content = "<!-- factbase:review -->\n## Review Queue\n- [ ] @q[temporal] When?";
        let body = content_body(content);
        assert!(body.is_empty() || !body.contains("@q["));
    }

    #[test]
    fn test_body_end_offset_no_review() {
        let content = "# Title\n\nSome content.";
        assert_eq!(body_end_offset(content), content.len());
    }

    #[test]
    fn test_body_end_offset_with_callout_review() {
        let content = "# Title\n\nContent.\n\n> [!info]- Review Queue\n> - [ ] @q[temporal] When?\n> <!-- factbase:review -->";
        let offset = body_end_offset(content);
        assert!(offset < content.len());
        assert!(!content[..offset].contains("@q["));
    }

    #[test]
    fn test_fact_line_regex_various_bullets() {
        assert!(FACT_LINE_REGEX.is_match("- Simple fact"));
        assert!(FACT_LINE_REGEX.is_match("* Star bullet fact"));
        assert!(FACT_LINE_REGEX.is_match("  - Indented fact"));
        assert!(!FACT_LINE_REGEX.is_match("# Heading"));
        assert!(!FACT_LINE_REGEX.is_match(""));
        assert!(!FACT_LINE_REGEX.is_match("Plain paragraph text"));
    }

    #[test]
    fn test_doc_id_regex_boundary() {
        assert!(DOC_ID_REGEX.is_match("abcdef"));
        assert!(DOC_ID_REGEX.is_match("000000"));
        assert!(DOC_ID_REGEX.is_match("ffffff"));
        assert!(!DOC_ID_REGEX.is_match("abcde"));   // too short
        assert!(!DOC_ID_REGEX.is_match("abcdefg")); // too long
        assert!(!DOC_ID_REGEX.is_match("ABCDEF"));  // uppercase
        assert!(!DOC_ID_REGEX.is_match("abcdeg"));  // 'g' not hex
    }

    #[test]
    fn test_extract_heading_title_various() {
        assert_eq!(extract_heading_title("# Title"), Some("Title".into()));
        assert_eq!(extract_heading_title("# Title\n\nContent"), Some("Title".into()));
        assert_eq!(extract_heading_title("<!-- factbase:abc123 -->\n# Title"), Some("Title".into()));
        assert_eq!(extract_heading_title("No heading here"), None);
        assert_eq!(extract_heading_title(""), None);
    }

    #[test]
    fn test_clean_title_edge_cases() {
        assert_eq!(clean_title("Title [^1]"), "Title");
        assert_eq!(clean_title("Title"), "Title");
        assert_eq!(clean_title(""), "");
        assert_eq!(clean_title("Title [^1] [^2]"), "Title");
    }

    #[test]
    fn test_has_corruption_artifacts_clean_doc() {
        assert!(!has_corruption_artifacts("# Title\n\n- Normal fact @t[2024]\n"));
    }

    #[test]
    fn test_is_reference_doc_with_marker() {
        assert!(is_reference_doc("<!-- factbase:reference -->\n# Glossary\n\nTerms here."));
        assert!(!is_reference_doc("# Normal Doc\n\nContent."));
    }

    #[test]
    fn test_extract_reviewed_date_invalid() {
        assert!(extract_reviewed_date("- Fact <!-- reviewed:not-a-date -->").is_none());
        assert!(extract_reviewed_date("- Fact without marker").is_none());
    }

    #[test]
    fn test_extract_frontmatter_reviewed_date_missing() {
        assert!(extract_frontmatter_reviewed_date("# Title\n\nContent").is_none());
    }

    #[test]
    fn test_manual_link_regex() {
        assert!(MANUAL_LINK_REGEX.is_match("[[abc123]]"));
        assert!(!MANUAL_LINK_REGEX.is_match("[[not_hex]]"));
    }

    #[test]
    fn test_wikilink_regex() {
        assert!(WIKILINK_REGEX.is_match("[[Some Name]]"));
        assert!(WIKILINK_REGEX.is_match("[[path/to/file|Display Name]]"));
    }
}
