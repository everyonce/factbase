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
use std::collections::HashSet;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// HTML comment marker appended to a footnote line when a weak-source question
/// is dismissed after tier-2 evaluation. Prevents the footnote from being
/// re-flagged on subsequent scans.
pub const CITATION_ACCEPTED_MARKER: &str = "<!-- ✓ -->";

/// Generate weak-source review questions by running tier-1 structural validation
/// on every referenced footnote definition.
///
/// For each citation that fails `validate_citation()` and no `extra_patterns` match,
/// emits a `@q[weak-source]` question with the failure reason.
/// Suppressed by a recent `reviewed:` marker (same 180-day window as other generators).
///
/// `known_source_types`: keys from `perspective.yaml source_types`. When `Some`, a
/// `{type:x}` tag whose value is in the set is treated as valid and suppresses the
/// question. When `None` (no `source_types` block in perspective), falls back to
/// existing tier-1 behavior.
pub fn generate_weak_source_questions(
    content: &str,
    extra_patterns: &[Regex],
    known_source_types: Option<&HashSet<String>>,
) -> Vec<ReviewQuestion> {
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
                if line.contains(CITATION_ACCEPTED_MARKER) {
                    return false;
                }
                if extract_reviewed_date(line)
                    .is_some_and(|dt| (today - dt).num_days() <= REVIEWED_SKIP_DAYS)
                {
                    return false;
                }
            }
            true
        })
        .filter(|d| !is_citation_specific_with_patterns(&d.context, extra_patterns))
        .filter(|d| {
            // If {type:x} is present and x is in the known source types → skip (no question)
            if let Some(known) = known_source_types {
                if let Some(ref type_tag) = d.type_tag {
                    return !known.contains(type_tag.as_str());
                }
            }
            true
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_source(footnote_text: &str) -> String {
        format!(
            "---\nfactbase_id: abc123\n---\n# Test\n\n- Some fact [^1]\n\n---\n[^1]: {footnote_text}\n"
        )
    }

    // --- generate_weak_source_questions ---

    #[test]
    fn test_generate_weak_source_questions_bad_citation() {
        // Vague citation → generates a question
        let content = doc_with_source("AWS documentation, accessed 2026-03-07");
        let qs = generate_weak_source_questions(&content, &[], None);
        assert!(
            !qs.is_empty(),
            "vague citation should generate a weak-source question"
        );
        assert_eq!(qs[0].question_type, crate::models::QuestionType::WeakSource);
    }

    #[test]
    fn test_generate_weak_source_questions_url_citation() {
        // URL citation passes tier 1 → no question
        let content = doc_with_source("https://docs.aws.amazon.com/page.html");
        assert!(generate_weak_source_questions(&content, &[], None).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_book_with_page() {
        // Book + page passes tier 1 → no question
        let content = doc_with_source("Peterson Field Guide, p.247");
        assert!(generate_weak_source_questions(&content, &[], None).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_book_with_publisher_year() {
        // Book + publisher + year passes tier 1 → no question
        let content = doc_with_source("Smith, John. The Art of Programming. MIT Press, 2019");
        assert!(generate_weak_source_questions(&content, &[], None).is_empty());
    }

    #[test]
    fn test_generate_weak_source_questions_phonetool_no_url() {
        // Tool without URL → generates a question
        let content = doc_with_source("Phonetool lookup, 2026-02-10");
        let qs = generate_weak_source_questions(&content, &[], None);
        assert!(!qs.is_empty());
        assert!(qs[0].description.contains("[^1]"));
        assert!(qs[0].description.contains("Phonetool lookup"));
    }

    #[test]
    fn test_generate_weak_source_questions_unreferenced_not_generated() {
        // Unreferenced footnote → no question
        let content = "---\nfactbase_id: abc123\n---\n# Test\n\n- Some fact without ref\n\n---\n[^1]: Wikipedia article on mushrooms\n";
        assert!(generate_weak_source_questions(content, &[], None).is_empty());
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
        let qs = generate_weak_source_questions(&content, &compiled, None);
        assert!(
            qs.is_empty(),
            "perspective pattern should suppress weak-source question"
        );
    }

    #[test]
    fn test_generate_weak_source_questions_without_matching_pattern_still_generates_question() {
        // A citation that fails universal tier-1 and doesn't match any perspective pattern → question
        let content = doc_with_source("AWS documentation");
        let patterns = vec![crate::models::CitationPattern {
            name: "verse_ref".into(),
            pattern: r"\w+ \d+:\d+".into(),
            description: None,
        }];
        let compiled = crate::processor::compile_citation_patterns(&patterns);
        let qs = generate_weak_source_questions(&content, &compiled, None);
        assert!(
            !qs.is_empty(),
            "non-matching pattern should not suppress question"
        );
    }

    #[test]
    fn test_generate_weak_source_accepted_marker_suppresses_question() {
        // Footnote with <!-- ✓ --> → no question generated
        let content = format!(
            "---\nfactbase_id: abc123\n---\n# Test\n\n- Some fact [^1]\n\n---\n[^1]: Phonetool lookup, 2026-02-10 {}",
            CITATION_ACCEPTED_MARKER
        );
        let qs = generate_weak_source_questions(&content, &[], None);
        assert!(
            qs.is_empty(),
            "accepted marker should suppress weak-source question"
        );
    }

    // --- known_source_types: {type:x} suppression ---

    #[test]
    fn test_known_source_type_suppresses_weak_source_question() {
        // {type:aws-docs} is in known_source_types → no question
        let content = doc_with_source(
            "AWS Lambda docs, https://docs.aws.amazon.com/lambda/, accessed 2026-03-21 {type:aws-docs}",
        );
        let known: HashSet<String> = ["aws-docs".to_string()].into();
        let qs = generate_weak_source_questions(&content, &[], Some(&known));
        assert!(
            qs.is_empty(),
            "known source type should suppress weak-source question"
        );
    }

    #[test]
    fn test_unknown_source_type_with_source_types_block_generates_question() {
        // {type:unknown-type} is NOT in known_source_types → question generated
        let content = doc_with_source("Some internal doc {type:unknown-type}");
        let known: HashSet<String> = ["aws-docs".to_string()].into();
        let qs = generate_weak_source_questions(&content, &[], Some(&known));
        assert!(
            !qs.is_empty(),
            "unrecognized type with source_types block should generate question"
        );
    }

    #[test]
    fn test_no_source_types_block_falls_back_to_existing_behavior() {
        // No known_source_types (None) → existing tier-1 behavior, {type:x} not recognized
        let content = doc_with_source("Some internal doc {type:aws-docs}");
        let qs = generate_weak_source_questions(&content, &[], None);
        assert!(
            !qs.is_empty(),
            "without source_types block, {{type:x}} should not suppress question"
        );
    }

    #[test]
    fn test_known_source_type_without_url_still_suppressed() {
        // {type:phonetool} in known_source_types → suppressed even without URL
        let content = doc_with_source("Phonetool lookup, 2026-02-10 {type:phonetool}");
        let known: HashSet<String> = ["phonetool".to_string()].into();
        let qs = generate_weak_source_questions(&content, &[], Some(&known));
        assert!(
            qs.is_empty(),
            "known source type should suppress even without URL"
        );
    }
}
