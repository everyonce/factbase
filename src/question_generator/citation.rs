//! Citation quality — tier-1 question generation and tier-2 batch collection.
//!
//! Tier 1 (this module): `generate_weak_source_questions` — runs `validate_citation()`
//! on every referenced footnote and emits `@q[weak-source]` questions for failures.
//! This runs during every `check` operation.
//!
//! Tier 2: `collect_weak_citations` + `format_citation_triage_batch` — batch LLM review
//! used by the resolve workflow pre-step to auto-dismiss VALID citations and label
//! INVALID/WEAK ones before the normal resolve loop handles them.

use crate::models::ReviewQuestion;
use crate::patterns::{extract_frontmatter_reviewed_date, extract_reviewed_date};
use crate::processor::{
    citation_failure_reason, detect_citation_type, is_citation_specific_with_patterns,
    parse_source_definitions, parse_source_references,
};
use chrono::Utc;
use regex::Regex;

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

/// Generate weak-source review questions by running tier-1 structural validation
/// on every referenced footnote definition.
///
/// For each citation that fails `validate_citation()` and no `extra_patterns` match,
/// emits a `@q[weak-source]` question with the failure reason.
/// Suppressed by a recent `reviewed:` marker (same 180-day window as other generators).
pub fn generate_weak_source_questions(content: &str, extra_patterns: &[Regex]) -> Vec<ReviewQuestion> {
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
        .filter(|d| !is_citation_specific_with_patterns(&d.context, extra_patterns))
        .map(|d| {
            let ct = detect_citation_type(&d.context);
            let reason = citation_failure_reason(&ct);
            ReviewQuestion::new(
                crate::models::QuestionType::WeakSource,
                Some(d.line_number),
                format!(
                    "Citation [^{}] \"{}\" is not specific enough to verify — {}",
                    d.number, d.context, reason
                ),
            )
        })
        .collect()
}

/// Collect all citations that fail tier-1 structural validation from a document.
///
/// Returns structured `WeakCitation` entries for tier-2 batch LLM review.
/// Skips citations that are untraceable (handled by `@q[missing]`) and
/// citations suppressed by a recent reviewed marker.
pub fn collect_weak_citations(
    content: &str,
    doc_id: &str,
    doc_title: &str,
    extra_patterns: &[Regex],
) -> Vec<WeakCitation> {
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
            !is_citation_specific_with_patterns(&d.context, extra_patterns)
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

/// Phase 1 triage batch size: evaluate up to this many citations per LLM call.
pub const CITATION_TRIAGE_BATCH_SIZE: usize = 200;

/// Phase 2 resolve batch size: fix up to this many citations per LLM call (tool calls needed).
pub const CITATION_RESOLVE_BATCH_SIZE: usize = 15;

/// Format a list of weak citations as a Phase 1 triage batch.
///
/// The agent labels each citation VALID, INVALID, or WEAK + suggestion.
/// No tool calls needed — pure reasoning over the citation text.
pub fn format_citation_triage_batch(citations: &[WeakCitation]) -> String {
    if citations.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Evaluate these citations. For each, ask: could someone with access to this KB's domain \
         find the exact source using only the information provided? Consider:\n\
         - Source authority (is this a primary/authoritative source for the claim?)\n\
         - Accessibility (can the URL be reached? Is it behind a paywall?)\n\
         - Specificity (does it point to a specific page, not just a homepage?)\n\
         - Duplicates (is this the same source as another footnote?)\n\
         - Fabrication risk (does this source actually exist?)\n\
         Respond: VALID|INVALID|WEAK — reason — suggestion with specific replacement if applicable\n\n",
    );
    for (i, c) in citations.iter().enumerate() {
        out.push_str(&format!(
            "{}. [doc: {}] [^{}] \"{}\" — {}\n",
            i + 1,
            c.doc_title,
            c.footnote_number,
            c.citation_text,
            c.failure_reason,
        ));
    }
    out
}

/// Format a list of weak citations as a Phase 2 resolve batch.
///
/// Each entry includes the doc context and an optional hint from Phase 1 triage.
/// The agent uses tool calls (web_search, get_entity, etc.) to fix each citation.
pub fn format_citation_resolve_batch(citations: &[WeakCitation], hints: &[String]) -> String {
    if citations.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Fix these citations using your available tools. For each:\n\
         - FIXED: provide the corrected footnote text\n\
         - DEFER: explain what you tried and why it cannot be improved\n\n",
    );
    for (i, c) in citations.iter().enumerate() {
        let hint = hints.get(i).map(|s| s.as_str()).unwrap_or("");
        out.push_str(&format!(
            "{}. [doc: {}, id: {}] [^{}] \"{}\"\n",
            i + 1,
            c.doc_title,
            c.doc_id,
            c.footnote_number,
            c.citation_text,
        ));
        if !hint.is_empty() {
            out.push_str(&format!("   Hint: {hint}\n"));
        }
    }
    out
}

/// Format a list of weak citations as a numbered batch for agent review.
///
/// Legacy format — kept for backward compatibility.
/// New code should use `format_citation_triage_batch` + `format_citation_resolve_batch`.
pub fn format_citation_batch(citations: &[WeakCitation]) -> String {
    format_citation_triage_batch(citations)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_source(footnote_text: &str) -> String {
        format!(
            "<!-- factbase:abc123 -->\n# Test\n\n- Some fact [^1]\n\n---\n[^1]: {footnote_text}\n"
        )
    }

    // --- generate_weak_source_questions ---

    #[test]
    fn test_generate_weak_source_questions_bad_citation() {
        // Vague citation → generates a question
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content, &[]);
        assert!(!qs.is_empty(), "vague citation should generate a weak-source question");
        assert_eq!(qs[0].question_type, crate::models::QuestionType::WeakSource);
    }

    #[test]
    fn test_generate_weak_source_questions_url_citation() {
        // URL citation passes tier 1 → no question
        let content = doc_with_source("https://docs.aws.amazon.com/page.html");
        assert!(generate_weak_source_questions(&content, &[]).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_book_with_page() {
        // Book + page passes tier 1 → no question
        let content = doc_with_source("Peterson Field Guide, p.247");
        assert!(generate_weak_source_questions(&content, &[]).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_book_with_publisher_year() {
        // Book + publisher + year passes tier 1 → no question
        let content = doc_with_source("Smith, John. The Art of Programming. MIT Press, 2019");
        assert!(generate_weak_source_questions(&content, &[]).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_phonetool_no_url() {
        // Tool without URL → generates a question
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let qs = generate_weak_source_questions(&content, &[]);
        assert!(!qs.is_empty());
        assert!(qs[0].description.contains("[^1]"));
        assert!(qs[0].description.contains("Phonetool lookup"));
    }

    #[test]
    fn test_generate_weak_source_questions_unreferenced_not_generated() {
        // Unreferenced footnote → no question
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Some fact without ref\n\n---\n[^1]: Wikipedia article on mushrooms\n";
        assert!(generate_weak_source_questions(content, &[]).is_empty());
    }

    // --- collect_weak_citations ---

    #[test]
    fn test_url_citation_not_collected() {
        let content = doc_with_source("https://docs.aws.amazon.com/page.html, accessed 2026-03-07");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        assert!(weak.is_empty(), "URL citation should pass tier 1");
    }

    #[test]
    fn test_book_with_page_not_collected() {
        let content = doc_with_source("Peterson Field Guide, p.247");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        assert!(weak.is_empty(), "Book+page should pass tier 1");
    }

    #[test]
    fn test_book_without_page_collected() {
        let content = doc_with_source("Peterson Field Guide to Mushrooms");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].footnote_number, 1);
        assert!(weak[0].failure_reason.contains("page") || weak[0].failure_reason.contains("publisher"));
    }

    #[test]
    fn test_phonetool_without_url_collected() {
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        assert_eq!(weak.len(), 1);
        assert!(weak[0].failure_reason.contains("URL"));
    }

    #[test]
    fn test_meeting_notes_alone_collected() {
        let content = doc_with_source("Meeting notes");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        assert_eq!(weak.len(), 1);
    }

    #[test]
    fn test_untraceable_also_collected() {
        // "Slack" alone is very vague but still collected for tier-2 review
        // The agent can DEFER it if it can't be improved
        let content = doc_with_source("Slack");
        let weak = collect_weak_citations(&content, "abc123", "Test", &[]);
        // "Slack" is SlackOrTeams type, fails validation (no channel/date) → collected
        assert_eq!(weak.len(), 1, "Vague sources are collected for tier-2 review");
    }

    #[test]
    fn test_unreferenced_source_not_collected() {
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Some fact without ref\n\n---\n[^1]: Wikipedia article on mushrooms\n";
        let weak = collect_weak_citations(content, "abc123", "Test", &[]);
        assert!(weak.is_empty(), "Unreferenced sources should not be collected");
    }

    #[test]
    fn test_collect_includes_doc_context() {
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let weak = collect_weak_citations(&content, "doc001", "My Entity", &[]);
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
        let weak = collect_weak_citations(content, "abc123", "Test", &[]);
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
        // format_citation_batch is now an alias for format_citation_triage_batch
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
        // New triage format: "[doc: Title] [^N] "text" — reason"
        assert!(batch.contains("1. [doc: John Smith] [^3]"));
        assert!(batch.contains("2. [doc: XSOLIS] [^7]"));
        assert!(batch.contains("Phonetool lookup"));
        assert!(batch.contains("VALID"));
        assert!(batch.contains("WEAK"));
    }

    #[test]
    fn test_format_citation_triage_batch_format() {
        let citations = vec![WeakCitation {
            doc_id: "doc1".into(),
            doc_title: "Amazon S3".into(),
            footnote_number: 2,
            citation_text: "AWS documentation".into(),
            failure_reason: "source type unrecognized — add URL, record ID, or other navigable reference",
            line_number: 5,
        }];
        let batch = format_citation_triage_batch(&citations);
        assert!(batch.contains("VALID"));
        assert!(batch.contains("INVALID"));
        assert!(batch.contains("WEAK"));
        assert!(batch.contains("[doc: Amazon S3] [^2]"));
        assert!(batch.contains("AWS documentation"));
    }

    #[test]
    fn test_format_citation_resolve_batch_includes_hints() {
        let citations = vec![WeakCitation {
            doc_id: "doc1".into(),
            doc_title: "John Smith".into(),
            footnote_number: 3,
            citation_text: "Phonetool lookup, 2026-02-10".into(),
            failure_reason: "tool name present but no URL — add the direct URL",
            line_number: 10,
        }];
        let hints = vec!["construct https://phonetool.amazon.com/users/{alias}".to_string()];
        let batch = format_citation_resolve_batch(&citations, &hints);
        assert!(batch.contains("FIXED"));
        assert!(batch.contains("DEFER"));
        assert!(batch.contains("[doc: John Smith, id: doc1] [^3]"));
        assert!(batch.contains("Phonetool lookup"));
        assert!(batch.contains("phonetool.amazon.com"));
    }

    #[test]
    fn test_format_citation_resolve_batch_no_hints() {
        let citations = vec![WeakCitation {
            doc_id: "doc2".into(),
            doc_title: "XSOLIS".into(),
            footnote_number: 7,
            citation_text: "Meeting with account team, January".into(),
            failure_reason: "meeting/call source missing participants or date",
            line_number: 15,
        }];
        let batch = format_citation_resolve_batch(&citations, &[]);
        assert!(batch.contains("FIXED"));
        assert!(batch.contains("DEFER"));
        assert!(batch.contains("[doc: XSOLIS, id: doc2] [^7]"));
        // No hint line when hints is empty
        assert!(!batch.contains("Hint:"));
    }

    #[test]
    fn test_citation_batch_sizes() {
        assert_eq!(CITATION_TRIAGE_BATCH_SIZE, 200);
        assert_eq!(CITATION_RESOLVE_BATCH_SIZE, 15);
    }

    // --- QuestionType::WeakSource still parseable (for backward compat) ---

    #[test]
    fn test_weak_source_question_type_still_exists() {
        use crate::models::QuestionType;
        // The QuestionType::WeakSource variant must still exist for parsing old docs
        let qt = QuestionType::WeakSource;
        assert_eq!(qt.to_string(), "weak-source");
    }

    #[test]
    fn test_generate_weak_source_with_perspective_pattern_suppresses_question() {
        // "internal memo" fails universal tier-1 but matches a custom perspective pattern → no question
        let content = doc_with_source("internal memo");
        let patterns = vec![crate::models::CitationPattern {
            name: "internal_memo".into(),
            pattern: r"internal memo".into(),
            description: None,
        }];
        let compiled = crate::processor::compile_citation_patterns(&patterns);
        let qs = generate_weak_source_questions(&content, &compiled);
        assert!(qs.is_empty(), "perspective pattern should suppress weak-source question");
    }

    #[test]
    fn test_generate_weak_source_without_matching_pattern_still_generates_question() {
        // A citation that fails universal tier-1 and doesn't match any perspective pattern → question
        let content = doc_with_source("AWS documentation");
        let patterns = vec![crate::models::CitationPattern {
            name: "verse_ref".into(),
            pattern: r"\w+ \d+:\d+".into(),
            description: None,
        }];
        let compiled = crate::processor::compile_citation_patterns(&patterns);
        let qs = generate_weak_source_questions(&content, &compiled);
        assert!(!qs.is_empty(), "non-matching pattern should not suppress question");
    }
}
