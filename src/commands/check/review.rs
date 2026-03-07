//! Review question generation for lint.
//!
//! Contains logic for generating review questions during `lint --review`.
//! This module provides helper functions to generate questions for documents
//! based on various quality checks (temporal tags, sources, duplicates, etc.).

use factbase::{
    filter_sequential_conflicts,
    generate_duplicate_questions,
    generate_required_field_questions,
    normalize_conflict_desc, parse_review_queue, prune_stale_questions, QuestionType,
    ReviewQuestion,
};
use factbase::question_generator::check::run_generators;
use std::collections::{HashMap, HashSet};

/// Configuration for review question generation
pub struct ReviewConfig {
    /// Threshold in days for stale content detection
    pub stale_threshold: i64,
    /// Required fields per document type (from perspective.yaml)
    pub required_fields: Option<HashMap<String, Vec<String>>>,
    /// Terms defined in glossary/definitions docs (suppresses ambiguous questions)
    pub defined_terms: HashSet<String>,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            stale_threshold: 365,
            required_fields: None,
            defined_terms: HashSet::new(),
        }
    }
}

/// Generate review questions for a document's content.
///
/// This function generates questions based on:
/// - Missing temporal tags
/// - Temporal conflicts
/// - Missing source references
/// - Ambiguous phrasing
/// - Stale content
/// - Required fields (if configured)
///
/// Note: Duplicate detection requires database access and should be handled separately.
///
/// # Arguments
/// * `content` - Document content to analyze
/// * `doc_type` - Optional document type for required field checks
/// * `config` - Configuration for question generation
///
/// # Returns
/// Vector of new questions (excludes questions already in the document's review queue)
#[cfg(test)]
pub fn generate_questions_for_content(
    content: &str,
    doc_type: Option<&str>,
    config: &ReviewConfig,
) -> Vec<ReviewQuestion> {
    let body = factbase::content_body(content);

    let mut new_questions = run_generators(body, doc_type, &config.defined_terms, config.stale_threshold, true);

    // Deduplicate: stale subsumes temporal for the same line
    let stale_lines: HashSet<_> = new_questions
        .iter()
        .filter(|q| q.question_type == QuestionType::Stale)
        .filter_map(|q| q.line_ref)
        .collect();
    new_questions.retain(|q| {
        !(q.question_type == QuestionType::Temporal
            && matches!(q.line_ref, Some(lr) if stale_lines.contains(&lr)))
    });

    if let Some(ref required_fields) = config.required_fields {
        new_questions.extend(generate_required_field_questions(
            body,
            doc_type,
            required_fields,
        ));
    }

    filter_sequential_conflicts(body, &mut new_questions);
    filter_existing_questions(content, new_questions)
}

/// Generate questions and prune stale ones from the document content.
///
/// Returns `(new_questions, pruned_content, pruned_count)`.
/// `pruned_content` has stale unanswered questions removed.
pub fn generate_and_prune(
    content: &str,
    doc_type: Option<&str>,
    config: &ReviewConfig,
) -> (Vec<ReviewQuestion>, String, usize) {
    let body = factbase::content_body(content);

    let mut all_generated = run_generators(body, doc_type, &config.defined_terms, config.stale_threshold, true);
    if let Some(ref required_fields) = config.required_fields {
        all_generated.extend(generate_required_field_questions(
            body,
            doc_type,
            required_fields,
        ));
    }

    // Post-filter: remove conflict questions for boundary-month sequential entries
    filter_sequential_conflicts(body, &mut all_generated);

    let valid_descriptions: HashSet<_> =
        all_generated.iter().map(|q| q.description.clone()).collect();

    // Count existing unanswered before pruning
    let existing_unanswered = parse_review_queue(content)
        .unwrap_or_default()
        .iter()
        .filter(|q| !q.answered)
        .count();

    // Prune stale questions (no cross-check in CLI path without --cross-check)
    let pruned_content = prune_stale_questions(content, &valid_descriptions, false);

    let remaining_unanswered = parse_review_queue(&pruned_content)
        .unwrap_or_default()
        .iter()
        .filter(|q| !q.answered)
        .count();
    let pruned_count = existing_unanswered - remaining_unanswered;

    // Dedup against remaining questions
    let new_questions = filter_existing_questions(&pruned_content, all_generated);

    (new_questions, pruned_content, pruned_count)
}

/// Add duplicate questions to an existing question list.
///
/// This is separate from `generate_questions_for_content` because duplicate
/// detection requires database access.
///
/// # Arguments
/// * `questions` - Mutable vector to add duplicate questions to
/// * `similar_docs` - List of (id, title, similarity) tuples from database
pub fn add_duplicate_questions(
    questions: &mut Vec<ReviewQuestion>,
    similar_docs: &[(String, String, f32)],
) {
    questions.extend(generate_duplicate_questions(similar_docs));
}

/// Filter out questions that already exist in the document's review queue.
///
/// # Arguments
/// * `content` - Document content containing existing review queue
/// * `questions` - Questions to filter
///
/// # Returns
/// Questions that don't already exist in the document
pub fn filter_existing_questions(
    content: &str,
    questions: Vec<ReviewQuestion>,
) -> Vec<ReviewQuestion> {
    let existing_questions = parse_review_queue(content).unwrap_or_default();
    let existing_descriptions: HashSet<_> =
        existing_questions.iter().map(|q| &q.description).collect();
    // Build a set of normalized conflict descriptions so that line-number
    // shifts (from footnote additions etc.) don't cause duplicate conflicts.
    let existing_conflict_normalized: HashSet<_> = existing_questions
        .iter()
        .filter(|q| q.question_type == QuestionType::Conflict)
        .map(|q| normalize_conflict_desc(&q.description))
        .collect();

    questions
        .into_iter()
        .filter(|q| {
            if existing_descriptions.contains(&q.description) {
                return false;
            }
            // For conflict questions, also check normalized (line-number-stripped) match
            if q.question_type == QuestionType::Conflict
                && existing_conflict_normalized.contains(normalize_conflict_desc(&q.description))
            {
                return false;
            }
            true
        })
        .collect()
}

/// Count fact lines with recent reviewed markers (within 180 days).
pub fn count_reviewed_facts(content: &str) -> usize {
    use chrono::Utc;
    use factbase::{extract_reviewed_date, FACT_LINE_REGEX};

    let today = Utc::now().date_naive();
    content
        .lines()
        .filter(|line| FACT_LINE_REGEX.is_match(line))
        .filter(|line| extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= 180))
        .count()
}

/// Count questions suppressed by reviewed/sequential markers.
///
/// Strips all suppression markers, re-generates questions, and compares
/// with the normal (marker-respecting) generation to measure the delta.
pub fn count_suppressed_questions(content: &str, max_age: Option<i64>) -> usize {
    let stale_threshold = max_age.unwrap_or(365);
    let config = ReviewConfig {
        stale_threshold,
        required_fields: None,
        ..Default::default()
    };

    let (normal, _, _) = generate_and_prune(content, None, &config);
    let stripped = factbase::strip_reviewed_markers(content);
    let (unrestricted, _, _) = generate_and_prune(&stripped, None, &config);
    unrestricted.len().saturating_sub(normal.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_questions_empty_content() {
        let config = ReviewConfig::default();
        let questions = generate_questions_for_content("", None, &config);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_questions_with_temporal_issues() {
        let content = "- Some fact without temporal tag\n- Another fact";
        let config = ReviewConfig::default();
        let questions = generate_questions_for_content(content, None, &config);
        // Should generate temporal questions for facts without tags
        assert!(!questions.is_empty());
    }

    #[test]
    fn test_filter_existing_questions() {
        let content = r#"
# Test Doc

- Some fact

<!-- factbase:review -->
## Review Queue

- [ ] `@q[temporal]` Line 3: "Some fact" - when was this true?
  > 
"#;
        // The description in the parsed question has "Line N:" stripped
        let questions = vec![ReviewQuestion {
            question_type: factbase::QuestionType::Temporal,
            line_ref: Some(3),
            description: "\"Some fact\" - when was this true?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
        }];

        let filtered = filter_existing_questions(content, questions);
        assert!(filtered.is_empty(), "Should filter out existing question");
    }

    #[test]
    fn test_question_type_as_str() {
        assert_eq!(factbase::QuestionType::Temporal.as_str(), "temporal");
        assert_eq!(factbase::QuestionType::Conflict.as_str(), "conflict");
        assert_eq!(factbase::QuestionType::Missing.as_str(), "missing");
        assert_eq!(factbase::QuestionType::Ambiguous.as_str(), "ambiguous");
        assert_eq!(factbase::QuestionType::Stale.as_str(), "stale");
        assert_eq!(factbase::QuestionType::Duplicate.as_str(), "duplicate");
    }

    #[test]
    fn test_review_config_default() {
        let config = ReviewConfig::default();
        assert_eq!(config.stale_threshold, 365);
        assert!(config.required_fields.is_none());
    }

    #[test]
    fn test_count_reviewed_facts_recent() {
        let today = chrono::Utc::now().format("%Y-%m-%d");
        let content = format!(
            "- Fact one <!-- reviewed:{today} -->\n- Fact two\n- Fact three <!-- reviewed:{today} -->\n"
        );
        assert_eq!(count_reviewed_facts(&content), 2);
    }

    #[test]
    fn test_count_reviewed_facts_old() {
        let content = "- Fact one <!-- reviewed:2020-01-01 -->\n- Fact two\n";
        assert_eq!(count_reviewed_facts(content), 0);
    }

    #[test]
    fn test_count_reviewed_facts_none() {
        let content = "- Fact one\n- Fact two\n";
        assert_eq!(count_reviewed_facts(content), 0);
    }

    #[test]
    fn test_generate_and_prune_suppresses_boundary_month_conflicts() {
        // LinkedIn pattern: sequential roles with shared boundary dates
        let content = "# Person\n\n## Career\n\
            - Engineer at Acme @t[2018..2020-06]\n\
            - CTO at BigCo @t[2020-06..2023]\n";
        let config = ReviewConfig::default();
        let (questions, _, _) = generate_and_prune(content, Some("person"), &config);
        let conflict_questions: Vec<_> = questions
            .iter()
            .filter(|q| q.question_type == QuestionType::Conflict)
            .collect();
        assert!(
            conflict_questions.is_empty(),
            "Boundary-month sequential roles should not generate conflict questions via generate_and_prune, got {}",
            conflict_questions.len()
        );
    }

    #[test]
    fn test_filter_existing_conflict_with_shifted_line_numbers() {
        // Existing conflict question has (line:5) but new one has (line:7) due to
        // footnotes being added. The normalized description should still match.
        let content = r#"
# Person

## Career
- Role A @t[2018..2022]
- Role B @t[2020..2024]

<!-- factbase:review -->
## Review Queue

- [ ] `@q[conflict]` "Role A" @t[2018..2022] overlaps with "Role B" @t[2020..2024] - were both true simultaneously? (line:5)
  > 
"#;
        let questions = vec![ReviewQuestion {
            question_type: QuestionType::Conflict,
            line_ref: Some(4),
            // Same conflict but line number shifted from 5 to 7
            description: "\"Role A\" @t[2018..2022] overlaps with \"Role B\" @t[2020..2024] - were both true simultaneously? (line:7)".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
        }];

        let filtered = filter_existing_questions(content, questions);
        assert!(
            filtered.is_empty(),
            "Conflict question with shifted line number should be filtered as duplicate"
        );
    }

    #[test]
    fn test_count_suppressed_questions_with_reviewed_markers() {
        let today = chrono::Utc::now().format("%Y-%m-%d");
        // Two facts: one with reviewed marker (suppressed), one without (generates question)
        let content = format!(
            "- Fact one without temporal tag <!-- reviewed:{today} -->\n- Fact two without temporal tag"
        );
        let suppressed = count_suppressed_questions(&content, None);
        // The reviewed fact would generate a temporal question if the marker were stripped
        assert!(
            suppressed >= 1,
            "Should count at least 1 suppressed question, got {suppressed}"
        );
    }

    #[test]
    fn test_count_suppressed_questions_no_markers() {
        let content = "- Fact one @t[2020..2022]\n- Fact two @t[2022..]";
        let suppressed = count_suppressed_questions(content, None);
        assert_eq!(suppressed, 0, "No markers means no suppression");
    }

    #[test]
    fn test_review_queue_entries_not_treated_as_facts() {
        // Regression: the checker was generating temporal questions about
        // answered review queue entries as if they were document facts.
        let content = r#"# Battle of Actium

- Date: 2 September 31 BCE @t[=31 BCE]
- Location: Ionian Sea

---

## Review Queue

<!-- factbase:review -->
- [x] `@q[temporal]` Line 10: "Date: 2 September 31 BCE" - when was this true?
  > Added @t[=31 BCE]
- [ ] `@q[missing]` Line 4: "Location: Ionian Sea" - what is the source?
  > 
"#;
        let config = ReviewConfig::default();
        let questions = generate_questions_for_content(content, None, &config);
        // No question should reference review queue text
        for q in &questions {
            assert!(
                !q.description.contains("@q["),
                "Generated question references review queue entry: {}",
                q.description
            );
        }
    }
}
