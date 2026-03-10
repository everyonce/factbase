//! Temporal question generation.
//!
//! Generates `@q[temporal]` questions for facts missing temporal tags
//! or with stale ongoing roles.

use chrono::{Datelike, NaiveDate, Utc};

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{extract_frontmatter_reviewed_date, extract_reviewed_date, MALFORMED_TAG_REGEX, ONGOING_TAG_REGEX, SOURCE_REF_DETECT_REGEX,
    TEMPORAL_TAG_FULL_REGEX,
};
use crate::processor::{find_malformed_tags, normalize_temporal_tags, line_has_temporal_tag};

use super::iter_fact_lines;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Generate temporal questions for a document.
///
/// Detects:
/// 1. Facts (list items) without any `@t[...]` tags
/// 2. Ongoing roles (`@t[YYYY..]`) older than 1 year that may have ended
///
/// Returns a list of `ReviewQuestion` with `question_type = Temporal`.
///
/// `doc_type` is used to provide confidence signals — e.g., facts in
/// definition/glossary documents are flagged as low-confidence candidates
/// since they often describe stable concepts.
pub fn generate_temporal_questions(content: &str, doc_type: Option<&str>) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let current_year = Utc::now().year();
    let today = Utc::now().date_naive();

    let is_definition_type = doc_type.is_some_and(|t| {
        matches!(t, "definition" | "glossary" | "reference")
    });

    // Check frontmatter for document-level reviewed date (obsidian format)
    let fm_skip = extract_frontmatter_reviewed_date(content)
        .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS);

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with a recent reviewed marker (inline or frontmatter)
        if fm_skip
            || extract_reviewed_date(line)
                .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        if !line_has_temporal_tag(line) {
            if MALFORMED_TAG_REGEX.is_match(line) {
                // Has a malformed tag — find_malformed_tags will flag it below
                continue;
            }
            // No temporal tag at all
            let mut q = ReviewQuestion::new(
                QuestionType::Temporal,
                Some(line_number),
                format!("\"{fact_text}\" - when was this true?"),
            );
            // Provide confidence signals for the agent
            if is_definition_type {
                q = q.with_confidence("low", "fact in definition/glossary document");
            } else if SOURCE_REF_DETECT_REGEX.is_match(line) {
                q = q.with_confidence("low", "fact has source citation — may be an evergreen description");
            }
            questions.push(q);
        } else {
            let normalized = normalize_temporal_tags(line);
            if let Some(cap) = ONGOING_TAG_REGEX.captures(&normalized) {
                // Check for stale ongoing tags
                let start_date = &cap[1];
                if is_stale_ongoing(start_date, current_year)
                    && !has_recent_verification(line, today)
                {
                    questions.push(ReviewQuestion::new(
                        QuestionType::Temporal,
                        Some(line_number),
                        format!(
                            "\"{fact_text}\" has @t[{start_date}..] - is this role still current?"
                        ),
                    ));
                }
            }
        }
    }

    // Flag malformed temporal tags (e.g., @t[~2025-10..~2026-01])
    // Truncate at review queue marker to avoid flagging tags inside review entries
    let body = &content[..crate::patterns::body_end_offset(content)];
    for (line_number, raw) in find_malformed_tags(body) {
        questions.push(ReviewQuestion::new(
            QuestionType::Temporal,
            Some(line_number),
            format!("Malformed temporal tag {raw} — see docs for valid syntax"),
        ));
    }

    questions
}

/// Check if an ongoing tag is stale (started more than 1 year ago).
fn is_stale_ongoing(start_date: &str, current_year: i32) -> bool {
    // Parse the start year from the date string
    let start_year: i32 = start_date
        .split('-')
        .next()
        .and_then(|y| y.parse().ok())
        .unwrap_or(current_year);

    // Consider stale if started more than 1 year ago
    current_year - start_year > 1
}

/// Check if a line has a recent `@t[~DATE]` verification tag (within 180 days).
pub(crate) fn has_recent_verification(line: &str, today: NaiveDate) -> bool {
    let normalized = normalize_temporal_tags(line);
    for cap in TEMPORAL_TAG_FULL_REGEX.captures_iter(&normalized) {
        if cap.get(1).map(|m| m.as_str()) == Some("~") {
            if let Some(date_str) = cap.get(2).map(|m| m.as_str()) {
                if let Some(date) = parse_verification_date(date_str) {
                    if (today - date).num_days() <= 180 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Parse a date string (YYYY, YYYY-MM, YYYY-MM-DD, YYYY-QN) into a NaiveDate.
/// Uses the latest day in the period for generous interpretation.
fn parse_verification_date(date_str: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = date_str.split('-').collect();
    let year: i32 = parts.first()?.parse().ok()?;
    match parts.get(1) {
        None => NaiveDate::from_ymd_opt(year, 12, 31),
        Some(m) if m.starts_with('Q') => {
            let q: u32 = m[1..].parse().ok()?;
            let month = q * 3;
            let day = if month == 6 || month == 9 { 30 } else { 31 };
            NaiveDate::from_ymd_opt(year, month, day)
        }
        Some(month_str) => {
            let month: u32 = month_str.parse().ok()?;
            match parts.get(2) {
                Some(day_str) => {
                    let day: u32 = day_str.parse().ok()?;
                    NaiveDate::from_ymd_opt(year, month, day)
                }
                None => {
                    // Last day of month
                    let next = if month == 12 {
                        NaiveDate::from_ymd_opt(year + 1, 1, 1)
                    } else {
                        NaiveDate::from_ymd_opt(year, month + 1, 1)
                    };
                    next.and_then(|d| d.pred_opt())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_generate_temporal_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        let questions = generate_temporal_questions(content, None);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_temporal_questions_fact_without_tag() {
        let content = "# Person\n\n- Works at Acme Corp";
        let questions = generate_temporal_questions(content, None);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Temporal);
        assert_eq!(questions[0].line_ref, Some(3));
        assert!(questions[0].description.contains("Works at Acme Corp"));
        assert!(questions[0].description.contains("when was this true?"));
    }

    #[test]
    fn test_generate_temporal_questions_fact_with_tag() {
        let content = "# Person\n\n- Works at Acme Corp @t[2020..]";
        let questions = generate_temporal_questions(content, None);
        // Should generate stale ongoing question if >1 year old
        // (depends on current year)
        for q in &questions {
            assert_eq!(q.question_type, QuestionType::Temporal);
        }
    }

    #[test]
    fn test_generate_temporal_questions_multiple_facts() {
        let content = "# Person\n\n- Fact one\n- Fact two @t[2024]\n- Fact three";
        let questions = generate_temporal_questions(content, None);
        // Should have questions for facts without tags (line 3 and 5)
        let without_tag: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("when was this true?"))
            .collect();
        assert_eq!(without_tag.len(), 2);
    }

    #[test]
    fn test_generate_temporal_questions_stale_ongoing() {
        // Use a date that's definitely >1 year old
        let content = "# Person\n\n- CTO at Acme @t[2020..]";
        let questions = generate_temporal_questions(content, None);
        // Should generate a stale ongoing question
        assert!(!questions.is_empty());
        assert!(questions[0].description.contains("still current?"));
    }

    #[test]
    fn test_generate_temporal_questions_recent_ongoing() {
        // Use current year - should not be stale
        let current_year = Utc::now().year();
        let content = format!("# Person\n\n- CTO at Acme @t[{}..]", current_year);
        let questions = generate_temporal_questions(&content, None);
        // Should not generate stale question for current year
        let stale_questions: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("still current?"))
            .collect();
        assert!(stale_questions.is_empty());
    }

    #[test]
    fn test_generate_temporal_questions_line_numbers() {
        let content = "# Title\n\nParagraph\n\n- Fact one\n- Fact two";
        let questions = generate_temporal_questions(content, None);
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].line_ref, Some(5));
        assert_eq!(questions[1].line_ref, Some(6));
    }

    #[test]
    fn test_is_stale_ongoing_old() {
        assert!(is_stale_ongoing("2020", 2026));
        assert!(is_stale_ongoing("2020-03", 2026));
        assert!(is_stale_ongoing("2020-Q2", 2026));
    }

    #[test]
    fn test_is_stale_ongoing_recent() {
        let current_year = Utc::now().year();
        assert!(!is_stale_ongoing(&current_year.to_string(), current_year));
        assert!(!is_stale_ongoing(
            &(current_year - 1).to_string(),
            current_year
        ));
    }

    // --- Task 13.1 tests ---

    #[test]
    fn test_stale_ongoing_with_recent_verification_suppressed() {
        // @t[2024..] is stale, but @t[~2026-01] is within 180 days → no question
        let content = "# Person\n\n- CTO at Acme @t[2024..] @t[~2026-01]";
        let questions = generate_temporal_questions(content, None);
        let stale: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("still current?"))
            .collect();
        assert!(stale.is_empty());
    }

    #[test]
    fn test_stale_ongoing_without_verification_still_generates() {
        // @t[2024..] is stale, no @t[~] → should generate question
        let content = "# Person\n\n- CTO at Acme @t[2024..]";
        let questions = generate_temporal_questions(content, None);
        let stale: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("still current?"))
            .collect();
        assert_eq!(stale.len(), 1);
    }

    #[test]
    fn test_stale_ongoing_with_old_verification_still_generates() {
        // @t[2024..] is stale, @t[~2024-06] is >180 days old → should still generate
        let content = "# Person\n\n- CTO at Acme @t[2024..] @t[~2024-06]";
        let questions = generate_temporal_questions(content, None);
        let stale: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("still current?"))
            .collect();
        assert_eq!(stale.len(), 1);
    }

    #[test]
    fn test_has_recent_verification_recent() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();
        assert!(has_recent_verification(
            "- CTO @t[2024..] @t[~2026-01]",
            today
        ));
    }

    #[test]
    fn test_has_recent_verification_old() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();
        assert!(!has_recent_verification(
            "- CTO @t[2024..] @t[~2024-06]",
            today
        ));
    }

    #[test]
    fn test_has_recent_verification_none() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 8).unwrap();
        assert!(!has_recent_verification("- CTO @t[2024..]", today));
    }

    #[test]
    fn test_parse_verification_date_formats() {
        assert_eq!(
            parse_verification_date("2026"),
            NaiveDate::from_ymd_opt(2026, 12, 31)
        );
        assert_eq!(
            parse_verification_date("2026-01"),
            NaiveDate::from_ymd_opt(2026, 1, 31)
        );
        assert_eq!(
            parse_verification_date("2026-01-15"),
            NaiveDate::from_ymd_opt(2026, 1, 15)
        );
        assert_eq!(
            parse_verification_date("2026-Q1"),
            NaiveDate::from_ymd_opt(2026, 3, 31)
        );
        assert_eq!(
            parse_verification_date("2025-Q2"),
            NaiveDate::from_ymd_opt(2025, 6, 30)
        );
    }

    #[test]
    fn test_reviewed_marker_suppresses_missing_temporal() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Works at Acme Corp <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_temporal_questions(&content, None);
        assert!(
            questions.is_empty(),
            "Recent reviewed marker should suppress temporal question"
        );
    }

    #[test]
    fn test_old_reviewed_marker_still_generates_temporal() {
        let content = "# Person\n\n- Works at Acme Corp <!-- reviewed:2020-01-01 -->";
        let questions = generate_temporal_questions(content, None);
        assert_eq!(
            questions.len(),
            1,
            "Old reviewed marker should not suppress temporal question"
        );
    }

    #[test]
    fn test_reviewed_marker_suppresses_stale_ongoing() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- CTO at Acme @t[2020..] <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_temporal_questions(&content, None);
        let stale: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("still current?"))
            .collect();
        assert!(
            stale.is_empty(),
            "Recent reviewed marker should suppress stale ongoing question"
        );
    }

    #[test]
    fn test_malformed_nondate_tags_flagged() {
        // Non-date content inside @t[...] should produce malformed tag questions
        let cases = [
            "- Domesticated @t[traditional..modern]",
            "- Used since @t[domestication..present]",
            "- Status @t[static]",
            "- Method @t[traditional]",
        ];
        for case in cases {
            let content = format!("# Doc\n\n{case}");
            let questions = generate_temporal_questions(&content, None);
            let malformed: Vec<_> = questions
                .iter()
                .filter(|q| q.description.contains("Malformed"))
                .collect();
            assert_eq!(
                malformed.len(),
                1,
                "Expected malformed question for: {case}"
            );
            // Should NOT also generate "when was this true?" (malformed is sufficient)
            let missing: Vec<_> = questions
                .iter()
                .filter(|q| q.description.contains("when was this true?"))
                .collect();
            assert!(
                missing.is_empty(),
                "Should not generate 'when was this true?' for malformed tag: {case}"
            );
        }
    }

    #[test]
    fn test_valid_tags_not_flagged_malformed() {
        let content = "# Doc\n\n- Fact @t[2024]\n- Range @t[2020..2023]\n- Unknown @t[?]";
        let questions = generate_temporal_questions(content, None);
        let malformed: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("Malformed"))
            .collect();
        assert!(malformed.is_empty(), "Valid tags should not be flagged");
    }

    #[test]
    fn test_bce_tags_not_flagged_as_missing_temporal() {
        let content =
            "# Doc\n\n- Battle @t[=331 BCE]\n- Reign @t[336 BCE..323 BCE]\n- Event @t[=-0490]";
        let questions = generate_temporal_questions(content, None);
        let missing: Vec<_> = questions
            .iter()
            .filter(|q| q.description.contains("when was this true?"))
            .collect();
        assert!(
            missing.is_empty(),
            "BCE-tagged facts should not be flagged as missing temporal tags"
        );
    }

    #[test]
    fn test_confidence_none_by_default() {
        let content = "# Doc\n\n- Unsourced fact without temporal tag";
        let questions = generate_temporal_questions(content, None);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].confidence.is_none());
        assert!(questions[0].confidence_reason.is_none());
    }

    #[test]
    fn test_confidence_low_for_sourced_fact() {
        let content = "# Doc\n\n- Sourced fact [^1]\n\n---\n[^1]: Some source";
        let questions = generate_temporal_questions(content, None);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].confidence.as_deref(), Some("low"));
        assert!(questions[0].confidence_reason.as_ref().unwrap().contains("source citation"));
    }

    #[test]
    fn test_confidence_low_for_definition_doc_type() {
        let content = "# Glossary\n\n- Term means something";
        let questions = generate_temporal_questions(content, Some("definition"));
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].confidence.as_deref(), Some("low"));
        assert!(questions[0].confidence_reason.as_ref().unwrap().contains("definition"));
    }

    #[test]
    fn test_confidence_low_for_glossary_doc_type() {
        let content = "# Terms\n\n- Another term";
        let questions = generate_temporal_questions(content, Some("glossary"));
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].confidence.as_deref(), Some("low"));
    }

    #[test]
    fn test_confidence_low_for_reference_doc_type() {
        let content = "# Ref\n\n- Reference fact";
        let questions = generate_temporal_questions(content, Some("reference"));
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].confidence.as_deref(), Some("low"));
    }

    #[test]
    fn test_confidence_definition_takes_precedence_over_source() {
        // When both doc_type=definition AND fact has source, definition reason wins
        let content = "# Glossary\n\n- Term means something [^1]\n\n---\n[^1]: Docs";
        let questions = generate_temporal_questions(content, Some("definition"));
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].confidence.as_deref(), Some("low"));
        assert!(questions[0].confidence_reason.as_ref().unwrap().contains("definition"));
    }

    #[test]
    fn test_confidence_none_for_stale_ongoing() {
        // Stale ongoing questions should not have low confidence
        let content = "# Doc\n\n- Role at Company @t[2020..]";
        let questions = generate_temporal_questions(content, None);
        let stale: Vec<_> = questions.iter().filter(|q| q.description.contains("still current")).collect();
        for q in &stale {
            assert!(q.confidence.is_none(), "stale ongoing questions should have no confidence override");
        }
    }

    #[test]
    fn test_confidence_none_for_regular_doc_type() {
        let content = "# Person\n\n- Some fact without tag";
        let questions = generate_temporal_questions(content, Some("person"));
        assert_eq!(questions.len(), 1);
        assert!(questions[0].confidence.is_none(), "regular doc types should not get low confidence");
    }
}
