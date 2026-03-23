//! Corruption detection for the review system.
//!
//! Detects document corruption patterns from failed review application runs
//! and other workflows. Generates `@q[corruption]` questions.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{
    body_end_offset, FACT_LINE_REGEX, SOURCE_DEF_REGEX, SOURCE_REF_CAPTURE_REGEX,
    TEMPORAL_TAG_DETECT_REGEX,
};
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
    check_undated_url_citations(content, &mut questions);

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
                    format!("Footnote [^{num}] is defined but never referenced in the document body. Remove it or restore the inline citation."),
                ));
            }
        }
    }
}

/// Detect exact duplicate fact lines.
fn check_duplicate_fact_lines(content: &str, questions: &mut Vec<ReviewQuestion>) {
    let end = body_end_offset(content);
    let body = &content[..end];
    let fm_lines = crate::patterns::frontmatter_line_count(content);

    let mut seen: HashMap<String, usize> = HashMap::new();
    for (line_idx, line) in body.lines().enumerate() {
        if line_idx < fm_lines {
            continue;
        }
        if !FACT_LINE_REGEX.is_match(line) {
            continue;
        }
        let normalized = line.trim().to_string();
        if let Some(first_line) = seen.get(&normalized) {
            questions.push(ReviewQuestion::new(
                QuestionType::Corruption,
                Some(line_idx + 1),
                format!("Duplicate fact line (same as line {first_line})"),
            ));
        } else {
            seen.insert(normalized, line_idx + 1);
        }
    }
}

/// Detect footnote definitions that contain a URL but no date.
/// Web citations without any date indicator cannot contribute to temporal coverage.
///
/// Accepts any recognizable date format: YYYY-MM-DD, YYYY-MM, month name + year,
/// or a standalone year (19xx/20xx). URLs are stripped before the date check so
/// that years embedded in URL paths (e.g. `/2024/03/`) don't count as access dates.
fn check_undated_url_citations(content: &str, questions: &mut Vec<ReviewQuestion>) {
    for (line_idx, line) in content.lines().enumerate() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            let def_text = &cap[2];
            // Only flag footnotes that contain a URL
            if !def_text.contains("http://") && !def_text.contains("https://") {
                continue;
            }
            // Strip URLs before checking for dates so that years in URL paths
            // (e.g. https://example.com/2024/03/post) don't count as access dates.
            let non_url = strip_urls(def_text);
            if !citation_has_date(&non_url) {
                let num = &cap[1];
                questions.push(ReviewQuestion::new(
                    QuestionType::Corruption,
                    Some(line_idx + 1),
                    format!("Footnote [^{num}] has a URL but no date — add 'accessed YYYY-MM-DD'"),
                ));
            }
        }
    }
}

/// Remove all http/https URLs from text, leaving surrounding context intact.
fn strip_urls(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    loop {
        let http_pos = remaining.find("http://");
        let https_pos = remaining.find("https://");
        let url_pos = match (http_pos, https_pos) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        match url_pos {
            None => {
                result.push_str(remaining);
                break;
            }
            Some(pos) => {
                result.push_str(&remaining[..pos]);
                let after = &remaining[pos..];
                let url_len = after
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(after.len());
                remaining = &remaining[pos + url_len..];
            }
        }
    }
    result
}

/// Return true if the text contains any recognizable date indicator.
///
/// Accepts: YYYY-MM-DD, YYYY-MM, month name + year (e.g. "March 2024"),
/// or a standalone year in the range 1900–2099.
fn citation_has_date(text: &str) -> bool {
    use crate::patterns::{MONTH_NAME_REGEX, YEAR_REGEX};
    // YEAR_REGEX matches \b(19|20)\d{2}\b — covers YYYY, YYYY-MM, YYYY-MM-DD
    // (the year portion always has a word boundary before the following '-' or end).
    YEAR_REGEX.is_match(text) || MONTH_NAME_REGEX.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_document_no_corruption() {
        let content = "---\nfactbase_id: abc123\n---\n# Test Entity\n\n- Fact one @t[2024..] [^1]\n- Fact two @t[=2023-06] [^2]\n\n---\n[^1]: Source A, 2024-01-15\n[^2]: Source B, 2023-06-01\n";
        let questions = generate_corruption_questions(content);
        assert!(
            questions.is_empty(),
            "Clean doc should have no corruption: {:?}",
            questions
        );
    }

    #[test]
    fn test_corrupted_title_with_temporal_tag() {
        let content = "# Test Entity @t[?]\n\n- Some fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("Title contains temporal tag")));
    }

    #[test]
    fn test_corrupted_title_with_footnote_ref() {
        let content = "# Test Entity [^1]\n\n- Some fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("Title contains footnote reference")));
    }

    #[test]
    fn test_garbage_footnote_not_a_conflict() {
        let content = "- Fact [^1]\n\n[^1]: Not a conflict, sequential progression\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_garbage_footnote_classification() {
        let content = "- Fact [^1]\n\n[^1]: classification: confirmed\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_legitimate_footnote_not_flagged() {
        let content = "- Fact [^1]\n\n[^1]: LinkedIn profile, scraped 2024-01-15\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions
            .iter()
            .any(|q| q.description.contains("review-answer text")));
    }

    #[test]
    fn test_duplicate_footnote_defs() {
        let content = "- Fact [^1]\n\n[^1]: Source A\n[^1]: Source B\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("defined multiple times")));
    }

    #[test]
    fn test_orphaned_footnote_def() {
        let content = "- Fact without refs\n\n[^5]: Some source\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("never referenced")));
    }

    #[test]
    fn test_referenced_footnote_not_orphaned() {
        let content = "- Fact [^1]\n\n[^1]: Valid source\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions
            .iter()
            .any(|q| q.description.contains("never referenced")));
    }
    #[test]
    fn test_duplicate_fact_lines() {
        let content = "# Title\n\n- Exact same fact\n- Different fact\n- Exact same fact\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("Duplicate fact line")));
    }

    #[test]
    fn test_no_duplicate_for_unique_facts() {
        let content = "# Title\n\n- Fact one\n- Fact two\n- Fact three\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions
            .iter()
            .any(|q| q.description.contains("Duplicate fact line")));
    }

    #[test]
    fn test_h2_title_not_checked() {
        // Only H1 titles should be checked, not H2 section headers
        let content = "# Clean Title\n\n## Section @t[2024..]\n\n- Fact\n";
        let questions = generate_corruption_questions(content);
        assert!(!questions
            .iter()
            .any(|q| q.description.contains("Title contains")));
    }

    #[test]
    fn test_all_corruption_types_detected() {
        let content = "# Bad Title @t[?] [^1]\n\n- Dup fact\n- Dup fact\n\n[^1]: Not a conflict\n[^1]: Duplicate def\n[^9]: Orphaned source\n";
        let questions = generate_corruption_questions(content);
        let descs: Vec<_> = questions.iter().map(|q| q.description.as_str()).collect();
        assert!(
            descs.iter().any(|d| d.contains("temporal tag")),
            "Missing title temporal: {:?}",
            descs
        );
        assert!(
            descs.iter().any(|d| d.contains("footnote reference")),
            "Missing title footnote: {:?}",
            descs
        );
        assert!(
            descs.iter().any(|d| d.contains("Duplicate fact")),
            "Missing dup fact: {:?}",
            descs
        );
        assert!(
            descs.iter().any(|d| d.contains("review-answer text")),
            "Missing garbage footnote: {:?}",
            descs
        );
        assert!(
            descs.iter().any(|d| d.contains("defined multiple times")),
            "Missing dup def: {:?}",
            descs
        );
        assert!(
            descs.iter().any(|d| d.contains("never referenced")),
            "Missing orphaned: {:?}",
            descs
        );
    }

    // === Citation year check removed — matching temporal tag year to citation year
    // is almost always correct behavior (you cite a 2021 source for a 2021 fact).
    // These tests verify no corruption question is generated for such cases. ===

    #[test]
    fn test_temporal_year_matches_citation_year_no_question() {
        // @t[=2021] with a 2021 source: no corruption question
        let content = "# Entity\n\n- Some fact @t[=2021] [^1]\n\n---\n[^1]: Report, 2021\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("citation year")),
            "Should not flag temporal year matching citation year: {:?}",
            questions
        );
    }

    #[test]
    fn test_temporal_month_matches_citation_month_no_question() {
        // @t[2020-11] with a Nov 2020 source: no corruption question
        let content = "# Entity\n\n- Some fact @t[2020-11] [^1]\n\n---\n[^1]: Source, Nov 2020\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("citation year")),
            "Should not flag temporal month matching citation: {:?}",
            questions
        );
    }

    #[test]
    fn test_orphan_footnote_still_flagged() {
        // Orphan footnote check still works
        let content = "- Fact without refs\n\n[^3]: Some source, 2021\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("never referenced")));
    }

    #[test]
    fn test_duplicate_line_still_flagged() {
        // Duplicate fact line check still works
        let content = "# Title\n\n- Same fact @t[=2021] [^1]\n- Same fact @t[=2021] [^1]\n\n[^1]: Source, 2021\n";
        let questions = generate_corruption_questions(content);
        assert!(questions
            .iter()
            .any(|q| q.description.contains("Duplicate fact line")));
    }

    #[test]
    fn test_undated_url_citation_flagged() {
        let content = "- Fact [^1]\n\n[^1]: https://docs.aws.amazon.com/some/page — confirms feature exists\n";
        let questions = generate_corruption_questions(content);
        assert!(
            questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Should flag URL citation without date: {:?}",
            questions
        );
    }

    #[test]
    fn test_dated_url_citation_not_flagged() {
        let content =
            "- Fact [^1]\n\n[^1]: https://docs.aws.amazon.com/some/page, accessed 2026-03-20\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Should not flag URL citation with date: {:?}",
            questions
        );
    }

    #[test]
    fn test_non_url_citation_without_date_not_flagged() {
        // Non-URL citations (books, etc.) are not required to have YYYY-MM-DD
        let content = "- Fact [^1]\n\n[^1]: Herodotus, Histories, Book VII\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Should not flag non-URL citation: {:?}",
            questions
        );
    }

    #[test]
    fn test_dangling_footnote_with_inline_ref_no_question() {
        // [^1]: defined at bottom AND [^1] referenced in body → no dangling footnote
        let content = "- Fact [^1]\n\n---\n[^1]: Valid source, 2024-01-01\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("never referenced")),
            "Should not flag footnote that is referenced: {:?}",
            questions
        );
    }

    #[test]
    fn test_dangling_footnote_no_inline_ref_generates_question() {
        // [^1]: defined at bottom but NO [^1] in body → one dangling_footnote question
        let content = "- Fact with no citation\n\n---\n[^1]: Orphaned source, 2024-01-01\n";
        let questions = generate_corruption_questions(content);
        assert_eq!(
            questions
                .iter()
                .filter(|q| q.description.contains("never referenced"))
                .count(),
            1,
            "Should generate exactly one dangling footnote question: {:?}",
            questions
        );
        assert!(questions[0]
            .description
            .contains("Remove it or restore the inline citation"));
    }

    // === False-positive regression tests for check_undated_url_citations ===
    // These patterns were generating ~85% false positives because the old check
    // only accepted YYYY-MM-DD and didn't strip URL paths before checking.

    #[test]
    fn test_url_citation_with_year_month_not_flagged() {
        // YYYY-MM format is a valid date indicator
        let content = "- Fact [^1]\n\n[^1]: https://example.com/page, accessed 2024-03\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "YYYY-MM should be accepted as a date: {:?}",
            questions
        );
    }

    #[test]
    fn test_url_citation_with_month_name_not_flagged() {
        // "March 2024" is a valid date indicator
        let content = "- Fact [^1]\n\n[^1]: https://example.com/page, March 2024\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Month name + year should be accepted as a date: {:?}",
            questions
        );
    }

    #[test]
    fn test_url_citation_with_year_only_not_flagged() {
        // A standalone year is a valid date indicator
        let content = "- Fact [^1]\n\n[^1]: https://example.com/page, retrieved 2024\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Standalone year should be accepted as a date: {:?}",
            questions
        );
    }

    #[test]
    fn test_url_with_year_in_path_but_no_access_date_flagged() {
        // Year in URL path (/2024/03/) must NOT count as an access date
        let content =
            "- Fact [^1]\n\n[^1]: https://example.com/2024/03/article — confirms feature\n";
        let questions = generate_corruption_questions(content);
        assert!(
            questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Year in URL path should not count as access date: {:?}",
            questions
        );
    }

    #[test]
    fn test_url_with_year_in_path_plus_access_date_not_flagged() {
        // Year in URL path + explicit access date → should not flag
        let content =
            "- Fact [^1]\n\n[^1]: https://example.com/2024/03/article, accessed 2024-03\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "URL with path year + access date should not be flagged: {:?}",
            questions
        );
    }

    #[test]
    fn test_url_citation_abbreviated_month_not_flagged() {
        // "Jan 2024" style — covered by MONTH_NAME_REGEX
        let content = "- Fact [^1]\n\n[^1]: https://example.com/page, Jan 2024\n";
        let questions = generate_corruption_questions(content);
        assert!(
            !questions
                .iter()
                .any(|q| q.description.contains("accessed YYYY-MM-DD")),
            "Abbreviated month + year should be accepted: {:?}",
            questions
        );
    }
}
