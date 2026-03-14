//! Citation quality — tier-2 batch collection.
//!
//! Tier 1 (Rust validator) runs in `processor::citations`.
//! This module provides:
//! - `generate_weak_source_questions` — returns empty (no more individual questions)
//! - `WeakCitation` — structured data about a failing citation
//! - `collect_weak_citations` — collect all tier-1 failures from a document

use crate::models::ReviewQuestion;
use crate::patterns::{extract_frontmatter_reviewed_date, extract_reviewed_date};
use crate::processor::{
    citation_failure_reason, detect_citation_type, parse_source_definitions,
    parse_source_references, validate_citation,
};
use chrono::Utc;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// A citation that failed tier-1 structural validation.
/// Collected for tier-2 batch LLM review.
#[derive(Debug, Clone)]
pub struct WeakCitation {
    pub doc_id: String,
    pub doc_title: String,
    pub footnote_number: u32,
    pub citation_text: String,
    pub failure_reason: &'static str,
    pub line_number: usize,
}

/// Generate weak-source review questions.
///
/// **Deprecated behavior**: previously generated individual `@q[weak-source]` questions.
/// Now returns empty — citations are handled via tier-2 batch LLM review in the
/// maintain workflow (step 4: citation_review).
pub fn generate_weak_source_questions(_content: &str) -> Vec<ReviewQuestion> {
    vec![]
}

/// Collect all citations that fail tier-1 structural validation from a document.
///
/// Returns structured `WeakCitation` entries for tier-2 batch LLM review.
/// Skips citations that are untraceable (handled by `@q[missing]`) and
/// citations suppressed by a recent reviewed marker.
pub fn collect_weak_citations(content: &str, doc_id: &str, doc_title: &str) -> Vec<WeakCitation> {
    let today = Utc::now().date_naive();
    let fm_skip = extract_frontmatter_reviewed_date(content)
        .is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS);
    if fm_skip {
        return vec![];
    }

    let lines: Vec<&str> = content.lines().collect();
    let defs = parse_source_definitions(content);
    let refs = parse_source_references(content);

    defs.iter()
        .filter(|d| refs.iter().any(|r| r.number == d.number))
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
            // Only collect tier-1 failures (not already-passing citations)
            // Note: we collect ALL failing citations including very vague ones —
            // the agent can DEFER those in tier-2 review.
            let ct = detect_citation_type(&d.context);
            !validate_citation(&ct, &d.context)
        })
        .map(|d| {
            let ct = detect_citation_type(&d.context);
            WeakCitation {
                doc_id: doc_id.to_string(),
                doc_title: doc_title.to_string(),
                footnote_number: d.number,
                citation_text: d.context.clone(),
                failure_reason: citation_failure_reason(&ct),
                line_number: d.line_number,
            }
        })
        .collect()
}

/// Format a list of weak citations as a numbered batch for agent review.
///
/// Returns the formatted prompt text and the count of citations.
pub fn format_citation_batch(citations: &[WeakCitation]) -> String {
    if citations.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Review these citations. For each, respond with either:\n\
         - FIXED: provide the corrected citation text\n\
         - DEFER: explain why it cannot be improved\n\n",
    );
    for (i, c) in citations.iter().enumerate() {
        out.push_str(&format!(
            "{}. [^{}] doc: {} — '{}' — {}\n",
            i + 1,
            c.footnote_number,
            c.doc_title,
            c.citation_text,
            c.failure_reason,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_source(footnote_text: &str) -> String {
        format!(
            "<!-- factbase:abc123 -->\n# Test\n\n- Some fact [^1]\n\n---\n[^1]: {footnote_text}\n"
        )
    }

    // --- generate_weak_source_questions returns empty ---

    #[test]
    fn test_generate_weak_source_questions_always_empty() {
        // URL citation — previously would pass, now returns empty
        let content = doc_with_source("https://docs.aws.amazon.com/page.html");
        assert!(generate_weak_source_questions(&content).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_vague_also_empty() {
        // Vague citation — previously would generate a question, now returns empty
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        assert!(generate_weak_source_questions(&content).is_empty());
    }

    // --- collect_weak_citations ---

    #[test]
    fn test_url_citation_not_collected() {
        let content = doc_with_source("https://docs.aws.amazon.com/page.html, accessed 2026-03-07");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        assert!(weak.is_empty(), "URL citation should pass tier 1");
    }

    #[test]
    fn test_book_with_page_not_collected() {
        let content = doc_with_source("Peterson Field Guide, p.247");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        assert!(weak.is_empty(), "Book+page should pass tier 1");
    }

    #[test]
    fn test_book_without_page_collected() {
        let content = doc_with_source("Peterson Field Guide to Mushrooms");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].footnote_number, 1);
        assert!(weak[0].failure_reason.contains("page"));
    }

    #[test]
    fn test_phonetool_without_url_collected() {
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        assert_eq!(weak.len(), 1);
        assert!(weak[0].failure_reason.contains("URL"));
    }

    #[test]
    fn test_meeting_notes_alone_collected() {
        let content = doc_with_source("Meeting notes");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        assert_eq!(weak.len(), 1);
    }

    #[test]
    fn test_untraceable_also_collected() {
        // "Slack" alone is very vague but still collected for tier-2 review
        // The agent can DEFER it if it can't be improved
        let content = doc_with_source("Slack");
        let weak = collect_weak_citations(&content, "abc123", "Test");
        // "Slack" is SlackOrTeams type, fails validation (no channel/date) → collected
        assert_eq!(weak.len(), 1, "Vague sources are collected for tier-2 review");
    }

    #[test]
    fn test_unreferenced_source_not_collected() {
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Some fact without ref\n\n---\n[^1]: Wikipedia article on mushrooms\n";
        let weak = collect_weak_citations(content, "abc123", "Test");
        assert!(weak.is_empty(), "Unreferenced sources should not be collected");
    }

    #[test]
    fn test_collect_includes_doc_context() {
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let weak = collect_weak_citations(&content, "doc001", "My Entity");
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].doc_id, "doc001");
        assert_eq!(weak[0].doc_title, "My Entity");
        assert_eq!(weak[0].footnote_number, 1);
        assert_eq!(weak[0].citation_text, "Phonetool lookup, 2026-02-10");
    }

    #[test]
    fn test_collect_multiple_sources_mixed() {
        let content = "<!-- factbase:abc123 -->\n# Test\n\n\
            - Fact one [^1]\n\
            - Fact two [^2]\n\
            - Fact three [^3]\n\n\
            ---\n\
            [^1]: https://example.com\n\
            [^2]: Company internal wiki, 2026-01-15\n\
            [^3]: RFC 7231\n";
        let weak = collect_weak_citations(content, "abc123", "Test");
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].footnote_number, 2);
    }

    // --- format_citation_batch ---

    #[test]
    fn test_format_citation_batch_empty() {
        assert!(format_citation_batch(&[]).is_empty());
    }

    #[test]
    fn test_format_citation_batch_numbered() {
        let citations = vec![
            WeakCitation {
                doc_id: "doc1".into(),
                doc_title: "John Smith".into(),
                footnote_number: 3,
                citation_text: "Phonetool lookup, 2026-02-10".into(),
                failure_reason: "tool name present but no URL — add the direct URL",
                line_number: 10,
            },
            WeakCitation {
                doc_id: "doc2".into(),
                doc_title: "XSOLIS".into(),
                footnote_number: 7,
                citation_text: "Meeting with account team, January".into(),
                failure_reason: "meeting/call source missing participants or date",
                line_number: 15,
            },
        ];
        let batch = format_citation_batch(&citations);
        assert!(batch.contains("1. [^3] doc: John Smith"));
        assert!(batch.contains("2. [^7] doc: XSOLIS"));
        assert!(batch.contains("Phonetool lookup"));
        assert!(batch.contains("FIXED"));
        assert!(batch.contains("DEFER"));
    }

    // --- QuestionType::WeakSource still parseable (for backward compat) ---

    #[test]
    fn test_weak_source_question_type_still_exists() {
        use crate::models::QuestionType;
        // The QuestionType::WeakSource variant must still exist for parsing old docs
        let qt = QuestionType::WeakSource;
        assert_eq!(qt.to_string(), "weak-source");
    }
}
