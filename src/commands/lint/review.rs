//! Review question generation for lint.
//!
//! Contains logic for generating review questions during `lint --review`.
//! This module provides helper functions to generate questions for documents
//! based on various quality checks (temporal tags, sources, duplicates, etc.).

use factbase::{
    generate_ambiguous_questions, generate_conflict_questions, generate_duplicate_questions,
    generate_missing_questions, generate_required_field_questions, generate_stale_questions,
    generate_temporal_questions, parse_review_queue, QuestionType, ReviewQuestion,
};
use std::collections::{HashMap, HashSet};

/// Configuration for review question generation
pub struct ReviewConfig {
    /// Threshold in days for stale content detection
    pub stale_threshold: i64,
    /// Required fields per document type (from perspective.yaml)
    pub required_fields: Option<HashMap<String, Vec<String>>>,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            stale_threshold: 365,
            required_fields: None,
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
pub fn generate_questions_for_content(
    content: &str,
    doc_type: Option<&str>,
    config: &ReviewConfig,
) -> Vec<ReviewQuestion> {
    // Generate temporal questions (missing tags, stale ongoing)
    let mut new_questions = generate_temporal_questions(content);

    // Generate conflict questions (overlapping dates)
    new_questions.extend(generate_conflict_questions(content));

    // Generate missing source questions
    new_questions.extend(generate_missing_questions(content));

    // Generate ambiguous questions (unclear phrasing)
    new_questions.extend(generate_ambiguous_questions(content));

    // Generate stale questions (old sources or @t[~...] dates)
    new_questions.extend(generate_stale_questions(content, config.stale_threshold));

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

    // Generate required field questions (missing required fields per doc type)
    if let Some(ref required_fields) = config.required_fields {
        new_questions.extend(generate_required_field_questions(
            content,
            doc_type,
            required_fields,
        ));
    }

    // Filter out questions that already exist in the document
    filter_existing_questions(content, new_questions)
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

    questions
        .into_iter()
        .filter(|q| !existing_descriptions.contains(&q.description))
        .collect()
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
        // The description in the parsed question includes the full text after @q[type]
        let questions = vec![ReviewQuestion {
            question_type: factbase::QuestionType::Temporal,
            line_ref: Some(3),
            description: "Line 3: \"Some fact\" - when was this true?".to_string(),
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
}
