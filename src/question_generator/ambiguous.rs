//! Ambiguous question generation.
//!
//! Generates `@q[ambiguous]` questions for unclear phrasing
//! that needs clarification.

use chrono::Utc;

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::extract_reviewed_date;

use super::iter_fact_lines;

/// Default number of days a reviewed marker suppresses question regeneration.
const REVIEWED_SKIP_DAYS: i64 = 180;

/// Generate ambiguous questions for a document.
///
/// Detects facts with unclear phrasing that needs clarification:
/// 1. Locations without context (could be home, work, or other)
/// 2. Relationships without direction (e.g., "knows John" - professional or personal?)
/// 3. Vague pronouns or references
///
/// Returns a list of `ReviewQuestion` with `question_type = Ambiguous`.
pub fn generate_ambiguous_questions(content: &str) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();
    let today = Utc::now().date_naive();

    for (line_number, line, fact_text) in iter_fact_lines(content) {
        // Skip facts with a recent reviewed marker
        if extract_reviewed_date(line).is_some_and(|d| (today - d).num_days() <= REVIEWED_SKIP_DAYS)
        {
            continue;
        }

        // Check for ambiguous location (no context like "home", "work", "office")
        let ambiguity = detect_ambiguous_location(&fact_text)
            .or_else(|| detect_ambiguous_relationship(&fact_text));

        if let Some(ambiguity) = ambiguity {
            questions.push(ReviewQuestion::new(
                QuestionType::Ambiguous,
                Some(line_number),
                format!("\"{fact_text}\" - {ambiguity}"),
            ));
        }
    }

    questions
}

/// Detect ambiguous location references.
/// Returns Some(clarification_question) if ambiguous, None otherwise.
fn detect_ambiguous_location(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();

    // Location phrases that need context
    let location_phrases = ["lives in", "based in", "located in", "resides in"];

    // Context words that clarify the location type
    let context_words = [
        "home",
        "work",
        "office",
        "headquarters",
        "hq",
        "remote",
        "primary",
        "secondary",
        "main",
    ];

    // Check if it's a location statement
    let is_location = location_phrases.iter().any(|p| lower.contains(p));
    if !is_location {
        return None;
    }

    // Check if context is provided
    let has_context = context_words.iter().any(|c| lower.contains(c));
    if has_context {
        return None;
    }

    Some("is this home, work, or another type of location?")
}

/// Detect ambiguous relationship references.
/// Returns Some(clarification_question) if ambiguous, None otherwise.
fn detect_ambiguous_relationship(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();

    // Vague relationship phrases
    let vague_relationships = [
        ("knows ", "is this a professional or personal relationship?"),
        ("connected to ", "what is the nature of this connection?"),
        (
            "associated with ",
            "what is the nature of this association?",
        ),
        (
            "works with ",
            "is this a direct colleague, collaborator, or other?",
        ),
        ("met ", "in what context did they meet?"),
    ];

    for (phrase, question) in vague_relationships {
        if lower.contains(phrase) {
            // Check if there's already clarifying context
            let clarifiers = [
                "colleague",
                "friend",
                "mentor",
                "manager",
                "report",
                "client",
                "partner",
                "investor",
                "advisor",
                "board",
                "professional",
                "personal",
            ];
            let has_clarifier = clarifiers.iter().any(|c| lower.contains(c));
            if !has_clarifier {
                return Some(question);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ambiguous_questions_no_facts() {
        let content = "# Title\n\nSome paragraph text.";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_location_without_context() {
        let content = "# Person\n\n- Lives in San Francisco";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Ambiguous);
        assert!(questions[0].description.contains("home, work, or another"));
    }

    #[test]
    fn test_generate_ambiguous_questions_location_with_context() {
        let content = "# Person\n\n- Lives in San Francisco (home)";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_location_work_context() {
        let content = "# Person\n\n- Based in NYC office";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_relationship_vague() {
        let content = "# Person\n\n- Knows John Smith";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Ambiguous);
        assert!(questions[0]
            .description
            .contains("professional or personal"));
    }

    #[test]
    fn test_generate_ambiguous_questions_relationship_with_context() {
        let content = "# Person\n\n- Knows John Smith (colleague from Acme)";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_works_with_vague() {
        let content = "# Person\n\n- Works with Jane Doe";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert!(questions[0]
            .description
            .contains("direct colleague, collaborator"));
    }

    #[test]
    fn test_generate_ambiguous_questions_works_with_context() {
        let content = "# Person\n\n- Works with Jane Doe as her manager";
        let questions = generate_ambiguous_questions(content);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_ambiguous_questions_met_vague() {
        let content = "# Person\n\n- Met Bob at a conference";
        let questions = generate_ambiguous_questions(content);
        // "at a conference" provides context, but we still ask for more detail
        assert_eq!(questions.len(), 1);
        assert!(questions[0].description.contains("context"));
    }

    #[test]
    fn test_generate_ambiguous_questions_line_numbers() {
        let content = "# Person\n\n- Clear fact\n- Lives in Boston\n- Another clear fact";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].line_ref, Some(4));
    }

    #[test]
    fn test_generate_ambiguous_questions_multiple() {
        let content = "# Person\n\n- Lives in NYC\n- Knows Jane";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(questions.len(), 2);
    }

    #[test]
    fn test_detect_ambiguous_location_various_phrases() {
        assert!(detect_ambiguous_location("Lives in NYC").is_some());
        assert!(detect_ambiguous_location("Based in London").is_some());
        assert!(detect_ambiguous_location("Located in Paris").is_some());
        assert!(detect_ambiguous_location("Resides in Tokyo").is_some());
    }

    #[test]
    fn test_detect_ambiguous_location_with_clarifiers() {
        assert!(detect_ambiguous_location("Lives in NYC (home)").is_none());
        assert!(detect_ambiguous_location("Based in London office").is_none());
        assert!(detect_ambiguous_location("Located in Paris headquarters").is_none());
        assert!(detect_ambiguous_location("Primary residence in Tokyo").is_none());
    }

    #[test]
    fn test_detect_ambiguous_relationship_various() {
        assert!(detect_ambiguous_relationship("Knows John").is_some());
        assert!(detect_ambiguous_relationship("Connected to Jane").is_some());
        assert!(detect_ambiguous_relationship("Associated with Acme").is_some());
        assert!(detect_ambiguous_relationship("Works with Bob").is_some());
    }

    #[test]
    fn test_detect_ambiguous_relationship_with_clarifiers() {
        assert!(detect_ambiguous_relationship("Knows John (colleague)").is_none());
        assert!(detect_ambiguous_relationship("Connected to Jane as mentor").is_none());
        assert!(detect_ambiguous_relationship("Works with Bob as his manager").is_none());
        assert!(detect_ambiguous_relationship("Met Jane, now a close friend").is_none());
    }

    #[test]
    fn test_reviewed_marker_suppresses_ambiguous() {
        let today = Utc::now().date_naive();
        let marker_date = today - chrono::Duration::days(30);
        let content = format!(
            "# Person\n\n- Lives in San Francisco <!-- reviewed:{} -->",
            marker_date.format("%Y-%m-%d")
        );
        let questions = generate_ambiguous_questions(&content);
        assert!(
            questions.is_empty(),
            "Recent reviewed marker should suppress ambiguous question"
        );
    }

    #[test]
    fn test_old_reviewed_marker_still_generates_ambiguous() {
        let content = "# Person\n\n- Lives in San Francisco <!-- reviewed:2020-01-01 -->";
        let questions = generate_ambiguous_questions(content);
        assert_eq!(
            questions.len(),
            1,
            "Old reviewed marker should not suppress ambiguous question"
        );
    }
}
