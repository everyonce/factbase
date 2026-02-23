//! Review question parsing.
//!
//! This module handles parsing `@q[...]` review questions from document content
//! and appending new questions to documents.

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{REVIEW_QUESTION_REGEX, REVIEW_QUEUE_MARKER};
use tracing::warn;

/// Parse the Review Queue section from document content.
/// Returns None if no Review Queue marker exists.
/// Returns Some(empty vec) if marker exists but no questions.
pub fn parse_review_queue(content: &str) -> Option<Vec<ReviewQuestion>> {
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
            let description = cap[3].to_string();

            let checkbox_checked = checkbox == "x" || checkbox == "X";
            let question_type = type_str.parse::<QuestionType>().unwrap_or_else(|_| {
                warn!("Unknown question type: {}, defaulting to Missing", type_str);
                QuestionType::Missing
            });

            // Extract line_ref from description if present (e.g., "Line 5: ...")
            let line_ref = extract_line_ref(&description);

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
            });

            i = j;
        } else {
            i += 1;
        }
    }

    Some(questions)
}

/// Extract line reference from question description.
/// Looks for patterns like "Line 5:" or "Lines 5-10:" at the start.
fn extract_line_ref(description: &str) -> Option<usize> {
    // Pattern: "Line N:" at start of description
    if let Some(rest) = description.strip_prefix("Line ") {
        if let Some(colon_pos) = rest.find(':') {
            if let Ok(line_num) = rest[..colon_pos].trim().parse::<usize>() {
                return Some(line_num);
            }
        }
    }
    // Pattern: "Lines N-M:" - return first line
    if let Some(rest) = description.strip_prefix("Lines ") {
        if let Some(dash_pos) = rest.find('-') {
            if let Ok(line_num) = rest[..dash_pos].trim().parse::<usize>() {
                return Some(line_num);
            }
        }
    }
    None
}

/// Append review questions to a document's Review Queue section.
/// Creates the section if it doesn't exist.
pub fn append_review_questions(content: &str, questions: &[ReviewQuestion]) -> String {
    let mut result = content.to_string();

    // Check if Review Queue section exists
    if !result.contains(REVIEW_QUEUE_MARKER) {
        // Add Review Queue section at the end
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str("\n---\n\n## Review Queue\n\n");
        result.push_str(REVIEW_QUEUE_MARKER);
        result.push('\n');
    }

    // Find the marker position and append questions after it
    if let Some(marker_pos) = result.find(REVIEW_QUEUE_MARKER) {
        let insert_pos = marker_pos + REVIEW_QUEUE_MARKER.len();
        let mut questions_text = String::new();

        for q in questions {
            let line_ref = q
                .line_ref
                .map(|n| format!("Line {}: ", n))
                .unwrap_or_default();
            let type_tag = match q.question_type {
                QuestionType::Temporal => "temporal",
                QuestionType::Conflict => "conflict",
                QuestionType::Missing => "missing",
                QuestionType::Ambiguous => "ambiguous",
                QuestionType::Stale => "stale",
                QuestionType::Duplicate => "duplicate",
            };
            questions_text.push_str(&format!(
                "\n- [ ] `@q[{}]` {}{}\n  > \n",
                type_tag, line_ref, q.description
            ));
        }

        result.insert_str(insert_pos, &questions_text);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Review Queue Parsing Tests
    // ============================================================================

    #[test]
    fn test_review_queue_no_marker() {
        // Document without Review Queue marker returns None
        let content = "# Document\n\nSome content here.\n";
        assert!(parse_review_queue(content).is_none());
    }

    #[test]
    fn test_review_queue_empty() {
        // Marker exists but no questions returns Some(empty vec)
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
        assert_eq!(result[2].line_ref, Some(3)); // First line of range
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
        // Unknown types default to Missing
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
        // Edge case: checkbox checked but no blockquote answer
        // Should be treated as unanswered
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: when did this start?
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].answered); // Not answered because no blockquote
        assert!(result[0].answer.is_none());
    }

    #[test]
    fn test_review_queue_checked_empty_blockquote() {
        // Edge case: checkbox checked but empty blockquote
        // Should be treated as unanswered
        let content = r#"# Document

<!-- factbase:review -->
- [x] `@q[temporal]` Line 5: when did this start?
  >
"#;
        let result = parse_review_queue(content).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].answered); // Empty answer treated as unanswered
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
        // Multi-line answers joined with space
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
        // Line numbers are 1-indexed, counting from start of document
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
        assert_eq!(result[1].line_ref, Some(10)); // First line of range
        assert_eq!(result[2].line_ref, None);
    }

    #[test]
    fn test_review_queue_malformed_checkbox() {
        // Lines that don't match the expected format are skipped
        let content = r#"# Document

<!-- factbase:review -->
- [] `@q[temporal]` missing space in checkbox
- [xx] `@q[temporal]` invalid checkbox
- [ ] `@q[temporal]` valid question
"#;
        let result = parse_review_queue(content).unwrap();
        // Only the valid question is parsed
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].description, "valid question");
    }

    #[test]
    fn test_review_queue_malformed_type() {
        // Missing backticks or @q[] format
        let content = r#"# Document

<!-- factbase:review -->
- [ ] @q[temporal] missing backticks
- [ ] `temporal` missing @q[]
- [ ] `@q[temporal]` valid question
"#;
        let result = parse_review_queue(content).unwrap();
        // Only the valid question is parsed
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
        assert_eq!(
            result[0].description,
            "Line 5: \"Started at Acme Corp\" - when did this role begin?"
        );
    }

    #[test]
    fn test_review_queue_with_content_before() {
        // Review Queue after document content and footnotes
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
}
