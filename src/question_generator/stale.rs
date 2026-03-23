//! Stale question generation.
//!
//! Generates `@q[stale]` questions for facts with old sources
//! or old `@t[~...]` dates.

use chrono::{NaiveDate, Utc};
use std::collections::HashMap;

use crate::models::repository::ReviewPerspective;
use crate::models::{QuestionType, ReviewQuestion, TemporalTagType};
use crate::patterns::{
    extract_frontmatter_reviewed_date, is_suppressed_for_type, ReviewedType, FACT_LINE_REGEX,
};
use crate::processor::{parse_source_definitions, parse_source_references, parse_temporal_tags};

use super::temporal::has_recent_verification;
use super::{extract_fact_text, iter_fact_lines};

/// (source_type, date, def_line, type_tag)
type DefMapEntry<'a> = (&'a str, Option<&'a str>, usize, Option<&'a str>);
///
/// Detects facts that may be outdated based on:
/// 1. Source footnote dates older than threshold (default: 365 days)
/// 2. `@t[~...]` (LastSeen) dates older than threshold
///
/// Returns a list of `ReviewQuestion` with `question_type = Stale`.
pub fn generate_stale_questions(content: &str, max_age_days: i64) -> Vec<ReviewQuestion> {
    generate_stale_questions_with_perspective(content, max_age_days, None, None)
}

/// Generate stale questions with per-source-type and per-doc-type staleness thresholds.
///
/// `perspective` provides `source_types` and `stale_days_by_type` overrides.
/// `doc_type` is the document type for `stale_days_by_type` lookup.
pub fn generate_stale_questions_with_perspective(
    content: &str,
    global_max_age_days: i64,
    perspective: Option<&ReviewPerspective>,
    doc_type: Option<&str>,
) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let today = Utc::now().date_naive();

    // Check frontmatter for document-level reviewed date (obsidian format)
    let fm_skip = extract_frontmatter_reviewed_date(content)
        .is_some_and(|d| (today - d).num_days() <= global_max_age_days);

    // Truncate at review queue marker — review entries are not document facts
    let body = &content[..crate::patterns::body_end_offset(content)];

    // Parse temporal tags upfront to identify closed ranges
    let tags = parse_temporal_tags(body);

    // Parse source references and definitions
    let refs = parse_source_references(body);
    let defs = parse_source_definitions(body);

    // Build map of footnote number -> (source_type, date, def_line, type_tag)
    let def_map: HashMap<u32, DefMapEntry<'_>> = defs
        .iter()
        .map(|d| {
            (
                d.number,
                (
                    d.source_type.as_str(),
                    d.date.as_deref(),
                    d.line_number,
                    d.type_tag.as_deref(),
                ),
            )
        })
        .collect();

    // Build map: line_number -> inherited tag type from nearest preceding ## heading
    let heading_tag_map = build_heading_temporal_map(body, &tags);

    // Check source dates for each fact line
    let lines: Vec<&str> = body.lines().collect();
    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts under headings with closed temporal ranges — old sources are expected
        // for historical sections.
        let under_closed_heading = heading_tag_map.get(&line_number).is_some_and(|tt| {
            matches!(
                tt,
                TemporalTagType::Range | TemporalTagType::PointInTime | TemporalTagType::Historical
            )
        });
        if under_closed_heading {
            continue;
        }

        // Skip facts whose own temporal tag marks them as immutable (point-in-time, closed
        // range, or historical). These facts cannot become stale — the period is over.
        // Open-ended (@t[YYYY..], @t[~DATE]) and unknown (@t[?]) tags do NOT exempt.
        let has_immutable_tag = tags.iter().any(|t| {
            t.line_number == line_number
                && matches!(
                    t.tag_type,
                    TemporalTagType::PointInTime
                        | TemporalTagType::Range
                        | TemporalTagType::Historical
                )
        });
        if has_immutable_tag {
            continue;
        }

        // Find source references on this line
        let line_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.line_number == line_number)
            .collect();

        // Skip facts with a recent reviewed marker (inline or frontmatter)
        if fm_skip || is_suppressed_for_type(line, ReviewedType::Stale, today, global_max_age_days)
        {
            continue;
        }

        // Resolve effective max_age for this fact line using most-permissive rule:
        // collect all type_tags from footnotes on this line, look up each in source_types,
        // use the highest stale_days (most permissive). None means never stale.
        let effective_max_age: Option<i64> = if let Some(persp) = perspective {
            resolve_fact_max_age(&line_refs, &def_map, persp, doc_type, global_max_age_days)
        } else {
            Some(global_max_age_days)
        };

        // Check if any source is stale
        for source_ref in &line_refs {
            if let Some((source_type, Some(date_str), _, type_tag)) =
                def_map.get(&source_ref.number)
            {
                if let Some(days_old) = days_since_date(date_str, today) {
                    let threshold = match effective_max_age {
                        None => continue, // never stale
                        Some(t) => t,
                    };
                    if days_old > threshold && !has_recent_verification(line, today) {
                        let type_note = type_tag
                            .map(|t| format!(" (source type: `{t}`, threshold: {threshold} days)"))
                            .unwrap_or_default();
                        questions.push(ReviewQuestion::new(
                            QuestionType::Stale,
                            Some(line_number),
                            format!(
                                "\"{fact_text}\" - {source_type} source from {date_str} may be outdated{type_note}, is this still accurate?"
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
                    if days_old > global_max_age_days && days_old > 180 {
                        // Get the fact text from this line
                        if tag.line_number > 0 && tag.line_number <= lines.len() {
                            let line = lines[tag.line_number - 1];
                            // Skip facts with a recent reviewed marker
                            // Skip facts with a recent reviewed marker (inline or frontmatter)
                            if fm_skip
                                || is_suppressed_for_type(
                                    line,
                                    ReviewedType::Stale,
                                    today,
                                    global_max_age_days,
                                )
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

/// Resolve the effective max_age_days for a fact line using the most-permissive rule.
///
/// Collects all `{type:x}` tags from footnotes on this line, looks each up in
/// `perspective.source_types`, and returns the highest stale_days (most permissive).
/// `None` means never stale (at least one source type is never-stale).
fn resolve_fact_max_age(
    line_refs: &[&crate::models::SourceReference],
    def_map: &HashMap<u32, DefMapEntry<'_>>,
    perspective: &ReviewPerspective,
    doc_type: Option<&str>,
    global_default: i64,
) -> Option<i64> {
    // Collect type_tags from all footnotes on this line
    let type_tags: Vec<&str> = line_refs
        .iter()
        .filter_map(|r| def_map.get(&r.number))
        .filter_map(|(_, _, _, tt)| *tt)
        .collect();

    if type_tags.is_empty() {
        // No type tags — fall through to doc_type / global
        return perspective.resolve_stale_days(None, doc_type, global_default);
    }

    // Most-permissive: highest stale_days wins; None (never) beats everything
    let mut best: Option<i64> = Some(0); // start at 0, will be replaced
    let mut found_any = false;
    for tt in &type_tags {
        let resolved = perspective.resolve_stale_days(Some(tt), doc_type, global_default);
        match (best, resolved) {
            (_, None) => return None, // never stale — short-circuit
            (Some(b), Some(r)) => {
                if !found_any || r > b {
                    best = Some(r);
                }
                found_any = true;
            }
            (None, _) => unreachable!(),
        }
    }
    if found_any {
        best
    } else {
        perspective.resolve_stale_days(None, doc_type, global_default)
    }
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
        let content2 =
            "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile, scraped 2023-06-01";
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

    // --- source type staleness tests ---

    fn make_perspective_with_source_types(
        types: &[(&str, Option<u32>)],
        global: Option<u32>,
    ) -> ReviewPerspective {
        use crate::models::repository::{ReviewPerspective, SourceTypeConfig};
        let mut map = std::collections::HashMap::new();
        for (k, v) in types {
            map.insert(k.to_string(), SourceTypeConfig { stale_days: *v });
        }
        ReviewPerspective {
            stale_days: global,
            source_types: Some(map),
            ..Default::default()
        }
    }

    #[test]
    fn test_source_type_never_suppresses_stale() {
        // book type → never stale
        let persp = make_perspective_with_source_types(&[("book", None)], Some(180));
        let content = "# Entity\n\n- Old fact [^1]\n\n[^1]: Some Book, 2010-01-01 {type:book}";
        let qs = generate_stale_questions_with_perspective(content, 180, Some(&persp), None);
        assert!(qs.is_empty(), "book source should never be stale");
    }

    #[test]
    fn test_source_type_threshold_applied() {
        // web type → 90 days; fact is 120 days old → stale
        let persp = make_perspective_with_source_types(&[("web", Some(90))], Some(365));
        let today = chrono::Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(120);
        let content = format!(
            "# Entity\n\n- Old fact [^1]\n\n[^1]: Web page, accessed {} {{type:web}}",
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions_with_perspective(&content, 365, Some(&persp), None);
        assert_eq!(qs.len(), 1, "web source should be stale after 90 days");
        assert!(
            qs[0].description.contains("web"),
            "question should mention source type"
        );
        assert!(
            qs[0].description.contains("90"),
            "question should mention threshold"
        );
    }

    #[test]
    fn test_source_type_threshold_not_exceeded() {
        // web type → 180 days; fact is 60 days old → not stale
        let persp = make_perspective_with_source_types(&[("web", Some(180))], Some(365));
        let today = chrono::Utc::now().date_naive();
        let recent_date = today - chrono::Duration::days(60);
        let content = format!(
            "# Entity\n\n- Recent fact [^1]\n\n[^1]: Web page, accessed {} {{type:web}}",
            recent_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions_with_perspective(&content, 365, Some(&persp), None);
        assert!(
            qs.is_empty(),
            "web source within threshold should not be stale"
        );
    }

    #[test]
    fn test_most_permissive_multi_footnote_rule() {
        // Fact has two footnotes: web (90 days) and book (never).
        // Most permissive = never → no stale question.
        let persp =
            make_perspective_with_source_types(&[("web", Some(90)), ("book", None)], Some(365));
        let today = chrono::Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(200);
        let content = format!(
            "# Entity\n\n- Old fact [^1][^2]\n\n[^1]: Web page, accessed {} {{type:web}}\n[^2]: Some Book, 2010-01-01 {{type:book}}",
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions_with_perspective(&content, 365, Some(&persp), None);
        assert!(
            qs.is_empty(),
            "book (never) should make fact never-stale even with old web source"
        );
    }

    #[test]
    fn test_most_permissive_multi_footnote_uses_highest_days() {
        // Fact has two footnotes: slack (90 days) and web (180 days).
        // Most permissive = 180 days. Fact is 120 days old → not stale.
        let persp = make_perspective_with_source_types(
            &[("slack", Some(90)), ("web", Some(180))],
            Some(365),
        );
        let today = chrono::Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(120);
        let content = format!(
            "# Entity\n\n- Fact [^1][^2]\n\n[^1]: Slack #ch, @user, {} {{type:slack}}\n[^2]: Web page, accessed {} {{type:web}}",
            old_date.format("%Y-%m-%d"),
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions_with_perspective(&content, 365, Some(&persp), None);
        assert!(
            qs.is_empty(),
            "most permissive (180 days) should apply; 120 days is within threshold"
        );
    }

    #[test]
    fn test_temporal_tag_on_fact_line_does_not_suppress_stale() {
        // Fact with @t[=2023-11] AND old footnote with {type:web} (30-day threshold).
        // PointInTime tag on the fact line → immutable → NO stale question.
        let persp = make_perspective_with_source_types(&[("web", Some(30))], Some(365));
        let content =
            "# Entity\n\n- Some fact @t[=2023-11] [^1]\n\n[^1]: Web page, accessed 2023-11-01 {type:web}";
        let qs = generate_stale_questions_with_perspective(content, 365, Some(&persp), None);
        assert!(
            qs.is_empty(),
            "point-in-time tag on fact line should exempt it from stale"
        );
    }

    #[test]
    fn test_fact_line_range_tag_does_not_suppress_stale() {
        // @t[2020..2022] on a fact line → closed range → NO stale question.
        let content =
            "# Entity\n\n- Some fact @t[2020..2022] [^1]\n\n[^1]: Web page, accessed 2020-01-01";
        let qs = generate_stale_questions(content, 365);
        assert!(
            qs.is_empty(),
            "closed range tag on fact line should exempt it from stale"
        );
    }

    #[test]
    fn test_point_in_time_fact_line_exempts_stale() {
        // @t[=2024-04-30] — exact point-in-time → NO stale question
        let content =
            "# Entity\n\n- Signed contract @t[=2024-04-30] [^1]\n\n[^1]: Contract doc, 2024-04-30";
        assert!(
            generate_stale_questions(content, 365).is_empty(),
            "@t[=2024-04-30] should exempt fact from stale"
        );
    }

    #[test]
    fn test_closed_range_fact_line_exempts_stale() {
        // @t[2021..2023] — closed range → NO stale question
        let content =
            "# Entity\n\n- Held position @t[2021..2023] [^1]\n\n[^1]: Records, 2023-12-01";
        assert!(
            generate_stale_questions(content, 365).is_empty(),
            "@t[2021..2023] should exempt fact from stale"
        );
    }

    #[test]
    fn test_open_range_fact_line_fires_stale() {
        // @t[2023..] — open/ongoing range → stale question FIRES
        let content =
            "# Entity\n\n- Currently employed @t[2023..] [^1]\n\n[^1]: LinkedIn, scraped 2023-01-01";
        assert_eq!(
            generate_stale_questions(content, 365).len(),
            1,
            "@t[2023..] (open range) should still fire stale"
        );
    }

    #[test]
    fn test_approximate_tag_fact_line_fires_stale() {
        // @t[~2023] — approximate/last-seen → stale question FIRES (via LastSeen loop)
        let content = "# Entity\n\n- Lives in Berlin @t[~2023]";
        assert_eq!(
            generate_stale_questions(content, 365).len(),
            1,
            "@t[~2023] should still fire stale"
        );
    }

    #[test]
    fn test_open_range_with_month_fires_stale() {
        // @t[2024-06..] — open range with month precision → stale question FIRES
        let content = "# Entity\n\n- Active project @t[2024-06..] [^1]\n\n[^1]: Jira, 2024-06-01";
        assert_eq!(
            generate_stale_questions(content, 365).len(),
            1,
            "@t[2024-06..] (open range) should still fire stale"
        );
    }

    #[test]
    fn test_heading_closed_range_suppresses_open_fact_tag() {
        // Heading has closed range; fact has open @t[2023..] — heading suppression wins
        let content = "# Entity\n\n## Q1 2023 @t[2023-01..2023-03]\n\n- Active then @t[2023..] [^1]\n\n[^1]: Notes, 2023-03-01";
        assert!(
            generate_stale_questions(content, 365).is_empty(),
            "closed heading should suppress stale even when fact has open tag"
        );
    }

    #[test]
    fn test_unknown_source_type_falls_back_to_global() {
        // unknown-type not in source_types → falls back to global 180 days
        let persp = make_perspective_with_source_types(&[("web", Some(90))], Some(180));
        let today = chrono::Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(200);
        let content = format!(
            "# Entity\n\n- Old fact [^1]\n\n[^1]: Some source, {} {{type:unknown-type}}",
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions_with_perspective(&content, 180, Some(&persp), None);
        assert_eq!(
            qs.len(),
            1,
            "unknown type should fall back to global 180 days"
        );
    }

    #[test]
    fn test_point_in_time_with_frontmatter_no_stale() {
        // Document WITH frontmatter (5+ lines): @t[=date] on fact line must exempt from stale.
        // This tests the coordinate system alignment between iter_fact_lines and parse_temporal_tags.
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(500);
        let content = format!(
            "---\nfactbase_id: abc123\ntype: news\ntags:\n  - news\n---\n# Title\n\n- AI Amplified Podcast launched @t[={}] [^5]\n\n[^5]: Source, {}",
            old_date.format("%Y-%m-%d"),
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions(&content, 365);
        assert!(
            qs.is_empty(),
            "@t[=date] with frontmatter should exempt fact from stale, got: {:?}",
            qs.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_point_in_time_without_frontmatter_no_stale_regression() {
        // Same as above but without frontmatter — regression check.
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(500);
        let content = format!(
            "# Title\n\n- AI Amplified Podcast launched @t[={}] [^5]\n\n[^5]: Source, {}",
            old_date.format("%Y-%m-%d"),
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions(&content, 365);
        assert!(
            qs.is_empty(),
            "@t[=date] without frontmatter should exempt fact from stale"
        );
    }

    #[test]
    fn test_open_range_with_frontmatter_fires_stale() {
        // @t[2023..] (open-ended) in a doc with frontmatter — MUST still generate stale.
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(500);
        let content = format!(
            "---\nfactbase_id: abc123\ntype: news\ntags:\n  - news\n---\n# Title\n\n- Currently active @t[2023..] [^1]\n\n[^1]: Source, {}",
            old_date.format("%Y-%m-%d")
        );
        let qs = generate_stale_questions(&content, 365);
        assert_eq!(
            qs.len(),
            1,
            "@t[2023..] (open-ended) with frontmatter should still fire stale"
        );
    }

    #[test]
    fn test_point_in_time_with_large_frontmatter_no_stale() {
        // Document with 20+ lines of frontmatter — coordinate system stress test.
        // iter_fact_lines must return full-document line numbers matching parse_temporal_tags.
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(500);
        let mut content = String::from("---\nfactbase_id: abc123\ntype: news\ntags:\n");
        for i in 0..16 {
            content.push_str(&format!("  - tag{i}\n"));
        }
        content.push_str("---\n# Title\n\n");
        content.push_str(&format!(
            "- Fact @t[={}] [^1]\n\n[^1]: Source, {}\n",
            old_date.format("%Y-%m-%d"),
            old_date.format("%Y-%m-%d")
        ));
        let qs = generate_stale_questions(&content, 365);
        assert!(
            qs.is_empty(),
            "@t[=date] with 20-line frontmatter should exempt from stale, got: {:?}",
            qs.iter().map(|q| &q.description).collect::<Vec<_>>()
        );
    }
}
