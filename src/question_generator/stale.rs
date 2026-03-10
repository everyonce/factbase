//! Stale question generation.
//!
//! Generates `@q[stale]` questions for facts with old sources
//! or old `@t[~...]` dates.

use chrono::{NaiveDate, Utc};
use std::collections::HashMap;

use crate::models::{QuestionType, ReviewQuestion, TemporalTagType};
use crate::patterns::{extract_frontmatter_reviewed_date, extract_reviewed_date, FACT_LINE_REGEX};
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

    // Check frontmatter for document-level reviewed date (obsidian format)
    let fm_skip = extract_frontmatter_reviewed_date(content)
        .is_some_and(|d| (today - d).num_days() <= max_age_days);

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

        // Skip facts with a recent reviewed marker (inline or frontmatter)
        if fm_skip
            || extract_reviewed_date(line)
                .is_some_and(|d| (today - d).num_days() <= max_age_days)
        {
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
                            // Skip facts with a recent reviewed marker (inline or frontmatter)
                            if fm_skip
                                || extract_reviewed_date(line)
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
/// Build a map from line number → inherited closed temporal tag type from the
/// nearest preceding heading (`#` or `##`). This lets facts under a heading
/// like `## Battle @t[=378]` or a title like `# Event @t[216 BCE..202 BCE]`
/// inherit the closed temporal context, suppressing false stale-source flags.
pub(crate) fn build_heading_temporal_map(
    content: &str,
    tags: &[crate::models::TemporalTag],
) -> HashMap<usize, TemporalTagType> {
    let mut map = HashMap::new();
    let mut current_tag_type: Option<TemporalTagType> = None;
    let mut heading_line: usize = 0;

    for (idx, line) in content.lines().enumerate() {
        let line_number = idx + 1;
        if line.starts_with("# ") && !line.starts_with("## ") {
            // H1 title heading — sets default for lines before first H2
            current_tag_type = find_closed_tag(tags, line_number);
            heading_line = line_number;
        } else if line.starts_with("## ") {
            // H2 section heading — overrides H1 context
            current_tag_type = find_closed_tag(tags, line_number);
            heading_line = line_number;
        } else if let Some(ref tt) = current_tag_type {
            if line_number > heading_line {
                map.insert(line_number, *tt);
            }
        }
    }
    map
}

fn find_closed_tag(
    tags: &[crate::models::TemporalTag],
    line_number: usize,
) -> Option<TemporalTagType> {
    tags.iter()
        .find(|t| {
            t.line_number == line_number
                && matches!(
                    t.tag_type,
                    TemporalTagType::Range
                        | TemporalTagType::PointInTime
                        | TemporalTagType::Historical
                )
        })
        .map(|t| t.tag_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_since_date() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        assert_eq!(days_since_date("2025-01-29", today), Some(365));
        assert_eq!(days_since_date("2026-01-29", today), Some(0));
        assert_eq!(days_since_date("2025-01", today), Some(393));
        assert_eq!(days_since_date("2025", today), Some(393));
        assert_eq!(days_since_date("invalid", today), None);
        assert_eq!(days_since_date("2025-Q2", today), None);
    }

    #[test]
    fn test_generate_stale_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        assert!(generate_stale_questions(content, 365).is_empty());
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
    }

    #[test]
    fn test_generate_stale_questions_recent_source() {
        let today = Utc::now().date_naive();
        let recent_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped {}",
            recent_date.format("%Y-%m-%d")
        );
        assert!(generate_stale_questions(&content, 365).is_empty());
    }

    #[test]
    fn test_generate_stale_questions_last_seen_tag() {
        // Old @t[~] generates stale question
        let content = "# Person\n\n- Lives in NYC @t[~2020-06]";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("@t[~2020-06]"));

        // Recent @t[~] does not
        let today = Utc::now().date_naive();
        let recent = today - chrono::Duration::days(30);
        let content2 = format!("# Person\n\n- Lives in NYC @t[~{}]", recent.format("%Y-%m"));
        assert!(generate_stale_questions(&content2, 365).is_empty());
    }

    #[test]
    fn test_generate_stale_questions_no_source_date() {
        let content = "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile";
        assert!(generate_stale_questions(content, 365).is_empty());
    }

    #[test]
    fn test_generate_stale_questions_custom_threshold() {
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(60);
        let content = format!(
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped {}",
            old_date.format("%Y-%m-%d")
        );
        assert_eq!(generate_stale_questions(&content, 30).len(), 1);
    }

    #[test]
    fn test_generate_stale_questions_line_numbers() {
        let content =
            "# Person\n\nParagraph\n\n- Old fact [^1]\n- Another fact\n\n[^1]: Source, 2020-01-01";
        let questions = generate_stale_questions(content, 365);
        assert_eq!(questions[0].line_ref, Some(5));
    }

    #[test]
    fn test_generate_stale_questions_multiple_sources() {
        let content = "# Person\n\n- Fact one [^1]\n- Fact two [^2]\n\n[^1]: Source, 2020-01-01\n[^2]: Source, 2020-02-01";
        assert_eq!(generate_stale_questions(content, 365).len(), 2);
    }

    #[test]
    fn test_generate_stale_questions_avoids_duplicate_for_same_line() {
        let content = "# Person\n\n- Lives in NYC @t[~2020-06] [^1]\n\n[^1]: Source, 2020-01-01";
        assert_eq!(generate_stale_questions(content, 365).len(), 1);
    }

    #[test]
    fn test_stale_source_verification_suppression() {
        // Recent @t[~] verification suppresses stale source
        let content = "# Person\n\n- Works at Acme Corp @t[~2026-01] [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        assert!(generate_stale_questions(content, 365).is_empty());
        // No @t[~] tag — still generates
        let content2 = "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        assert_eq!(generate_stale_questions(content2, 365).len(), 1);
        // Old @t[~] verification — still generates
        let content3 = "# Person\n\n- Works at Acme Corp @t[~2024-01] [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
        assert_eq!(generate_stale_questions(content3, 365).len(), 1);
    }

    #[test]
    fn test_heading_temporal_tag_suppression() {
        // PointInTime heading suppresses stale
        let c1 = "# Events\n\n## re:Invent 2024 @t[=2024-12]\n\n- Met with John Smith [^1]\n\n[^1]: Notes, 2024-12-05";
        assert!(generate_stale_questions(c1, 365).is_empty());
        // Range heading suppresses stale
        let c2 = "# Events\n\n## Q1 2023 Sprint @t[2023-01..2023-03]\n\n- Delivered feature X [^1]\n\n[^1]: Jira, 2023-03-15";
        assert!(generate_stale_questions(c2, 365).is_empty());
        // No tag — still generates
        let c3 = "# Person\n\n## Current Role\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2020-01-15";
        assert_eq!(generate_stale_questions(c3, 365).len(), 1);
        // Ongoing heading — still generates
        let c4 = "# Person\n\n## Current Job @t[2020..]\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2020-01-15";
        assert_eq!(generate_stale_questions(c4, 365).len(), 1);
    }

    #[test]
    fn test_reviewed_marker_suppresses_stale() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        // Recent reviewed marker suppresses stale source
        let c1 = format!(
            "# Person\n\n- Works at Acme Corp [^1] <!-- reviewed:{} -->\n\n[^1]: LinkedIn profile, scraped 2020-01-15",
            marker_date.format("%Y-%m-%d")
        );
        assert!(generate_stale_questions(&c1, 365).is_empty());
        // Recent reviewed marker suppresses stale @t[~]
        let c2 = format!(
            "# Person\n\n- Lives in NYC @t[~2020-06] <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        assert!(generate_stale_questions(&c2, 365).is_empty());
        // Old reviewed marker does NOT suppress
        let c3 = "# Person\n\n- Works at Acme Corp [^1] <!-- reviewed:2020-01-01 -->\n\n[^1]: LinkedIn profile, scraped 2019-06-01";
        assert_eq!(generate_stale_questions(c3, 365).len(), 1);
    }

    #[test]
    fn test_stale_skips_review_queue_without_marker() {
        let content = "# Person\n\n- Works at Acme Corp @t[~2020-06]\n\n## Review Queue\n\n- [ ] `@q[stale]` Line 3: \"Works at Acme Corp\" - is this still accurate?\n  > \n";
        let questions = generate_stale_questions(content, 365);
        assert!(questions.iter().all(|q| q.line_ref == Some(3)));
    }

    #[test]
    fn test_h1_title_temporal_tag_suppresses_stale() {
        // H1 title with closed temporal range suppresses stale for facts before first H2
        let c1 = "# Battle of Adrianople @t[=0378]\n\n- Fought near Adrianople [^1]\n\n[^1]: Burns (1994), 1994";
        assert!(
            generate_stale_questions(c1, 365).is_empty(),
            "facts under H1 with closed temporal tag should not be flagged as stale"
        );
        // H1 with range also suppresses
        let c2 = "# Second Punic War @t[-218..-201]\n\n- Hannibal crossed the Alps [^1]\n\n[^1]: Livy translation, 2003";
        assert!(
            generate_stale_questions(c2, 365).is_empty(),
            "facts under H1 with date range should not be flagged as stale"
        );
        // H1 without temporal tag still flags
        let c3 = "# Some Entity\n\n- Old fact [^1]\n\n[^1]: Source, 2020-01-15";
        assert_eq!(generate_stale_questions(c3, 365).len(), 1);
    }

    #[test]
    fn test_h2_overrides_h1_temporal_context() {
        // H1 has closed range but H2 has no tag — facts under H2 should still be flagged
        let content = "# Historical Event @t[=378]\n\n## Modern Analysis\n\n- Recent claim [^1]\n\n[^1]: Source, 2020-01-15";
        assert_eq!(
            generate_stale_questions(content, 365).len(),
            1,
            "H2 without temporal tag should override H1 context"
        );
    }
}
