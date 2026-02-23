//! Stale question generation.
//!
//! Generates `@q[stale]` questions for facts with old sources
//! or old `@t[~...]` dates.

use chrono::{NaiveDate, Utc};
use std::collections::HashMap;

use crate::models::{QuestionType, ReviewQuestion, TemporalTagType};
use crate::patterns::{extract_reviewed_date, FACT_LINE_REGEX};
use crate::processor::{parse_source_definitions, parse_source_references, parse_temporal_tags};

use super::temporal::has_recent_verification;
use super::{extract_fact_text, iter_fact_lines};

/// Generate stale questions for a document.
///
/// Detects facts that may be outdated based on:
/// 1. Source footnote dates older than threshold (default: 365 days)
/// 2. `@t[~...]` (LastSeen) dates older than threshold
///
/// Returns a list of `ReviewQuestion` with `question_type = Stale`.
pub fn generate_stale_questions(content: &str, max_age_days: i64) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let today = Utc::now().date_naive();

    // Truncate at review queue marker — review entries are not document facts
    let body = &content[..crate::patterns::body_end_offset(content)];

    // Parse temporal tags upfront to identify closed ranges
    let tags = parse_temporal_tags(body);

    // Parse source references and definitions
    let refs = parse_source_references(body);
    let defs = parse_source_definitions(body);

    // Build map of footnote number -> (source_type, date, def_line)
    let def_map: HashMap<u32, (&str, Option<&str>, usize)> = defs
        .iter()
        .map(|d| {
            (
                d.number,
                (d.source_type.as_str(), d.date.as_deref(), d.line_number),
            )
        })
        .collect();

    // Build map: line_number -> inherited tag type from nearest preceding ## heading
    let heading_tag_map = build_heading_temporal_map(body, &tags);

    // Check source dates for each fact line
    let lines: Vec<&str> = body.lines().collect();
    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with closed temporal ranges — old sources are expected for historical facts
        let has_closed_range = tags.iter().any(|t| {
            t.line_number == line_number
                && matches!(
                    t.tag_type,
                    TemporalTagType::Range
                        | TemporalTagType::PointInTime
                        | TemporalTagType::Historical
                )
        }) || heading_tag_map.get(&line_number).is_some_and(|tt| {
            matches!(
                tt,
                TemporalTagType::Range | TemporalTagType::PointInTime | TemporalTagType::Historical
            )
        });
        if has_closed_range {
            continue;
        }

        // Find source references on this line
        let line_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.line_number == line_number)
            .collect();

        // Skip facts with a recent reviewed marker
        if extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= max_age_days) {
            continue;
        }

        // Check if any source is stale
        for source_ref in &line_refs {
            if let Some((source_type, Some(date_str), _)) = def_map.get(&source_ref.number) {
                if let Some(days_old) = days_since_date(date_str, today) {
                    if days_old > max_age_days && !has_recent_verification(line, today) {
                        questions.push(ReviewQuestion::new(
                            QuestionType::Stale,
                            Some(line_number),
                            format!(
                                "\"{fact_text}\" - {source_type} source from {date_str} may be outdated, is this still accurate?"
                            ),
                        ));
                        break; // One question per fact line
                    }
                }
            }
        }
    }

    // Check @t[~...] (LastSeen) tags for staleness
    // Skip if verified within 180 days — recently confirmed facts don't need re-asking
    for tag in &tags {
        if tag.tag_type == TemporalTagType::LastSeen {
            if let Some(ref date_str) = tag.start_date {
                if let Some(days_old) = days_since_date(date_str, today) {
                    if days_old > max_age_days && days_old > 180 {
                        // Get the fact text from this line
                        if tag.line_number > 0 && tag.line_number <= lines.len() {
                            let line = lines[tag.line_number - 1];
                            // Skip facts with a recent reviewed marker
                            if extract_reviewed_date(line)
                                .is_some_and(|d| (today - d).num_days() <= max_age_days)
                            {
                                continue;
                            }
                            if FACT_LINE_REGEX.is_match(line) {
                                let fact_text = extract_fact_text(line);
                                // Avoid duplicate if we already have a stale source question for this line
                                let already_has_question = questions
                                    .iter()
                                    .any(|q| q.line_ref == Some(tag.line_number));
                                if !already_has_question {
                                    questions.push(ReviewQuestion::new(
                                        QuestionType::Stale,
                                        Some(tag.line_number),
                                        format!(
                                            "\"{fact_text}\" has @t[~{date_str}] which may be outdated - is this still accurate?"
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    questions
}

/// Calculate days since a date string (YYYY, YYYY-MM, or YYYY-MM-DD).
/// Returns None if the date cannot be parsed.
fn days_since_date(date_str: &str, today: NaiveDate) -> Option<i64> {
    let date = match date_str.len() {
        4 => {
            // YYYY - assume Jan 1
            let year: i32 = date_str.parse().ok()?;
            NaiveDate::from_ymd_opt(year, 1, 1)?
        }
        7 => {
            // YYYY-MM - assume 1st of month
            let year: i32 = date_str[0..4].parse().ok()?;
            let month: u32 = date_str[5..7].parse().ok()?;
            NaiveDate::from_ymd_opt(year, month, 1)?
        }
        10 => {
            // YYYY-MM-DD
            let year: i32 = date_str[0..4].parse().ok()?;
            let month: u32 = date_str[5..7].parse().ok()?;
            let day: u32 = date_str[8..10].parse().ok()?;
            NaiveDate::from_ymd_opt(year, month, day)?
        }
        _ => return None,
    };

    Some((today - date).num_days())
}

/// Build a map from line numbers to the temporal tag type of their enclosing `## Heading`.
/// If a heading has a closed temporal tag (PointInTime, Range, Historical), all fact lines
/// under it inherit that classification until the next heading.
fn build_heading_temporal_map(
    content: &str,
    tags: &[crate::models::TemporalTag],
) -> HashMap<usize, TemporalTagType> {
    let mut map = HashMap::new();
    let mut current_tag_type: Option<TemporalTagType> = None;
    let mut heading_line: usize = 0;

    for (idx, line) in content.lines().enumerate() {
        let line_number = idx + 1;
        if line.starts_with("## ") {
            // Check if this heading has a closed temporal tag
            current_tag_type = tags
                .iter()
                .find(|t| {
                    t.line_number == line_number
                        && matches!(
                            t.tag_type,
                            TemporalTagType::Range
                                | TemporalTagType::PointInTime
                                | TemporalTagType::Historical
                        )
                })
                .map(|t| t.tag_type);
            heading_line = line_number;
        } else if let Some(ref tt) = current_tag_type {
            if line_number > heading_line {
                map.insert(line_number, *tt);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_stale_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        let questions = generate_stale_questions(content, 365);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_stale_questions_old_source() {
        let content =
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2020-01-15";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Stale);
        assert_eq!(questions[0].line_ref, Some(3));
        assert!(questions[0].description.contains("LinkedIn"));
        assert!(questions[0].description.contains("2020-01-15"));
    }

    #[test]
    fn test_generate_stale_questions_recent_source() {
        // Use a recent date that won't be stale
        let today = Utc::now().date_naive();
        let recent_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped {}",
            recent_date.format("%Y-%m-%d")
        );
        let questions = generate_stale_questions(&content, 365);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_stale_questions_old_last_seen_tag() {
        let content = "# Person\n\n- Lives in NYC @t[~2020-06]";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Stale);
        assert!(questions[0].description.contains("@t[~2020-06]"));
        assert!(questions[0].description.contains("outdated"));
    }

    #[test]
    fn test_generate_stale_questions_recent_last_seen_tag() {
        let today = Utc::now().date_naive();
        let recent_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Lives in NYC @t[~{}]",
            recent_date.format("%Y-%m")
        );
        let questions = generate_stale_questions(&content, 365);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_stale_questions_no_source_date() {
        let content = "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile";
        let questions = generate_stale_questions(content, 365);
        // No date in source = no stale question
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_stale_questions_custom_threshold() {
        // Use 30 day threshold
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(60);
        let content = format!(
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped {}",
            old_date.format("%Y-%m-%d")
        );
        let questions = generate_stale_questions(&content, 30);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_generate_stale_questions_line_numbers() {
        let content =
            "# Person\n\nParagraph\n\n- Old fact [^1]\n- Another fact\n\n[^1]: Source, 2020-01-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].line_ref, Some(5));
    }

    #[test]
    fn test_generate_stale_questions_multiple_sources() {
        let content = "# Person\n\n- Fact one [^1]\n- Fact two [^2]\n\n[^1]: Source, 2020-01-01\n[^2]: Source, 2020-02-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 2);
    }

    #[test]
    fn test_generate_stale_questions_avoids_duplicate_for_same_line() {
        // If a line has both old source and old @t[~...], only one question
        let content = "# Person\n\n- Lives in NYC @t[~2020-06] [^1]\n\n[^1]: Source, 2020-01-01";
        let questions = generate_stale_questions(content, 365);
        // Should have one question (source is checked first, then @t[~] skips if already has question)
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_days_since_date_full_date() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        assert_eq!(days_since_date("2025-01-29", today), Some(365));
        assert_eq!(days_since_date("2026-01-29", today), Some(0));
    }

    #[test]
    fn test_days_since_date_year_month() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        // 2025-01 = Jan 1, 2025 = 393 days before Jan 29, 2026
        assert_eq!(days_since_date("2025-01", today), Some(393));
    }

    #[test]
    fn test_days_since_date_year_only() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        // 2025 = Jan 1, 2025 = 393 days before Jan 29, 2026
        assert_eq!(days_since_date("2025", today), Some(393));
    }

    #[test]
    fn test_days_since_date_invalid() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        assert_eq!(days_since_date("invalid", today), None);
        assert_eq!(days_since_date("2025-Q2", today), None); // Quarter format not supported
    }

    #[test]
    fn test_stale_source_suppressed_by_recent_verification() {
        // Old source (800+ days) but recent @t[~2026-01] verification — should NOT generate
        let content = "# Person\n\n- Works at Acme Corp @t[~2026-01] [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        let questions = generate_stale_questions(content, 365);
        assert!(
            questions.is_empty(),
            "Should suppress stale question when line has recent @t[~] verification"
        );
    }

    #[test]
    fn test_stale_source_no_verification_still_generates() {
        // Old source, no @t[~] tag — should still generate
        let content =
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_stale_source_old_verification_still_generates() {
        // Old source AND old @t[~2024-01] verification — should still generate
        let content = "# Person\n\n- Works at Acme Corp @t[~2024-01] [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(
            questions.len(),
            1,
            "Should still generate when @t[~] verification is old"
        );
    }

    #[test]
    fn test_heading_point_in_time_suppresses_stale() {
        // Facts under a heading with @t[=2024-12] should not generate stale questions
        let content = "# Events\n\n## re:Invent 2024 @t[=2024-12]\n\n- Met with John Smith [^1]\n- Attended keynote [^1]\n\n[^1]: Notes, 2024-12-05";
        let questions = generate_stale_questions(content, 365);
        assert!(
            questions.is_empty(),
            "Facts under a PointInTime heading should not be flagged as stale"
        );
    }

    #[test]
    fn test_heading_range_suppresses_stale() {
        let content = "# Events\n\n## Q1 2023 Sprint @t[2023-01..2023-03]\n\n- Delivered feature X [^1]\n\n[^1]: Jira, 2023-03-15";
        let questions = generate_stale_questions(content, 365);
        assert!(
            questions.is_empty(),
            "Facts under a Range heading should not be flagged as stale"
        );
    }

    #[test]
    fn test_heading_no_tag_still_generates() {
        // Facts under a heading WITHOUT temporal tag should still generate
        let content = "# Person\n\n## Current Role\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2020-01-15";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_heading_ongoing_still_generates() {
        // Ongoing heading should NOT suppress stale (it's still current)
        let content = "# Person\n\n## Current Job @t[2020..]\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2020-01-15";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(
            questions.len(),
            1,
            "Ongoing heading should not suppress stale questions"
        );
    }

    #[test]
    fn test_reviewed_marker_suppresses_stale_source() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Works at Acme Corp [^1] <!-- reviewed:{} -->\n\n[^1]: LinkedIn profile, scraped 2020-01-15",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_stale_questions(&content, 365);
        assert!(
            questions.is_empty(),
            "Recent reviewed marker should suppress stale source question"
        );
    }

    #[test]
    fn test_reviewed_marker_suppresses_stale_last_seen() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Lives in NYC @t[~2020-06] <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_stale_questions(&content, 365);
        assert!(
            questions.is_empty(),
            "Recent reviewed marker should suppress stale @t[~] question"
        );
    }

    #[test]
    fn test_old_reviewed_marker_still_generates() {
        let content = "# Person\n\n- Works at Acme Corp [^1] <!-- reviewed:2020-01-01 -->\n\n[^1]: LinkedIn profile, scraped 2019-06-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(
            questions.len(),
            1,
            "Old reviewed marker should not suppress stale question"
        );
    }

    #[test]
    fn test_stale_skips_review_queue_without_marker() {
        // Review queue heading without the HTML marker — entries should not be treated as facts
        let content = "# Person\n\n- Works at Acme Corp @t[~2020-06]\n\n## Review Queue\n\n- [ ] `@q[stale]` Line 3: \"Works at Acme Corp\" - is this still accurate?\n  > \n";
        let questions = generate_stale_questions(content, 365);
        // Should only generate question about the real fact, not the review queue entry
        assert!(
            questions.iter().all(|q| q.line_ref == Some(3)),
            "Should not generate questions about review queue entries: {:?}",
            questions
        );
    }
}
