//! Answer interpretation for review questions.

use crate::patterns::{QUOTED_TEXT_REGEX, TEMPORAL_TAG_CONTENT_REGEX};
use crate::ReviewQuestion;

use super::temporal::{extract_dates_from_answer, format_new_temporal_tag, format_temporal_tag};
use super::ChangeInstruction;

/// Interpret an answer to determine the change instruction
pub fn interpret_answer(question: &ReviewQuestion, answer: &str) -> ChangeInstruction {
    let answer_lower = answer.trim().to_lowercase();

    // Check for special keywords
    if answer_lower == "dismiss" || answer_lower == "ignore" {
        return ChangeInstruction::Dismiss;
    }

    if answer_lower == "delete" {
        let line_text = extract_quoted_text(&question.description).unwrap_or_default();
        return ChangeInstruction::Delete { line_text };
    }

    if answer_lower.starts_with("split:") {
        let line_text = extract_quoted_text(&question.description).unwrap_or_default();
        let instruction = answer[6..].trim().to_string();
        return ChangeInstruction::Split {
            line_text,
            instruction,
        };
    }

    // Try to extract date information for temporal questions
    let line_text = extract_quoted_text(&question.description).unwrap_or_default();
    let old_tag = extract_temporal_tag(&question.description);

    if let Some(dates) = extract_dates_from_answer(answer) {
        if let Some(ref old) = old_tag {
            // Update existing temporal tag
            let new_tag = format_temporal_tag(&dates, old);
            return ChangeInstruction::UpdateTemporal {
                line_text,
                old_tag: old.clone(),
                new_tag,
            };
        } else {
            // Add new temporal tag
            let tag = format_new_temporal_tag(&dates);
            return ChangeInstruction::AddTemporal { line_text, tag };
        }
    }

    // Fall back to generic change
    ChangeInstruction::Generic {
        description: format!("Apply answer '{}' to: {}", answer, question.description),
    }
}

/// Extract quoted text from a question description
fn extract_quoted_text(description: &str) -> Option<String> {
    QUOTED_TEXT_REGEX
        .captures(description)
        .map(|c| c[1].to_string())
}

/// Extract temporal tag from text
fn extract_temporal_tag(text: &str) -> Option<String> {
    TEMPORAL_TAG_CONTENT_REGEX
        .captures(text)
        .map(|c| format!("@t[{}]", &c[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuestionType;

    #[test]
    fn test_interpret_answer_dismiss() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "test".to_string(),
            answered: true,
            answer: Some("dismiss".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "dismiss");
        assert!(matches!(result, ChangeInstruction::Dismiss));
    }

    #[test]
    fn test_interpret_answer_ignore() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "test".to_string(),
            answered: true,
            answer: Some("ignore".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "IGNORE");
        assert!(matches!(result, ChangeInstruction::Dismiss));
    }

    #[test]
    fn test_interpret_answer_delete() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""Some fact" - when?"#.to_string(),
            answered: true,
            answer: Some("delete".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "delete");
        match result {
            ChangeInstruction::Delete { line_text } => {
                assert_eq!(line_text, "Some fact");
            }
            _ => panic!("Expected Delete instruction"),
        }
    }

    #[test]
    fn test_interpret_answer_split() {
        let q = ReviewQuestion {
            question_type: QuestionType::Ambiguous,
            line_ref: Some(5),
            description: r#""Engineer then Lead" - clarify"#.to_string(),
            answered: true,
            answer: Some("split: separate roles".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "split: separate roles");
        match result {
            ChangeInstruction::Split {
                line_text,
                instruction,
            } => {
                assert_eq!(line_text, "Engineer then Lead");
                assert_eq!(instruction, "separate roles");
            }
            _ => panic!("Expected Split instruction"),
        }
    }

    #[test]
    fn test_interpret_answer_end_date() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""VP at BigCo @t[2022..]" - still current?"#.to_string(),
            answered: true,
            answer: Some("No, left March 2024".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "No, left March 2024");
        match result {
            ChangeInstruction::UpdateTemporal {
                line_text,
                old_tag,
                new_tag,
            } => {
                assert_eq!(line_text, "VP at BigCo @t[2022..]");
                assert_eq!(old_tag, "@t[2022..]");
                assert_eq!(new_tag, "@t[2022..2024-03]");
            }
            _ => panic!("Expected UpdateTemporal instruction"),
        }
    }
}
