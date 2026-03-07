//! Weak source citation question generation.
//!
//! Generates `@q[weak-source]` questions for footnotes that are too vague
//! for independent verification.

use chrono::Utc;

use crate::models::{QuestionType, ReviewQuestion};
use crate::output::truncate_str;
use crate::patterns::extract_reviewed_date;
use crate::processor::{is_citation_specific, parse_source_definitions, parse_source_references};

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Generate weak-source questions for citations too vague to independently verify.
///
/// Checks each footnote definition against specificity patterns (URL, file path,
/// page/section ref, identifier, named person+date, channel+date). Skips sources
/// already flagged as completely untraceable by `generate_source_quality_questions`.
pub fn generate_weak_source_questions(content: &str) -> Vec<ReviewQuestion> {
    let today = Utc::now().date_naive();
    let lines: Vec<&str> = content.lines().collect();
    let defs = parse_source_definitions(content);
    let refs = parse_source_references(content);

    defs.iter()
        .filter(|d| {
            // Only check sources actually referenced by facts
            refs.iter().any(|r| r.number == d.number)
        })
        .filter(|d| {
            // Skip if recently reviewed
            if d.line_number > 0 && d.line_number <= lines.len() {
                let line = lines[d.line_number - 1];
                if extract_reviewed_date(line)
                    .is_some_and(|dt| (today - dt).num_days() <= REVIEWED_SKIP_DAYS)
                {
                    return false;
                }
            }
            true
        })
        .filter(|d| {
            // Skip sources already caught by is_untraceable_source (those get @q[missing])
            let ctx = &d.context;
            if super::missing::is_untraceable_source(ctx) {
                return false;
            }
            // Flag if not specific enough
            !is_citation_specific(ctx)
        })
        .map(|d| {
            ReviewQuestion::new(
                QuestionType::WeakSource,
                Some(d.line_number),
                format!(
                    "Citation [^{}] \"{}\" is not specific enough to independently verify. \
                     Add a URL, document path, page number, or other identifier that would \
                     let someone else find this exact source.",
                    d.number,
                    truncate_str(&d.context, 80),
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_source(footnote_text: &str) -> String {
        format!(
            "<!-- factbase:abc123 -->\n# Test\n\n- Some fact [^1]\n\n---\n[^1]: {footnote_text}\n"
        )
    }

    // --- Good citations (specific) → no questions ---

    #[test]
    fn test_specific_url_no_question() {
        let content = doc_with_source("https://docs.aws.amazon.com/page.html, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty(), "URL citation should not be flagged");
    }

    #[test]
    fn test_specific_isbn_no_question() {
        let content = doc_with_source("ISBN 978-0-13-468599-1, Chapter 12");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_rfc_no_question() {
        let content = doc_with_source("RFC 7231, Section 6.5.1");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_file_path_no_question() {
        let content = doc_with_source("/Users/daniel/work/notes/architecture.md");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_interview_person_date_no_question() {
        let content = doc_with_source("Interview with Jane Doe, CTO of Acme Corp, 2025-11-03");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_slack_channel_date_no_question() {
        let content = doc_with_source("Slack #project-alpha, thread 2026-01-20, re: deployment");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_page_ref_no_question() {
        let content = doc_with_source("Peterson Field Guide, p.247");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_specific_observation_number_no_question() {
        let content = doc_with_source("iNaturalist observation #12345, 2024-06-15");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    // --- Vague citations (not specific, not untraceable) → flagged ---

    #[test]
    fn test_vague_aws_documentation_with_date_flagged() {
        // Has date so not untraceable, but no URL/page → vague
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question_type, QuestionType::WeakSource);
    }

    #[test]
    fn test_vague_wikipedia_article_no_url_flagged() {
        // >20 chars so not untraceable, but no URL → vague
        let content = doc_with_source("Wikipedia article on mushroom taxonomy");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question_type, QuestionType::WeakSource);
    }

    #[test]
    fn test_vague_company_wiki_flagged() {
        let content = doc_with_source("Company internal wiki, last checked 2026-01");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
    }

    #[test]
    fn test_vague_slack_conversation_with_date_flagged() {
        // Has date but no channel/thread → vague
        let content = doc_with_source("Slack conversation, 2026-01-15");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
    }

    #[test]
    fn test_vague_google_search_results_flagged() {
        let content = doc_with_source("Google search results, 2026-02-01");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
    }

    #[test]
    fn test_vague_team_meeting_notes_flagged() {
        let content = doc_with_source("Team meeting notes, January 2026");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
    }

    #[test]
    fn test_vague_internal_documentation_with_date_flagged() {
        let content = doc_with_source("Internal documentation, reviewed 2026-01-15");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
    }

    // --- Untraceable sources → skipped (handled by @q[missing]) ---

    #[test]
    fn test_untraceable_slack_not_double_flagged() {
        let content = doc_with_source("Slack");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty(), "Untraceable sources handled by @q[missing]");
    }

    #[test]
    fn test_untraceable_email_not_double_flagged() {
        let content = doc_with_source("Email");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    #[test]
    fn test_untraceable_internal_not_double_flagged() {
        let content = doc_with_source("Internal");
        let qs = generate_weak_source_questions(&content);
        assert!(qs.is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn test_unreferenced_source_not_flagged() {
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Some fact without ref\n\n---\n[^1]: Wikipedia article on mushrooms\n";
        let qs = generate_weak_source_questions(content);
        assert!(qs.is_empty(), "Unreferenced sources should not be flagged");
    }

    #[test]
    fn test_line_ref_points_to_definition() {
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
        // The footnote definition is on line 7
        assert_eq!(qs[0].line_ref, Some(7));
    }

    #[test]
    fn test_multiple_sources_mixed() {
        let content = "<!-- factbase:abc123 -->\n# Test\n\n\
            - Fact one [^1]\n\
            - Fact two [^2]\n\
            - Fact three [^3]\n\n\
            ---\n\
            [^1]: https://example.com\n\
            [^2]: Company internal wiki, 2026-01-15\n\
            [^3]: RFC 7231\n";
        let qs = generate_weak_source_questions(content);
        assert_eq!(qs.len(), 1);
        assert!(qs[0].description.contains("[^2]"));
    }

    #[test]
    fn test_question_description_format() {
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content);
        assert_eq!(qs.len(), 1);
        assert!(qs[0].description.contains("not specific enough"));
        assert!(qs[0].description.contains("URL"));
    }
}
