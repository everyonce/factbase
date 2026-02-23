//! Answer interpretation for review questions.

use crate::patterns::{DATE_EXTRACT_REGEX, QUOTED_TEXT_REGEX, TEMPORAL_TAG_CONTENT_REGEX};
use crate::ReviewQuestion;

use super::temporal::{extract_dates_from_answer, format_new_temporal_tag, format_temporal_tag};
use super::{AnswerType, ChangeInstruction};

/// Source-like prefixes that indicate a citation rather than a correction.
const SOURCE_PREFIXES: &[&str] = &["per ", "via ", "from ", "source:", "source: "];

/// Confirmation keywords (exact match after lowercasing and trimming).
const CONFIRMATION_EXACT: &[&str] = &[
    "confirmed",
    "still accurate",
    "still current",
    "still valid",
    "yes",
    "yes, verified",
    "verified",
    "accurate",
];

/// Classify an answer into a structured type for deterministic handling.
pub fn classify_answer(answer: &str) -> AnswerType {
    let trimmed = answer.trim();
    let lower = trimmed.to_lowercase();

    // Dismissal
    if lower == "dismiss"
        || lower == "ignore"
        || lower.starts_with("not a conflict")
        || lower.starts_with("no conflict")
        || lower.starts_with("not conflicting")
    {
        return AnswerType::Dismissal;
    }

    // Deletion
    if lower == "delete" || lower == "remove" {
        return AnswerType::Deletion;
    }

    // Deferral
    if lower == "defer"
        || lower == "later"
        || lower.starts_with("defer ")
        || lower.starts_with("needs ")
        || lower.starts_with("check later")
    {
        return AnswerType::Deferral;
    }

    // Correction (explicit prefix)
    if let Some(rest) = lower
        .strip_prefix("correct:")
        .or_else(|| lower.strip_prefix("correction:"))
    {
        return AnswerType::Correction {
            detail: trimmed[trimmed.len() - rest.len()..].trim().to_string(),
        };
    }

    // Confirmation (exact keywords or "yes" prefix with short answer)
    if CONFIRMATION_EXACT.contains(&lower.as_str())
        || (lower.starts_with("yes") && trimmed.len() < 30)
    {
        return AnswerType::Confirmation;
    }

    // Source citation: "per ...", "via ...", or contains a date-like pattern without correction indicators
    if SOURCE_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        let source_text = trimmed;
        let date = extract_date_string(source_text);
        let source = date.as_ref().map_or(source_text.to_string(), |d| {
            source_text
                .replace(d, "")
                .replace(',', "")
                .trim()
                .to_string()
        });
        return AnswerType::SourceCitation { source, date };
    }

    // Source citation heuristic: looks like "SourceName, YYYY-MM-DD" or "SourceName YYYY-MM"
    if let Some(date) = extract_date_string(trimmed) {
        let source = trimmed
            .replace(&date, "")
            .replace(',', "")
            .trim()
            .to_string();
        if !source.is_empty() && !has_correction_indicators(&lower) {
            return AnswerType::SourceCitation {
                source,
                date: Some(date),
            };
        }
    }

    // Fallback: treat as correction
    AnswerType::Correction {
        detail: trimmed.to_string(),
    }
}

/// Extract a date-like string (YYYY-MM-DD, YYYY-MM, or YYYY) from text.
fn extract_date_string(text: &str) -> Option<String> {
    DATE_EXTRACT_REGEX
        .find(text)
        .map(|m| m.as_str().to_string())
}

/// Check if lowercased text contains indicators of a factual correction.
fn has_correction_indicators(lower: &str) -> bool {
    lower.contains("no,")
        || lower.contains("left")
        || lower.contains("ended")
        || lower.contains("started")
        || lower.contains("changed")
        || lower.contains("moved")
        || lower.contains("actually")
}

/// Interpret an answer to determine the change instruction.
///
/// Calls `classify_answer()` first, then maps `AnswerType` → `ChangeInstruction`.
pub fn interpret_answer(question: &ReviewQuestion, answer: &str) -> ChangeInstruction {
    let line_text = extract_quoted_text(&question.description).unwrap_or_default();
    let old_tag = extract_temporal_tag(&question.description);

    match classify_answer(answer) {
        AnswerType::Dismissal => ChangeInstruction::Dismiss,
        AnswerType::Deferral => ChangeInstruction::Defer,
        AnswerType::Deletion => ChangeInstruction::Delete { line_text },
        AnswerType::SourceCitation { source, date } => {
            let source_info = match date {
                Some(d) => format!("{source}, {d}"),
                None => source,
            };
            ChangeInstruction::AddSource {
                line_text,
                source_info,
            }
        }
        AnswerType::Confirmation => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            if let Some(ref old) = old_tag {
                let new_tag = format!("@t[~{today}]");
                ChangeInstruction::UpdateTemporal {
                    line_text,
                    old_tag: old.clone(),
                    new_tag,
                }
            } else {
                ChangeInstruction::AddTemporal {
                    line_text,
                    tag: format!("@t[~{today}]"),
                }
            }
        }
        AnswerType::Correction { detail } => {
            // Handle "split:" prefix within corrections
            if detail.to_lowercase().starts_with("split:") {
                return ChangeInstruction::Split {
                    line_text,
                    instruction: detail["split:".len()..].trim().to_string(),
                };
            }

            // Try to extract date information for temporal updates
            if let Some(dates) = extract_dates_from_answer(answer) {
                if let Some(ref old) = old_tag {
                    let new_tag = format_temporal_tag(&dates, old);
                    return ChangeInstruction::UpdateTemporal {
                        line_text,
                        old_tag: old.clone(),
                        new_tag,
                    };
                }
                let tag = format_new_temporal_tag(&dates);
                return ChangeInstruction::AddTemporal { line_text, tag };
            }

            ChangeInstruction::Generic {
                description: format!("Apply answer '{}' to: {}", answer, question.description),
            }
        }
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

    // --- classify_answer tests ---

    #[test]
    fn test_classify_dismissal() {
        assert_eq!(classify_answer("dismiss"), AnswerType::Dismissal);
        assert_eq!(classify_answer("ignore"), AnswerType::Dismissal);
        assert_eq!(classify_answer("DISMISS"), AnswerType::Dismissal);
        assert_eq!(classify_answer("  Ignore  "), AnswerType::Dismissal);
        assert_eq!(classify_answer("not a conflict"), AnswerType::Dismissal);
        assert_eq!(
            classify_answer("Not a conflict. Promotion from Director to Senior Director"),
            AnswerType::Dismissal
        );
        assert_eq!(
            classify_answer("no conflict - sequential roles with shared boundary month"),
            AnswerType::Dismissal
        );
    }

    #[test]
    fn test_classify_deletion() {
        assert_eq!(classify_answer("delete"), AnswerType::Deletion);
        assert_eq!(classify_answer("remove"), AnswerType::Deletion);
        assert_eq!(classify_answer("DELETE"), AnswerType::Deletion);
    }

    #[test]
    fn test_classify_deferral() {
        assert_eq!(classify_answer("defer"), AnswerType::Deferral);
        assert_eq!(classify_answer("later"), AnswerType::Deferral);
        assert_eq!(
            classify_answer("needs re-verification"),
            AnswerType::Deferral
        );
        assert_eq!(classify_answer("check later"), AnswerType::Deferral);
        assert_eq!(
            classify_answer("defer until next quarter"),
            AnswerType::Deferral
        );
    }

    #[test]
    fn test_classify_confirmation() {
        assert_eq!(classify_answer("confirmed"), AnswerType::Confirmation);
        assert_eq!(classify_answer("still accurate"), AnswerType::Confirmation);
        assert_eq!(classify_answer("yes, verified"), AnswerType::Confirmation);
        assert_eq!(classify_answer("still current"), AnswerType::Confirmation);
        assert_eq!(classify_answer("still valid"), AnswerType::Confirmation);
        assert_eq!(classify_answer("verified"), AnswerType::Confirmation);
        assert_eq!(classify_answer("accurate"), AnswerType::Confirmation);
        assert_eq!(classify_answer("yes"), AnswerType::Confirmation);
        assert_eq!(classify_answer("CONFIRMED"), AnswerType::Confirmation);
    }

    #[test]
    fn test_classify_yes_short_is_confirmation() {
        // "yes" prefix with short answer (<30 chars) → Confirmation
        assert_eq!(classify_answer("yes, checked"), AnswerType::Confirmation);
    }

    #[test]
    fn test_classify_source_citation_prefixes() {
        let result = classify_answer("per annual report");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: None } if source.contains("annual report"))
        );

        let result = classify_answer("via LinkedIn profile");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: None } if source.contains("LinkedIn"))
        );

        let result = classify_answer("from internal wiki");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: None } if source.contains("internal wiki"))
        );

        let result = classify_answer("source: team meeting notes");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: None } if source.contains("team meeting notes"))
        );
    }

    #[test]
    fn test_classify_source_citation_with_date() {
        let result = classify_answer("LinkedIn, 2026-01");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: Some(ref d) }
            if source.contains("LinkedIn") && d == "2026-01")
        );

        let result = classify_answer("Phonetool lookup 2026-02-10");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: Some(ref d) }
            if source.contains("Phonetool") && d == "2026-02-10")
        );

        let result = classify_answer("per LinkedIn profile, 2026-01");
        assert!(
            matches!(result, AnswerType::SourceCitation { ref source, date: Some(ref d) }
            if source.contains("LinkedIn") && d == "2026-01")
        );
    }

    #[test]
    fn test_classify_correction_explicit_prefix() {
        let result = classify_answer("correct: title is now Senior VP");
        assert!(
            matches!(result, AnswerType::Correction { ref detail } if detail == "title is now Senior VP")
        );

        let result = classify_answer("correction: left in 2025");
        assert!(
            matches!(result, AnswerType::Correction { ref detail } if detail == "left in 2025")
        );
    }

    #[test]
    fn test_classify_correction_indicators_prevent_source() {
        // "No, left March 2024" has a date but also correction indicators → Correction fallback
        let result = classify_answer("No, left March 2024");
        assert!(matches!(result, AnswerType::Correction { .. }));

        let result = classify_answer("Actually changed to Director 2025-01");
        assert!(matches!(result, AnswerType::Correction { .. }));

        let result = classify_answer("No, moved to Seattle 2025-06");
        assert!(matches!(result, AnswerType::Correction { .. }));
    }

    #[test]
    fn test_classify_correction_fallback() {
        // Long free-text that doesn't match any pattern → Correction
        let result = classify_answer(
            "The role was restructured and the title changed to Principal Engineer",
        );
        assert!(matches!(result, AnswerType::Correction { ref detail }
            if detail == "The role was restructured and the title changed to Principal Engineer"));
    }

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

    #[test]
    fn test_interpret_answer_deferral() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Some fact" - still valid?"#.to_string(),
            answered: true,
            answer: Some("defer".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "defer");
        assert!(matches!(result, ChangeInstruction::Defer));
    }

    #[test]
    fn test_interpret_answer_needs_deferral() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: "test".to_string(),
            answered: true,
            answer: Some("needs re-verification".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "needs re-verification");
        assert!(matches!(result, ChangeInstruction::Defer));
    }

    #[test]
    fn test_interpret_answer_source_citation() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Unsourced claim" - add source"#.to_string(),
            answered: true,
            answer: Some("per LinkedIn profile".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "per LinkedIn profile");
        match result {
            ChangeInstruction::AddSource {
                line_text,
                source_info,
            } => {
                assert_eq!(line_text, "Unsourced claim");
                assert!(source_info.contains("LinkedIn"));
            }
            _ => panic!("Expected AddSource, got {result:?}"),
        }
    }

    #[test]
    fn test_interpret_answer_source_with_date() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Some fact" - source?"#.to_string(),
            answered: true,
            answer: Some("Phonetool lookup, 2026-02-10".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "Phonetool lookup, 2026-02-10");
        match result {
            ChangeInstruction::AddSource { source_info, .. } => {
                assert!(source_info.contains("Phonetool"));
                assert!(source_info.contains("2026-02-10"));
            }
            _ => panic!("Expected AddSource, got {result:?}"),
        }
    }

    #[test]
    fn test_interpret_answer_confirmation_with_tag() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""VP at BigCo @t[~2024-01]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("confirmed".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "confirmed");
        match result {
            ChangeInstruction::UpdateTemporal {
                old_tag, new_tag, ..
            } => {
                assert_eq!(old_tag, "@t[~2024-01]");
                assert!(new_tag.starts_with("@t[~"));
            }
            _ => panic!("Expected UpdateTemporal, got {result:?}"),
        }
    }

    #[test]
    fn test_interpret_answer_confirmation_without_tag() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""Some fact" - when?"#.to_string(),
            answered: true,
            answer: Some("yes".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "yes");
        match result {
            ChangeInstruction::AddTemporal { tag, .. } => {
                assert!(tag.starts_with("@t[~"));
            }
            _ => panic!("Expected AddTemporal, got {result:?}"),
        }
    }
}
