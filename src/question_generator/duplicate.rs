//! Duplicate question generation.
//!
//! Generates `@q[duplicate]` questions for documents with high similarity
//! to other documents.

use crate::models::{QuestionType, ReviewQuestion};

/// Generate duplicate questions for a document based on similar documents found.
///
/// Takes a list of similar documents (id, title, similarity) from database lookup.
/// Generates `@q[duplicate]` questions for each similar document above threshold.
///
/// Returns a list of `ReviewQuestion` with `question_type = Duplicate`.
pub fn generate_duplicate_questions(similar_docs: &[(String, String, f32)]) -> Vec<ReviewQuestion> {
    similar_docs
        .iter()
        .map(|(similar_id, similar_title, similarity)| {
            ReviewQuestion::new(
                QuestionType::Duplicate,
                None,
                format!(
                    "This document may be a duplicate of \"{}\" [{}] ({:.0}% similar) - please verify",
                    similar_title,
                    similar_id,
                    similarity * 100.0
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_duplicate_questions_empty() {
        let similar_docs: Vec<(String, String, f32)> = vec![];
        let questions = generate_duplicate_questions(&similar_docs);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_duplicate_questions_single() {
        let similar_docs = vec![("abc123".to_string(), "Similar Doc".to_string(), 0.97)];
        let questions = generate_duplicate_questions(&similar_docs);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Duplicate);
        assert!(questions[0].line_ref.is_none()); // Duplicates apply to whole doc
        assert!(questions[0].description.contains("Similar Doc"));
        assert!(questions[0].description.contains("abc123"));
        assert!(questions[0].description.contains("97%"));
    }

    #[test]
    fn test_generate_duplicate_questions_multiple() {
        let similar_docs = vec![
            ("doc1".to_string(), "First Similar".to_string(), 0.98),
            ("doc2".to_string(), "Second Similar".to_string(), 0.96),
        ];
        let questions = generate_duplicate_questions(&similar_docs);
        assert_eq!(questions.len(), 2);
        assert!(questions[0].description.contains("First Similar"));
        assert!(questions[1].description.contains("Second Similar"));
    }

    #[test]
    fn test_generate_duplicate_questions_format() {
        let similar_docs = vec![("xyz789".to_string(), "Test Doc".to_string(), 0.955)];
        let questions = generate_duplicate_questions(&similar_docs);
        assert_eq!(questions.len(), 1);
        // Check format: "This document may be a duplicate of "Test Doc" [xyz789] (96% similar) - please verify"
        let desc = &questions[0].description;
        assert!(desc.starts_with("This document may be a duplicate of"));
        assert!(desc.contains("\"Test Doc\""));
        assert!(desc.contains("[xyz789]"));
        assert!(desc.contains("96%")); // 0.955 rounds to 96%
        assert!(desc.ends_with("please verify"));
    }
}
