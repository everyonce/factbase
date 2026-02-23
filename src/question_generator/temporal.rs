//! Temporal question generation.
//!
//! Generates `@q[temporal]` questions for facts missing temporal tags
//! or with stale ongoing roles.

use chrono::{Datelike, NaiveDate, Utc};

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{ONGOING_TAG_REGEX, TEMPORAL_TAG_DETECT_REGEX, TEMPORAL_TAG_FULL_REGEX};

use super::iter_fact_lines;

/// Generate temporal questions for a document.
///
/// Detects:
/// 1. Facts (list items) without any `@t[...]` tags
/// 2. Ongoing roles (`@t[YYYY..]`) older than 1 year that may have ended
///
/// Returns a list of `ReviewQuestion` with `question_type = Temporal`.
pub fn generate_temporal_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let current_year = Utc::now().year();
    let today = Utc::now().date_naive();

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        if !TEMPORAL_TAG_DETECT_REGEX.is_match(line) {
            // No temporal tag at all
            questions.push(ReviewQuestion::new(
                QuestionType::Temporal,
                Some(line_number),
                format!("\"{}\" - when was this true?", fact_text),
            ));
        } else if let Some(cap) = ONGOING_TAG_REGEX.captures(line) {
            // Check for stale ongoing tags
            let start_date = &cap[1];
            if is_stale_ongoing(start_date, current_year) && !has_recent_verification(line, today) {
                questions.push(ReviewQuestion::new(
                    QuestionType::Temporal,
                    Some(line_number),
                    format!(
                        "\"{}\" has @t[{}..] - is this role still current?",
                        fact_text, start_date
                    ),
                ));
            }
        }
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
    for cap in TEMPORAL_TAG_FULL_REGEX.captures_iter(line) {
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
        let questions = generate_temporal_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_temporal_questions_fact_without_tag() {
        let content = "# Person\n\n- Works at Acme Corp";
        let questions = generate_temporal_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Temporal);
        assert_eq!(questions[0].line_ref, Some(3));
        assert!(questions[0].description.contains("Works at Acme Corp"));
        assert!(questions[0].description.contains("when was this true?"));
    }

    #[test]
    fn test_generate_temporal_questions_fact_with_tag() {
        let content = "# Person\n\n- Works at Acme Corp @t[2020..]";
        let questions = generate_temporal_questions(content);
        // Should generate stale ongoing question if >1 year old
        // (depends on current year)
        for q in &questions {
            assert_eq!(q.question_type, QuestionType::Temporal);
        }
    }

    #[test]
    fn test_generate_temporal_questions_multiple_facts() {
        let content = "# Person\n\n- Fact one\n- Fact two @t[2024]\n- Fact three";
        let questions = generate_temporal_questions(content);
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
        let questions = generate_temporal_questions(content);
        // Should generate a stale ongoing question
        assert!(!questions.is_empty());
        assert!(questions[0].description.contains("still current?"));
    }

    #[test]
    fn test_generate_temporal_questions_recent_ongoing() {
        // Use current year - should not be stale
        let current_year = Utc::now().year();
        let content = format!("# Person\n\n- CTO at Acme @t[{}..]", current_year);
        let questions = generate_temporal_questions(&content);
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
        let questions = generate_temporal_questions(content);
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
        let questions = generate_temporal_questions(content);
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
        let questions = generate_temporal_questions(content);
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
        let questions = generate_temporal_questions(content);
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
}
