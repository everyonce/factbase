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
        || lower.starts_with("sequential")
        || lower.starts_with("roles are sequential")
        || lower.starts_with("boundary overlap")
        || lower.starts_with("boundary month")
        || lower.contains("not a conflict")
        || lower.contains("sequential roles")
        || lower.contains("boundary overlap")
        || lower.contains("boundary month")
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
        || lower.starts_with("defer:")
        || lower.starts_with("defer ")
        || lower.starts_with("deferred:")
        || lower.starts_with("deferred ")
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
                .replacen(d, "", 1)
                .replace(',', "")
                .trim()
                .to_string()
        });
        return AnswerType::SourceCitation { source, date };
    }

    // Source citation heuristic: looks like "SourceName, YYYY-MM-DD" or "SourceName YYYY-MM"
    if let Some(date) = extract_date_string(trimmed) {
        let source = trimmed
            .replacen(&date, "", 1)
            .replace(',', "")
            .trim()
            .to_string();
        if !source.is_empty() && !has_correction_indicators(&lower) {
            // If the non-date text is confirmation language, treat as confirmation
            // rather than injecting the answer as a garbage footnote.
            if has_confirmation_indicators(&source) {
                return AnswerType::Confirmation;
            }
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

/// Extract a verification date from answer text — only matches dates with month
/// precision or better (YYYY-MM-DD or YYYY-MM).  Bare years like "1998" are
/// excluded because they are almost always publication years from citations
/// (e.g. "Beard (1998)"), not verification dates.
fn extract_verification_date(text: &str) -> Option<String> {
    extract_date_string(text).filter(|d| d.contains('-'))
}

/// Check if text looks like confirmation language (e.g. "still current", "confirmed").
/// Used to prevent the date heuristic from misclassifying confirmations-with-dates
/// as source citations (the "garbage footnotes" bug).
fn has_confirmation_indicators(text: &str) -> bool {
    let lower = text.to_lowercase();
    let trimmed = lower.trim();
    CONFIRMATION_EXACT.iter().any(|kw| trimmed.starts_with(kw))
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

    let answer_type = classify_answer(answer);

    // Conflict questions ("were both true simultaneously?") should only produce
    // Dismiss, Defer, Delete, or date corrections.  Source citations and generic
    // corrections are meaningless for conflicts and cause footnote pollution.
    if question.question_type == crate::models::QuestionType::Conflict {
        return match answer_type {
            AnswerType::Dismissal | AnswerType::Confirmation => ChangeInstruction::Dismiss,
            AnswerType::Deferral => ChangeInstruction::Defer,
            AnswerType::Deletion => ChangeInstruction::Delete { line_text },
            AnswerType::Correction { .. } => {
                // Only apply if it contains actual date info; otherwise dismiss
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
                ChangeInstruction::Dismiss
            }
            // Source citations are never meaningful for conflict answers
            AnswerType::SourceCitation { .. } => ChangeInstruction::Dismiss,
        };
    }

    // Missing-source questions ("what is the source for this fact?") should only
    // produce Dismiss, Defer, or Delete.  Source citations from review answers
    // must NOT become footnote definitions — they cause garbage footnote
    // accumulation when questions are re-generated and re-answered.  Answers
    // belong in the review queue section, not as source citations.
    if question.question_type == crate::models::QuestionType::Missing {
        return match answer_type {
            AnswerType::Dismissal | AnswerType::Confirmation | AnswerType::SourceCitation { .. } => {
                ChangeInstruction::Dismiss
            }
            AnswerType::Deferral => ChangeInstruction::Defer,
            AnswerType::Deletion => ChangeInstruction::Delete { line_text },
            // Anything else (Correction fallback, etc.) is not a source — dismiss
            AnswerType::Correction { .. } => ChangeInstruction::Dismiss,
        };
    }

    // Stale questions ("is this still current?") should produce temporal updates
    // or dismissals.  Source citations are misclassified confirmations — long
    // answers with dates that corroborate a fact are confirmations, not new sources.
    if question.question_type == crate::models::QuestionType::Stale {
        return match answer_type {
            AnswerType::Dismissal => ChangeInstruction::Dismiss,
            AnswerType::Deferral => ChangeInstruction::Defer,
            AnswerType::Deletion => ChangeInstruction::Delete { line_text },
            // Treat source citations as confirmations — the user is confirming
            // the fact is still current with evidence, not providing a new source.
            // Use verification date (month precision+) to avoid extracting
            // publication years from citations like "Beard (1998)".
            AnswerType::Confirmation | AnswerType::SourceCitation { .. } => {
                let date_str = extract_verification_date(answer)
                    .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
                if let Some(ref old) = old_tag {
                    let inner = old
                        .strip_prefix("@t[")
                        .and_then(|s| s.strip_suffix("]"))
                        .unwrap_or("");
                    if inner.ends_with("..") {
                        ChangeInstruction::Dismiss
                    } else {
                        ChangeInstruction::UpdateTemporal {
                            line_text,
                            old_tag: old.clone(),
                            new_tag: format!("@t[~{date_str}]"),
                        }
                    }
                } else {
                    ChangeInstruction::AddTemporal {
                        line_text,
                        tag: format!("@t[~{date_str}]"),
                    }
                }
            }
            AnswerType::Correction { .. } => {
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
                ChangeInstruction::Dismiss
            }
        };
    }

    // Ambiguous questions ("which X is this?") are informational — the answer
    // clarifies meaning but should never modify document body text.  Injecting
    // answer prose into content causes corruption (e.g. "Antiochus III" becoming
    // "Antiochus III — the 3rd ruler of that name").
    // Exception: explicit "split:" corrections are allowed since they restructure
    // the document rather than injecting answer text.
    if question.question_type == crate::models::QuestionType::Ambiguous {
        // Allow "split:" corrections through — they restructure, not inject
        if let AnswerType::Correction { ref detail } = answer_type {
            if detail.to_lowercase().starts_with("split:") {
                return ChangeInstruction::Split {
                    line_text,
                    instruction: detail["split:".len()..].trim().to_string(),
                };
            }
        }
        return match answer_type {
            AnswerType::Dismissal | AnswerType::Confirmation | AnswerType::SourceCitation { .. } | AnswerType::Correction { .. } => {
                ChangeInstruction::Dismiss
            }
            AnswerType::Deferral => ChangeInstruction::Defer,
            AnswerType::Deletion => ChangeInstruction::Delete { line_text },
        };
    }

    match answer_type {
        AnswerType::Dismissal => ChangeInstruction::Dismiss,
        AnswerType::Deferral => ChangeInstruction::Defer,
        AnswerType::Deletion => ChangeInstruction::Delete { line_text },
        // Source citations from review answers must NOT become footnote
        // definitions — they cause garbage footnote accumulation.  Answers
        // belong in the review queue section, not as source citations.
        AnswerType::SourceCitation { .. } => ChangeInstruction::Dismiss,
        AnswerType::Confirmation => {
            // Use verification date (month precision+) from the answer if provided
            // (e.g. "Still current 2024-02"), otherwise fall back to today's date.
            // Bare years are excluded to avoid extracting publication years from
            // citations like "Beard (1998)".
            let date_str = extract_verification_date(answer)
                .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
            if let Some(ref old) = old_tag {
                // Preserve open-ended ranges: confirming @t[2023-08..] means
                // the range is still valid and ongoing — don't replace with ~date
                let inner = old
                    .strip_prefix("@t[")
                    .and_then(|s| s.strip_suffix("]"))
                    .unwrap_or("");
                if inner.ends_with("..") {
                    ChangeInstruction::Dismiss
                } else {
                    let new_tag = format!("@t[~{date_str}]");
                    ChangeInstruction::UpdateTemporal {
                        line_text,
                        old_tag: old.clone(),
                        new_tag,
                    }
                }
            } else {
                ChangeInstruction::AddTemporal {
                    line_text,
                    tag: format!("@t[~{date_str}]"),
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
    fn test_classify_sequential_dismissal() {
        assert_eq!(
            classify_answer("Sequential roles with boundary overlap"),
            AnswerType::Dismissal
        );
        assert_eq!(
            classify_answer("Roles are sequential, not a conflict"),
            AnswerType::Dismissal
        );
        assert_eq!(
            classify_answer("boundary month overlap - sequential career transition"),
            AnswerType::Dismissal
        );
        assert_eq!(
            classify_answer("boundary overlap, promotion"),
            AnswerType::Dismissal
        );
        assert_eq!(
            classify_answer("These are sequential roles, not a conflict"),
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
        // Source citations from review answers are dismissed — they should not
        // become footnote definitions (prevents garbage footnote accumulation).
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Expected Dismiss, got {result:?}"
        );
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
        // Source citations from review answers are dismissed — they should not
        // become footnote definitions.
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Expected Dismiss, got {result:?}"
        );
    }

    #[test]
    fn test_interpret_answer_temporal_source_citation_becomes_dismiss() {
        // Source citations should never create footnotes, regardless of question type
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""VP at BigCo" - when?"#.to_string(),
            answered: true,
            answer: Some("per LinkedIn 2024-06".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "per LinkedIn 2024-06");
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Expected Dismiss for source citation on temporal question, got {result:?}"
        );
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

    #[test]
    fn test_classify_confirmation_with_date_not_source() {
        // "Still current 2024-02" should NOT be classified as SourceCitation
        let result = classify_answer("Still current 2024-02");
        assert!(
            !matches!(result, AnswerType::SourceCitation { .. }),
            "Expected non-SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_classify_confirmed_with_date_not_source() {
        let result = classify_answer("Confirmed 2024-02");
        assert!(
            !matches!(result, AnswerType::SourceCitation { .. }),
            "Expected non-SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_classify_verified_with_date_not_source() {
        let result = classify_answer("Verified 2024-02-15");
        assert!(
            !matches!(result, AnswerType::SourceCitation { .. }),
            "Expected non-SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_classify_yes_verified_with_date_not_source() {
        let result = classify_answer("Yes, verified 2024-02");
        assert!(
            !matches!(result, AnswerType::SourceCitation { .. }),
            "Expected non-SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_classify_real_source_with_date_still_works() {
        // Genuine source citations should still be classified correctly
        let result = classify_answer("Phonetool lookup, 2024-02-10");
        assert!(
            matches!(result, AnswerType::SourceCitation { .. }),
            "Expected SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_classify_source_prefix_with_date_still_works() {
        let result = classify_answer("per LinkedIn 2024-02");
        assert!(
            matches!(result, AnswerType::SourceCitation { .. }),
            "Expected SourceCitation, got {result:?}"
        );
    }

    #[test]
    fn test_interpret_stale_confirmation_with_date_updates_temporal() {
        // Stale question answered with "Still current 2024-02" should update
        // the temporal tag, NOT inject a garbage footnote.
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Cost optimization @t[~2024-02]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("Still current 2024-02".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "Still current 2024-02");
        match result {
            ChangeInstruction::UpdateTemporal {
                old_tag, new_tag, ..
            } => {
                assert_eq!(old_tag, "@t[~2024-02]");
                assert!(new_tag.contains("2024-02"), "new_tag should contain the date from the answer: {new_tag}");
            }
            ChangeInstruction::AddSource { source_info, .. } => {
                panic!("Got AddSource (garbage footnote bug!): {source_info}");
            }
            other => panic!("Expected UpdateTemporal, got {other:?}"),
        }
    }

    #[test]
    fn test_date_replace_does_not_mangle_temporal_tags() {
        // If answer text contains @t tags, the date extraction should not
        // mangle them by stripping the year from inside the tag.
        let result = classify_answer("per LinkedIn @t[~2024-02] profile, 2024-02");
        if let AnswerType::SourceCitation { source, .. } = result {
            assert!(
                !source.contains("@t[~-02]"),
                "Date replace mangled @t tag: {source}"
            );
        }
    }

    // --- conflict-specific answer interpretation tests ---

    #[test]
    fn test_conflict_answer_source_citation_becomes_dismiss() {
        // Source citations are meaningless for conflict questions
        let q = ReviewQuestion {
            question_type: crate::models::QuestionType::Conflict,
            line_ref: Some(5),
            description: r#""VP at Acme" @t[2020..2023] overlaps with "Director at Acme" @t[2018..2020] - were both true simultaneously? (line:7)"#.to_string(),
            answered: true,
            answer: Some("per LinkedIn 2024-01".to_string()),
            line_number: 10,
        };
        assert!(matches!(interpret_answer(&q, "per LinkedIn 2024-01"), ChangeInstruction::Dismiss));
    }

    #[test]
    fn test_conflict_answer_generic_correction_becomes_dismiss() {
        // Generic corrections without dates should dismiss for conflicts
        let q = ReviewQuestion {
            question_type: crate::models::QuestionType::Conflict,
            line_ref: Some(5),
            description: "overlap question".to_string(),
            answered: true,
            answer: Some("these are sequential promotions at the same company".to_string()),
            line_number: 10,
        };
        assert!(matches!(interpret_answer(&q, "these are sequential promotions at the same company"), ChangeInstruction::Dismiss));
    }

    #[test]
    fn test_conflict_answer_with_date_correction_applies() {
        // Date corrections should still work for conflicts
        let q = ReviewQuestion {
            question_type: crate::models::QuestionType::Conflict,
            line_ref: Some(5),
            description: r#""VP at Acme" @t[2020..2023] overlaps"#.to_string(),
            answered: true,
            answer: Some("correct: ended 2021-06".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "correct: ended 2021-06");
        assert!(!matches!(result, ChangeInstruction::Dismiss), "Date correction should not be dismissed");
    }

    #[test]
    fn test_confirmation_preserves_open_ended_range() {
        // Confirming "still current" on @t[2023-08..] should NOT replace with @t[~date]
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""Engineer at Acme @t[2023-08..]" - still current?"#.to_string(),
            answered: true,
            answer: Some("still current per LinkedIn scraped 2026-02-10".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "still current per LinkedIn scraped 2026-02-10");
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Open-ended range should be preserved (dismissed), got {result:?}"
        );
    }

    #[test]
    fn test_confirmation_preserves_open_ended_range_simple_yes() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""VP at BigCo @t[2022..]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("yes".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "yes");
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Open-ended range should be preserved (dismissed), got {result:?}"
        );
    }

    #[test]
    fn test_confirmation_updates_last_verified_tag() {
        // Non-range tags like @t[~2024-01] should still get updated
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
            ChangeInstruction::UpdateTemporal { old_tag, new_tag, .. } => {
                assert_eq!(old_tag, "@t[~2024-01]");
                assert!(new_tag.starts_with("@t[~"), "Should update ~date tag: {new_tag}");
            }
            _ => panic!("Expected UpdateTemporal for ~date tag, got {result:?}"),
        }
    }

    #[test]
    fn test_conflict_answer_confirmation_becomes_dismiss() {
        // "yes" to a conflict question means "yes both were true" = dismiss
        let q = ReviewQuestion {
            question_type: crate::models::QuestionType::Conflict,
            line_ref: Some(5),
            description: "overlap question".to_string(),
            answered: true,
            answer: Some("yes".to_string()),
            line_number: 10,
        };
        assert!(matches!(interpret_answer(&q, "yes"), ChangeInstruction::Dismiss));
    }

    // --- Missing-source question tests ---

    #[test]
    fn test_missing_unable_to_verify_becomes_dismiss() {
        // "Unable to verify" is not a source — must dismiss, not corrupt content
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Email: user@example.com @t[~2025-10]" has no source"#.to_string(),
            answered: true,
            answer: Some("Unable to verify from available sources".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "Unable to verify from available sources"),
            ChangeInstruction::Dismiss
        ));
    }

    #[test]
    fn test_missing_source_citation_becomes_dismiss() {
        // Source citations from review answers are dismissed — they should not
        // become footnote definitions (prevents garbage footnote accumulation).
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Works at Acme" has no source"#.to_string(),
            answered: true,
            answer: Some("per LinkedIn 2024-06".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "per LinkedIn 2024-06"),
            ChangeInstruction::Dismiss
        ));
    }

    #[test]
    fn test_missing_correction_fallback_becomes_dismiss() {
        // Long freeform answers that fall through to Correction must not become Generic
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Primary contact for project" has no source"#.to_string(),
            answered: true,
            answer: Some("Corroborated by account document which lists them as contact".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "Corroborated by account document which lists them as contact"),
            ChangeInstruction::Dismiss
        ));
    }

    #[test]
    fn test_missing_defer_works() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Fact here" has no source"#.to_string(),
            answered: true,
            answer: Some("defer".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "defer"),
            ChangeInstruction::Defer
        ));
    }

    #[test]
    fn test_missing_delete_works() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(5),
            description: r#""Fact here" has no source"#.to_string(),
            answered: true,
            answer: Some("delete".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "delete"),
            ChangeInstruction::Delete { .. }
        ));
    }

    // --- Stale question tests ---

    #[test]
    fn test_stale_corroboration_with_date_becomes_confirmation_not_source() {
        // Long answer with date that corroborates a fact should update temporal tag,
        // NOT inject a garbage footnote
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Technical contact @t[~2025-06]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("Corroborated by account document (0db425) which lists them as technical implementation contact @t[~] sourced from 2026-01-15 transition meeting. Role is current.".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "Corroborated by account document (0db425) which lists them as technical implementation contact @t[~] sourced from 2026-01-15 transition meeting. Role is current.");
        match result {
            ChangeInstruction::UpdateTemporal { old_tag, new_tag, .. } => {
                assert_eq!(old_tag, "@t[~2025-06]");
                assert!(new_tag.starts_with("@t[~2026"), "Should use date from answer: {new_tag}");
            }
            _ => panic!("Expected UpdateTemporal, got {result:?}"),
        }
    }

    #[test]
    fn test_stale_source_citation_becomes_temporal_update() {
        // "per LinkedIn 2024-06" on a stale question is confirmation, not a new source
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""VP at BigCo @t[~2024-01]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("per LinkedIn 2024-06".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "per LinkedIn 2024-06");
        match result {
            ChangeInstruction::UpdateTemporal { old_tag, new_tag, .. } => {
                assert_eq!(old_tag, "@t[~2024-01]");
                assert!(new_tag.contains("2024-06"), "Should use date from answer: {new_tag}");
            }
            _ => panic!("Expected UpdateTemporal for stale confirmation, got {result:?}"),
        }
    }

    #[test]
    fn test_stale_correction_without_dates_becomes_dismiss() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Works at Acme @t[~2024-01]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("role appears unchanged based on available info".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "role appears unchanged based on available info"),
            ChangeInstruction::Dismiss
        ));
    }

    // --- Bug fix: publication years must not become temporal tags ---

    #[test]
    fn test_stale_confirmation_ignores_publication_year() {
        // "Beard (1998) findings are still current" — 1998 is a publication year,
        // not a verification date.  Should use today's date, not @t[~1998].
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Sacred fire as symbol of truth" - still valid?"#.to_string(),
            answered: true,
            answer: Some("Beard (1998) findings are still current".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "Beard (1998) findings are still current");
        match result {
            ChangeInstruction::AddTemporal { tag, .. } => {
                assert!(!tag.contains("1998"), "Should not use publication year: {tag}");
                // Should use today's date
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                assert!(tag.contains(&today), "Should use today's date: {tag}");
            }
            _ => panic!("Expected AddTemporal, got {result:?}"),
        }
    }

    #[test]
    fn test_stale_confirmation_with_precise_date_uses_it() {
        // "Still current per 2024-06 review" — 2024-06 has month precision, use it.
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: r#""Works at Acme @t[~2024-01]" - still valid?"#.to_string(),
            answered: true,
            answer: Some("Still current per 2024-06 review".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "Still current per 2024-06 review");
        match result {
            ChangeInstruction::UpdateTemporal { new_tag, .. } => {
                assert!(new_tag.contains("2024-06"), "Should use precise date: {new_tag}");
            }
            _ => panic!("Expected UpdateTemporal, got {result:?}"),
        }
    }

    #[test]
    fn test_confirmation_ignores_bare_year_in_citation() {
        // Default confirmation handler should also ignore bare years
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: r#""Founded in antiquity" - when?"#.to_string(),
            answered: true,
            answer: Some("yes".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "yes");
        match result {
            ChangeInstruction::AddTemporal { tag, .. } => {
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                assert!(tag.contains(&today), "Should use today's date: {tag}");
            }
            _ => panic!("Expected AddTemporal, got {result:?}"),
        }
    }

    // --- Bug fix: ambiguous answers must not inject text into body ---

    #[test]
    fn test_ambiguous_answer_dismissed_not_injected() {
        // "the 3rd ruler of that name" should NOT be injected into document body
        let q = ReviewQuestion {
            question_type: QuestionType::Ambiguous,
            line_ref: Some(5),
            description: r#""Antiochus III" - which one?"#.to_string(),
            answered: true,
            answer: Some("the 3rd ruler of that name. Standard historical convention.".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(
            &q,
            "the 3rd ruler of that name. Standard historical convention.",
        );
        assert!(
            matches!(result, ChangeInstruction::Dismiss),
            "Ambiguous answer should be dismissed, not injected: {result:?}"
        );
    }

    #[test]
    fn test_ambiguous_answer_defer_works() {
        let q = ReviewQuestion {
            question_type: QuestionType::Ambiguous,
            line_ref: Some(5),
            description: r#""Antiochus" - which one?"#.to_string(),
            answered: true,
            answer: Some("defer".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "defer"),
            ChangeInstruction::Defer
        ));
    }

    #[test]
    fn test_ambiguous_answer_delete_works() {
        let q = ReviewQuestion {
            question_type: QuestionType::Ambiguous,
            line_ref: Some(5),
            description: r#""Antiochus" - which one?"#.to_string(),
            answered: true,
            answer: Some("delete".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "delete"),
            ChangeInstruction::Delete { .. }
        ));
    }

    #[test]
    fn test_ambiguous_split_still_works() {
        // "split:" prefix should still work for ambiguous questions
        let q = ReviewQuestion {
            question_type: QuestionType::Ambiguous,
            line_ref: Some(5),
            description: r#""Engineer then Lead" - clarify"#.to_string(),
            answered: true,
            answer: Some("split: separate roles".to_string()),
            line_number: 10,
        };
        assert!(matches!(
            interpret_answer(&q, "split: separate roles"),
            ChangeInstruction::Split { .. }
        ));
    }

    #[test]
    fn test_interpret_answer_defer_with_colon() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: "test".to_string(),
            answered: true,
            answer: Some("defer: recursive question about review queue".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "defer: recursive question about review queue");
        assert!(matches!(result, ChangeInstruction::Defer));
    }

    #[test]
    fn test_interpret_answer_deferred_prefix() {
        let q = ReviewQuestion {
            question_type: QuestionType::Stale,
            line_ref: Some(5),
            description: "test".to_string(),
            answered: true,
            answer: Some("deferred: needs more research".to_string()),
            line_number: 10,
        };
        let result = interpret_answer(&q, "deferred: needs more research");
        assert!(matches!(result, ChangeInstruction::Defer));
    }

    #[test]
    fn test_classify_answer_defer_colon_variants() {
        assert!(matches!(classify_answer("defer: reason"), AnswerType::Deferral));
        assert!(matches!(classify_answer("Defer: reason"), AnswerType::Deferral));
        assert!(matches!(classify_answer("DEFER: reason"), AnswerType::Deferral));
        assert!(matches!(classify_answer("defer:reason"), AnswerType::Deferral));
        assert!(matches!(classify_answer("deferred: later"), AnswerType::Deferral));
        assert!(matches!(classify_answer("deferred later"), AnswerType::Deferral));
    }
}
