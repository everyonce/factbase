//! Corruption detection for the review system.
//!
//! Detects document corruption patterns from failed review application runs
//! and other workflows. Generates `@q[corruption]` questions.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{
    body_end_offset, FACT_LINE_REGEX, SOURCE_DEF_REGEX, SOURCE_REF_CAPTURE_REGEX,
    TEMPORAL_TAG_CONTENT_REGEX, TEMPORAL_TAG_DETECT_REGEX, YEAR_REGEX,
};
use chrono::{Datelike, Utc};
use std::collections::{HashMap, HashSet};

/// Phrases in footnote definitions that indicate review-answer text was dumped
/// as a source citation. These are structural indicators, not domain-specific.
const GARBAGE_FOOTNOTE_PHRASES: &[&str] = &[
    "not a conflict",
    "sequential progression",
    "unable to verify",
    "no conflict",
    "confirmed correct",
    "this is correct",
    "already addressed",
    "no action needed",
    "no change needed",
    "appears accurate",
    "verified correct",
    "overlapping roles",
    "concurrent positions",
    "no issue found",
    "classification:",
    "answer_type:",
    "change_instruction",
];

/// Generate corruption questions for a document.
pub fn generate_corruption_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();

    check_corrupted_title(content, &mut questions);
    check_garbage_footnotes(content, &mut questions);
    check_duplicate_footnote_defs(content, &mut questions);
    check_orphaned_footnote_defs(content, &mut questions);
    check_duplicate_fact_lines(content, &mut questions);
    check_citation_year_as_temporal(content, &mut questions, Utc::now().year());

    questions
}

/// Detect titles with `@t[...]` tags or `[^N]` footnote references appended.
fn check_corrupted_title(content: &str, questions: &mut Vec<ReviewQuestion>) {
    for line in content.lines() {
        if line.starts_with("# ") && !line.starts_with("## ") {
            let title = &line[2..];
            if TEMPORAL_TAG_DETECT_REGEX.is_match(title) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Corruption,
                    None,
                    "Title contains temporal tag — likely corrupted by apply".to_string(),
                ));
            }
            if SOURCE_REF_CAPTURE_REGEX.is_match(title) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Corruption,
                    None,
                    "Title contains footnote reference — likely corrupted by apply".to_string(),
                ));
            }
            break; // Only check first H1
        }
    }
}

/// Detect footnote definitions that contain review-answer text instead of source citations.
fn check_garbage_footnotes(content: &str, questions: &mut Vec<ReviewQuestion>) {
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let def_text = cap[2].to_lowercase();
            for phrase in GARBAGE_FOOTNOTE_PHRASES {
                if def_text.contains(phrase) {
                    let num = &cap[1];
                    questions.push(ReviewQuestion::new(
                        QuestionType::Corruption,
                        Some(line_idx + 1),
                        format!(
                            "Footnote [^{num}] contains review-answer text, not a source citation"
                        ),
                    ));
                    break;
                }
            }
        }
    }
}

/// Detect the same footnote number defined multiple times.
fn check_duplicate_footnote_defs(content: &str, questions: &mut Vec<ReviewQuestion>) {
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            if let Some(first_line) = seen.get(&num) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Corruption,
                    Some(line_idx + 1),
                    format!(
                        "Footnote [^{num}] defined multiple times (first at line {first_line})"
                    ),
                ));
            } else {
                seen.insert(num, line_idx + 1);
            }
        }
    }
}

/// Detect footnote definitions whose numbers are never referenced in the document body.
fn check_orphaned_footnote_defs(content: &str, questions: &mut Vec<ReviewQuestion>) {
    let end = body_end_offset(content);
    let body = &content[..end];

    // Collect all referenced footnote numbers from body text (excluding def lines)
    let mut referenced: HashSet<u32> = HashSet::new();
    for line in body.lines() {
        if SOURCE_DEF_REGEX.is_match(line) {
            continue; // Skip definition lines
        }
        for cap in SOURCE_REF_CAPTURE_REGEX.captures_iter(line) {
            if let Ok(num) = cap[1].parse::<u32>() {
                referenced.insert(num);
            }
        }
    }

    // Check each definition against referenced set
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            if !referenced.contains(&num) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Corruption,
                    Some(line_idx + 1),
                    format!("Footnote [^{num}] is defined but never referenced in document body"),
                ));
            }
        }
    }
}

/// Detect exact duplicate fact lines.
fn check_duplicate_fact_lines(content: &str, questions: &mut Vec<ReviewQuestion>) {
    let end = body_end_offset(content);
    let body = &content[..end];

    let mut seen: HashMap<String, usize> = HashMap::new();
    for (line_idx, line) in body.lines().enumerate() {
        if !FACT_LINE_REGEX.is_match(line) {
            continue;
        }
        let normalized = line.trim().to_string();
        if let Some(first_line) = seen.get(&normalized) {
            questions.push(ReviewQuestion::new(
                QuestionType::Corruption,
                Some(line_idx + 1),
                format!(
                    "Duplicate fact line (same as line {first_line})"
                ),
            ));
        } else {
            seen.insert(normalized, line_idx + 1);
        }
    }
}

/// Detect temporal tags whose year matches a year in the cited footnote definition.
/// This pattern suggests the author accidentally used the citation/publication year
/// as the temporal date instead of the actual historical date.
///
/// Only flags bare-year tags (e.g. `@t[~1991]`, `@t[=2024]`) — not ranges or
/// month-precision tags, which indicate intentional dating.
fn check_citation_year_as_temporal(content: &str, questions: &mut Vec<ReviewQuestion>, current_year: i32) {
    // Build map of footnote number -> set of years in definition text
    let mut footnote_years: HashMap<u32, HashSet<String>> = HashMap::new();
    for line in content.lines() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let num: u32 = cap[1].parse().unwrap_or(0);
            let years: HashSet<String> = YEAR_REGEX
                .find_iter(&cap[2])
                .map(|m| m.as_str().to_string())
                .collect();
            if !years.is_empty() {
                footnote_years.insert(num, years);
            }
        }
    }
    if footnote_years.is_empty() {
        return;
    }

    let end = body_end_offset(content);
    let body = &content[..end];

    for (line_idx, line) in body.lines().enumerate() {
        if !TEMPORAL_TAG_CONTENT_REGEX.is_match(line) {
            continue;
        }
        // Extract bare-year tags only: content matches [=~]?YYYY exactly
        let tag_years: HashSet<String> = TEMPORAL_TAG_CONTENT_REGEX
            .captures_iter(line)
            .filter_map(|cap| {
                let inner = &cap[1];
                // ~YYYY means "last verified", =YYYY means "as of" — in both
                // cases the temporal tag year naturally matches the source year
                // (you verify/observe a fact and record the source on the same date).
                // Only flag bare YYYY with no prefix, which is more likely a
                // copy-paste error from the source citation.
                if inner.starts_with('~') || inner.starts_with('=') {
                    return None;
                }
                // Bare year: only a 4-digit modern year with no prefix
                if YEAR_REGEX.is_match(inner) && inner.len() == 4 {
                    Some(inner.to_string())
                } else {
                    None
                }
            })
            .collect();
        if tag_years.is_empty() {
            continue;
        }
        // Check each footnote ref on this line
        for ref_cap in SOURCE_REF_CAPTURE_REGEX.captures_iter(line) {
            let num: u32 = ref_cap[1].parse().unwrap_or(0);
            if let Some(def_years) = footnote_years.get(&num) {
                for year in tag_years.intersection(def_years) {
                    // Suppress for recent years: a fact tagged with the current or
                    // previous year sourced from that same year is expected — the
                    // source is contemporaneous with the observation.
                    if let Ok(y) = year.parse::<i32>() {
                        if y >= current_year - 1 {
                            continue;
                        }
                    }
                    questions.push(ReviewQuestion::new(
                        QuestionType::Corruption,
                        Some(line_idx + 1),
                        format!(
                            "Temporal tag year {year} matches footnote [^{num}] citation year — \
                             verify this is the intended date, not a copy-paste from the source"
                        ),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_document_no_corruption() {
        let content = "<!-- factbase:abc123 -->\n# Test Entity\n\n- Fact one @t[2024..] [^1]\n- Fact two @t[=2023-06] [^2]\n\n---\n[^1]: Source A, 2024-01-15\n[^2]: Source B, 2023-06-01\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.is_empty(), "Clean doc should have no corruption: {:?}", questions);
    }

    #[test]
    fn test_corrupted_title_with_temporal_tag() {
        let content = "# Test Entity @t[?]\n\n- Some fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("Title contains temporal tag")));
    }

    #[test]
    fn test_corrupted_title_with_footnote_ref() {
        let content = "# Test Entity [^1]\n\n- Some fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("Title contains footnote reference")));
    }

    #[test]
    fn test_garbage_footnote_not_a_conflict() {
        let content = "- Fact [^1]\n\n[^1]: Not a conflict, sequential progression\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_garbage_footnote_classification() {
        let content = "- Fact [^1]\n\n[^1]: classification: confirmed\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_legitimate_footnote_not_flagged() {
        let content = "- Fact [^1]\n\n[^1]: LinkedIn profile, scraped 2024-01-15\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_duplicate_footnote_defs() {
        let content = "- Fact [^1]\n\n[^1]: Source A\n[^1]: Source B\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("defined multiple times")));
    }

    #[test]
    fn test_orphaned_footnote_def() {
        let content = "- Fact without refs\n\n[^5]: Some source\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("never referenced")));
    }

    #[test]
    fn test_referenced_footnote_not_orphaned() {
        let content = "- Fact [^1]\n\n[^1]: Valid source\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("never referenced")));
    }

    #[test]
    fn test_duplicate_fact_lines() {
        let content = "# Title\n\n- Exact same fact\n- Different fact\n- Exact same fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("Duplicate fact line")));
    }

    #[test]
    fn test_no_duplicate_for_unique_facts() {
        let content = "# Title\n\n- Fact one\n- Fact two\n- Fact three\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("Duplicate fact line")));
    }

    #[test]
    fn test_h2_title_not_checked() {
        // Only H1 titles should be checked, not H2 section headers
        let content = "# Clean Title\n\n## Section @t[2024..]\n\n- Fact\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("Title contains")));
    }

    #[test]
    fn test_all_corruption_types_detected() {
        let content = "# Bad Title @t[?] [^1]\n\n- Dup fact\n- Dup fact\n\n[^1]: Not a conflict\n[^1]: Duplicate def\n[^9]: Orphaned source\n";
        let questions = generate_corruption_questions(content);
        let descs: Vec<_> = questions.iter().map(|q| q.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("temporal tag")), "Missing title temporal: {:?}", descs);
        assert!(descs.iter().any(|d| d.contains("footnote reference")), "Missing title footnote: {:?}", descs);
        assert!(descs.iter().any(|d| d.contains("Duplicate fact")), "Missing dup fact: {:?}", descs);
        assert!(descs.iter().any(|d| d.contains("review-answer text")), "Missing garbage footnote: {:?}", descs);
        assert!(descs.iter().any(|d| d.contains("defined multiple times")), "Missing dup def: {:?}", descs);
        assert!(descs.iter().any(|d| d.contains("never referenced")), "Missing orphaned: {:?}", descs);
    }

    // === Citation year as temporal tag tests ===

    #[test]
    fn test_citation_year_matches_temporal_tag() {
        // Bare YYYY tag (no prefix) matching footnote year should be flagged
        let content = "# Entity\n\n- Some fact @t[1991] [^2]\n\n---\n[^2]: Book published 1991\n";
        let questions = generate_corruption_questions(content);
        assert!(
            questions.iter().any(|q| q.description.contains("Temporal tag year 1991 matches footnote [^2]")),
            "Should flag citation year match: {:?}", questions.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_citation_year_equals_prefix_not_flagged() {
        // =YYYY means "as of" — matching the source year is expected
        let content = "# Entity\n\n- Some fact @t[=2024] [^1]\n\n---\n[^1]: Report, 2024\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions.iter().any(|q| q.description.contains("citation year")),
            "Should not flag =YYYY tags: {:?}", questions.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_citation_year_approximate_month_precision_not_flagged() {
        // ~YYYY-MM should also be suppressed (not just ~YYYY)
        let content = "# Entity\n\n- Current role @t[~2026-02] [^1]\n\n---\n[^1]: Lookup, 2026-02-27\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions.iter().any(|q| q.description.contains("citation year")),
            "Should not flag ~YYYY-MM tags: {:?}", questions.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_citation_year_approximate_tag_not_flagged() {
        // ~YYYY means "last verified" — matching the source year is expected
        let content = "# Entity\n\n- Current role @t[~2026] [^1]\n\n---\n[^1]: Scraped 2026-02\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions.iter().any(|q| q.description.contains("citation year")),
            "Should not flag ~YYYY tags: {:?}", questions.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_citation_year_no_match_different_years() {
        // Temporal tag year differs from footnote year — no flag
        let content = "# Entity\n\n- Ruled from here @t[~323] [^1]\n\n---\n[^1]: Source, 1991\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions.iter().any(|q| q.description.contains("citation year")),
            "Should not flag non-matching years: {:?}", questions.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_citation_year_no_footnote_ref_on_line() {
        // Temporal tag but no footnote ref on the same line
        let content = "# Entity\n\n- Some fact @t[~1991]\n\n---\n[^1]: Source, 1991\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("citation year")));
    }

    #[test]
    fn test_citation_year_no_year_in_footnote() {
        // Footnote has no year at all
        let content = "# Entity\n\n- Some fact @t[=2024] [^1]\n\n---\n[^1]: Personal interview\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("citation year")));
    }

    #[test]
    fn test_citation_year_bce_tag_not_flagged() {
        // BCE/negative year in temporal tag can't match a modern citation year
        let content = "# Entity\n\n- Ancient event @t[=-330] [^1]\n\n---\n[^1]: Source, 2020\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("citation year")));
    }

    #[test]
    fn test_citation_year_range_tag_not_flagged() {
        // Range tags are not bare years — not flagged
        let content = "# Entity\n\n- Active period @t[1995..2003] [^1]\n\n---\n[^1]: Published 1995\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("citation year")));
    }

    #[test]
    fn test_citation_year_month_precision_not_flagged() {
        // Month-precision tag is not a bare year — not flagged
        let content = "# Entity\n\n- Observed @t[=2024-03] [^1]\n\n---\n[^1]: Report, March 2024\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions.iter().any(|q| q.description.contains("citation year")));
    }

    #[test]
    fn test_citation_year_multiple_footnotes() {
        // Only the matching footnote should be flagged (bare year, no prefix)
        let content = "# Entity\n\n- Fact @t[2005] [^1] [^2]\n\n---\n[^1]: Source A, 2005\n[^2]: Source B, 2010\n";
        let questions = generate_corruption_questions(content);
        assert!(questions.iter().any(|q| q.description.contains("[^1]")));
        assert!(!questions.iter().any(|q| q.description.contains("[^2]")));
    }

    #[test]
    fn test_citation_year_current_year_suppressed() {
        // Bare year matching current year should be suppressed — contemporaneous source
        let current_year = Utc::now().year();
        let content = format!(
            "# Entity\n\n- Fact @t[{current_year}] [^1]\n\n---\n[^1]: Lookup, {current_year}-02-10\n"
        );
        let mut questions = Vec::new();
        check_citation_year_as_temporal(&content, &mut questions, current_year);
        assert!(questions.is_empty(), "Current year should be suppressed: {:?}", questions);
    }

    #[test]
    fn test_citation_year_previous_year_suppressed() {
        // Previous year also suppressed — source may have been scraped late last year
        let current_year = Utc::now().year();
        let prev = current_year - 1;
        let content = format!(
            "# Entity\n\n- Fact @t[{prev}] [^1]\n\n---\n[^1]: Report, {prev}-11-30\n"
        );
        let mut questions = Vec::new();
        check_citation_year_as_temporal(&content, &mut questions, current_year);
        assert!(questions.is_empty(), "Previous year should be suppressed: {:?}", questions);
    }

    #[test]
    fn test_citation_year_old_year_still_flagged() {
        // A year well in the past should still be flagged
        let content = "# Entity\n\n- Fact @t[1991] [^1]\n\n---\n[^1]: Book, 1991\n";
        let mut questions = Vec::new();
        check_citation_year_as_temporal(&content, &mut questions, 2026);
        assert!(
            questions.iter().any(|q| q.description.contains("1991")),
            "Old year should still be flagged: {:?}", questions
        );
    }
}
