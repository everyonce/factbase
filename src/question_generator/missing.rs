//! Missing source question generation.
//!
//! Generates `@q[missing]` questions for facts without source references.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::SOURCE_REF_DETECT_REGEX;

use super::iter_fact_lines;

/// Generate missing source questions for a document.
///
/// Detects facts (list items) without any `[^N]` source references.
///
/// Returns a list of `ReviewQuestion` with `question_type = Missing`.
pub fn generate_missing_questions(content: &str) -> Vec<ReviewQuestion> {
    iter_fact_lines(content)
        .filter(|(_, line, _)| !SOURCE_REF_DETECT_REGEX.is_match(line))
        .map(|(line_number, _, fact_text)| {
            ReviewQuestion::new(
                QuestionType::Missing,
                Some(line_number),
                format!("\"{fact_text}\" - what is the source?"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_missing_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        let questions = generate_missing_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_missing_questions_fact_without_source() {
        let content = "# Person\n\n- Works at Acme Corp";
        let questions = generate_missing_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Missing);
        assert_eq!(questions[0].line_ref, Some(3));
        assert!(questions[0].description.contains("Works at Acme Corp"));
        assert!(questions[0].description.contains("what is the source?"));
    }

    #[test]
    fn test_generate_missing_questions_fact_with_source() {
        let content = "# Person\n\n- Works at Acme Corp [^1]\n\n[^1]: LinkedIn profile";
        let questions = generate_missing_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_missing_questions_multiple_facts() {
        let content = "# Person\n\n- Fact one\n- Fact two [^1]\n- Fact three\n\n[^1]: Source";
        let questions = generate_missing_questions(content);
        // Should have questions for facts without sources (line 3 and 5)
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].line_ref, Some(3));
        assert_eq!(questions[1].line_ref, Some(5));
    }

    #[test]
    fn test_generate_missing_questions_multiple_sources_on_line() {
        let content = "# Person\n\n- Complex fact [^1][^2]";
        let questions = generate_missing_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_missing_questions_line_numbers() {
        let content = "# Title\n\nParagraph\n\n- Fact one\n- Fact two";
        let questions = generate_missing_questions(content);
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].line_ref, Some(5));
        assert_eq!(questions[1].line_ref, Some(6));
    }

    #[test]
    fn test_generate_missing_questions_all_list_types() {
        let content = "- Dash fact\n* Asterisk fact\n1. Numbered dot\n2) Numbered paren";
        let questions = generate_missing_questions(content);
        assert_eq!(questions.len(), 4);
    }

    #[test]
    fn test_generate_missing_questions_indented_list() {
        let content = "# Doc\n\n  - Indented fact without source";
        let questions = generate_missing_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("Indented fact"));
    }
}
