//! Review question parsing.
//!
//! Parses `@q[...]` review questions from document content, extracts line
//! references, and normalizes conflict descriptions.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{REVIEW_QUESTION_REGEX, REVIEW_QUEUE_MARKER};
use tracing::warn;

use super::callout::unwrap_review_callout;

/// Parse the Review Queue section from document content.
/// Returns None if no Review Queue marker exists.
/// Returns Some(empty vec) if marker exists but no questions.
pub fn parse_review_queue(content: &str) -> Option<Vec<ReviewQuestion>> {
    // Unwrap callout format if present so parsing logic works unchanged
    let (content, _) = unwrap_review_callout(content);
    // Find the review queue marker
    let marker_pos = content.find(REVIEW_QUEUE_MARKER)?;

    // Get content after the marker
    let queue_content = &content[marker_pos + REVIEW_QUEUE_MARKER.len()..];

    // Calculate line number offset (lines before marker)
    let lines_before_marker = content[..marker_pos].lines().count();

    let mut questions = Vec::new();
    let lines: Vec<&str> = queue_content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        let line_number = lines_before_marker + i + 1; // 1-indexed, +1 for marker line

        if let Some(cap) = REVIEW_QUESTION_REGEX.captures(line) {
            let checkbox = &cap[1];
            let type_str = &cap[2];
            let raw_description = cap[3].to_string();

            let checkbox_checked = checkbox == "x" || checkbox == "X";
            let question_type = type_str.parse::<QuestionType>().unwrap_or_else(|_| {
                warn!("Unknown question type: {}, defaulting to Missing", type_str);
                QuestionType::Missing
            });

            // Extract line_ref from description if present (e.g., "Line 5: ...")
            // and strip the prefix so parsed descriptions match generator descriptions
            let (line_ref, description) = extract_line_ref_and_strip(&raw_description);

            // Look for answer in following blockquote line(s)
            let mut answer = None;
            let mut j = i + 1;
            let mut answer_lines = Vec::new();

            while j < lines.len() {
                let next_line = lines[j].trim();
                if let Some(quote_content) = next_line.strip_prefix('>') {
                    answer_lines.push(quote_content.trim().to_string());
                    j += 1;
                } else if next_line.is_empty() && answer_lines.is_empty() {
                    // Skip empty lines before blockquote
                    j += 1;
                } else {
                    break;
                }
            }

            if !answer_lines.is_empty() {
                let combined = answer_lines.join(" ").trim().to_string();
                if !combined.is_empty() {
                    answer = Some(combined);
                }
            }

            // A question is only considered "answered" if BOTH:
            // 1. The checkbox is checked ([x] or [X])
            // 2. There is a non-empty answer in the blockquote
            // Edge case: checked but empty answer is treated as unanswered
            let answered = checkbox_checked && answer.is_some();

            questions.push(ReviewQuestion {
                question_type,
                line_ref,
                description,
                answered,
                answer,
                line_number,
                confidence: None,
                confidence_reason: None,
            });

            i = j;
        } else {
            i += 1;
        }
    }

    Some(questions)
}

/// Strip trailing `(line:N)` and `[pattern:...]` from a conflict question
/// description so that descriptions remain stable when line numbers shift due
/// to document edits or pattern classification changes.
pub fn normalize_conflict_desc(desc: &str) -> &str {
    // Strip trailing [pattern:...] tag first
    let desc = desc.rfind(" [pattern:").map_or(desc, |idx| &desc[..idx]);
    // Then strip (line:N)
    if let Some(idx) = desc.rfind(" (line:") {
        if desc[idx..].ends_with(')') {
            return &desc[..idx];
        }
    }
    desc
}

/// Extract line reference from question description and strip the prefix.
/// Returns `(line_ref, stripped_description)`.
/// If a "Line N:" prefix is found, it's removed from the description so that
/// parsed descriptions match what generators produce (generators store line_ref
/// separately and don't include the prefix in the description).
pub(super) fn extract_line_ref_and_strip(description: &str) -> (Option<usize>, String) {
    // Pattern: "Line N: rest" at start of description
    if let Some(rest) = description.strip_prefix("Line ") {
        if let Some(colon_pos) = rest.find(':') {
            if let Ok(line_num) = rest[..colon_pos].trim().parse::<usize>() {
                let stripped = rest[colon_pos + 1..].trim_start().to_string();
                return (Some(line_num), stripped);
            }
        }
    }
    // Pattern: "Lines N-M: rest" - return first line
    if let Some(rest) = description.strip_prefix("Lines ") {
        if let Some(dash_pos) = rest.find('-') {
            if let Ok(line_num) = rest[..dash_pos].trim().parse::<usize>() {
                if let Some(colon_pos) = rest.find(':') {
                    let stripped = rest[colon_pos + 1..].trim_start().to_string();
                    return (Some(line_num), stripped);
                }
            }
        }
    }
    (None, description.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::QuestionType;

    #[test]
    fn test_review_queue_no_marker() {
        let content = "# Document\n\nSome content here.\n";
        assert!(parse_review_queue(content).is_none());
    }

    #[test]
    fn test_review_queue_empty() {
        let content = "# Document\n\n<!-- factbase:review -->\n";
        let result = parse_review_queue(content);
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_review_queue_single_unanswered() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] `@q[temporal]` Line 5: "Started at Acme" - when did this start?
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].question_type, QuestionType::Temporal);
        assert_eq!(result[0].line_ref, Some(5));
        assert!(!result[0].answered);
        assert!(result[0].answer.is_none());
    }

    #[test]
    fn test_review_queue_single_answered() {
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: "Started at Acme" - when did this start?
  > March 2020
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].answered);
        assert_eq!(result[0].answer, Some("March 2020".to_string()));
    }

    #[test]
    fn test_review_queue_mixed_answered_unanswered() {
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: when did this start?
  > March 2020
- [ ] `@q[missing]` Line 10: what is the source?
- [x] `@q[conflict]` Lines 3-5: which is correct?
  > The second one is correct
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 3);

        assert!(result[0].answered);
        assert_eq!(result[0].question_type, QuestionType::Temporal);
        assert_eq!(result[0].answer, Some("March 2020".to_string()));

        assert!(!result[1].answered);
        assert_eq!(result[1].question_type, QuestionType::Missing);
        assert!(result[1].answer.is_none());

        assert!(result[2].answered);
        assert_eq!(result[2].question_type, QuestionType::Conflict);
        assert_eq!(result[2].line_ref, Some(3));
    }

    #[test]
    fn test_review_queue_all_question_types() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] `@q[temporal]` temporal question
- [ ] `@q[conflict]` conflict question
- [ ] `@q[missing]` missing question
- [ ] `@q[ambiguous]` ambiguous question
- [ ] `@q[stale]` stale question
- [ ] `@q[duplicate]` duplicate question
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 6);
        assert_eq!(result[0].question_type, QuestionType::Temporal);
        assert_eq!(result[1].question_type, QuestionType::Conflict);
        assert_eq!(result[2].question_type, QuestionType::Missing);
        assert_eq!(result[3].question_type, QuestionType::Ambiguous);
        assert_eq!(result[4].question_type, QuestionType::Stale);
        assert_eq!(result[5].question_type, QuestionType::Duplicate);
    }

    #[test]
    fn test_review_queue_unknown_type_defaults_to_missing() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] `@q[unknown]` some question
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].question_type, QuestionType::Missing);
    }

    #[test]
    fn test_review_queue_checkbox_case_insensitive() {
        let content = r#"# Document

<!-- factbase:review -->
- [X] `@q[temporal]` uppercase X
  > answer
- [x] `@q[temporal]` lowercase x
  > answer
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].answered);
        assert!(result[1].answered);
    }

    #[test]
    fn test_review_queue_checked_but_no_answer() {
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: when did this start?
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].answered);
        assert!(result[0].answer.is_none());
    }

    #[test]
    fn test_review_queue_checked_empty_blockquote() {
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: when did this start?
  >
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].answered);
        assert!(result[0].answer.is_none());
    }

    #[test]
    fn test_review_queue_multiline_answer() {
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[conflict]` which is correct?
  > The first fact is correct.
  > The second one was outdated.
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].answered);
        assert_eq!(
            result[0].answer,
            Some("The first fact is correct. The second one was outdated.".to_string())
        );
    }

    #[test]
    fn test_review_queue_line_numbers() {
        let content = r#"# Document
Line 2
Line 3
<!-- factbase:review -->
- [ ] `@q[temporal]` first question
- [ ] `@q[missing]` second question
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line_number, 5);
        assert_eq!(result[1].line_number, 6);
    }

    #[test]
    fn test_review_queue_line_ref_extraction() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] `@q[temporal]` Line 5: "fact text" - when?
- [ ] `@q[conflict]` Lines 10-15: which is correct?
- [ ] `@q[missing]` No line reference here
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].line_ref, Some(5));
        assert_eq!(result[1].line_ref, Some(10));
        assert_eq!(result[2].line_ref, None);
    }

    #[test]
    fn test_review_queue_malformed_checkbox() {
        let content = r#"# Document

<!-- factbase:review -->
- [] `@q[temporal]` missing space in checkbox
- [xx] `@q[temporal]` invalid checkbox
- [ ] `@q[temporal]` valid question
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].description, "valid question");
    }

    #[test]
    fn test_review_queue_malformed_type() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] @q[temporal] missing backticks
- [ ] `temporal` missing @q[]
- [ ] `@q[temporal]` valid question
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].description, "valid question");
    }

    #[test]
    fn test_review_queue_preserves_description() {
        let content = r#"# Document

<!-- factbase:review -->
- [ ] `@q[temporal]` Line 5: "Started at Acme Corp" - when did this role begin?
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line_ref, Some(5));
        assert_eq!(
            result[0].description,
            "\"Started at Acme Corp\" - when did this role begin?"
        );
    }

    #[test]
    fn test_review_queue_with_content_before() {
        let content = r#"# John Doe

- Works at Acme Corp [^1]

---
[^1]: LinkedIn profile, scraped 2024-01-15

<!-- factbase:review -->
- [ ] `@q[temporal]` Line 3: when did this role start?
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].question_type, QuestionType::Temporal);
    }

    #[test]
    fn test_normalize_conflict_desc_strips_line_ref() {
        assert_eq!(
            normalize_conflict_desc("\"Role A\" overlaps with \"Role B\" (line:5)"),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    #[test]
    fn test_normalize_conflict_desc_no_line_ref() {
        let desc = "\"Role A\" overlaps with \"Role B\"";
        assert_eq!(normalize_conflict_desc(desc), desc);
    }

    #[test]
    fn test_normalize_conflict_desc_line_ref_at_end_only() {
        let desc = "some (note) text (line:10)";
        assert_eq!(normalize_conflict_desc(desc), "some (note) text");
    }

    #[test]
    fn test_normalize_conflict_desc_strips_pattern_tag() {
        assert_eq!(
            normalize_conflict_desc(
                "\"Role A\" overlaps with \"Role B\" (line:5) [pattern:parallel_overlap]"
            ),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    #[test]
    fn test_normalize_conflict_desc_pattern_tag_without_line_ref() {
        assert_eq!(
            normalize_conflict_desc(
                "\"Role A\" overlaps with \"Role B\" [pattern:same_entity_transition]"
            ),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    #[test]
    fn test_parse_review_queue_weak_source() {
        let content = "<!-- factbase:ws001 -->\n# Test\n\n- Fact\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Line 4: Vague citation\n  > \n- [ ] `@q[weak-source]` Line 5: Missing URL\n  > \n";
        let questions = parse_review_queue(content).unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].question_type, QuestionType::WeakSource);
        assert_eq!(questions[1].question_type, QuestionType::WeakSource);
    }
}
