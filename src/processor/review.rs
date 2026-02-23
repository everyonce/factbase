//! Review question parsing.
//!
//! This module handles parsing `@q[...]` review questions from document content
//! and appending new questions to documents.

use std::collections::HashSet;

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{INLINE_QUESTION_MARKER, REVIEW_QUESTION_REGEX, REVIEW_QUEUE_MARKER};
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
    let desc = desc
        .rfind(" [pattern:")
        .map_or(desc, |idx| &desc[..idx]);
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
fn extract_line_ref_and_strip(description: &str) -> (Option<usize>, String) {
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

/// Remove unanswered questions whose trigger conditions no longer exist.
///
/// Takes the set of descriptions that the generators would produce today.
/// Any unanswered question whose description is NOT in `valid_descriptions`
/// is removed. Answered and deferred questions are always kept.
///
/// If `had_deep_check` is false, questions starting with "Cross-check" are
/// preserved regardless (they require LLM to regenerate).
pub fn prune_stale_questions(
    content: &str,
    valid_descriptions: &HashSet<String>,
    had_deep_check: bool,
) -> String {
    let Some(marker_pos) = content.find(REVIEW_QUEUE_MARKER) else {
        return content.to_string();
    };

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[REVIEW_QUEUE_MARKER.len()..];

    let mut result_lines: Vec<&str> = Vec::new();
    let mut skip_answer = false;
    let mut pruned = 0usize;

    for line in queue_content.lines() {
        let trimmed = line.trim_start();
        let is_question = trimmed.starts_with("- [");

        if is_question {
            let is_answered = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");
            let is_cross_check = trimmed.contains("Cross-check");

            // Keep answered questions, cross-check questions (when no LLM), and valid questions
            if is_answered
                || (!had_deep_check && is_cross_check)
                || question_description_matches(trimmed, valid_descriptions)
            {
                result_lines.push(line);
                skip_answer = false;
            } else {
                skip_answer = true;
                pruned += 1;
            }
        } else if skip_answer && trimmed.starts_with('>') {
            continue;
        } else {
            result_lines.push(line);
            skip_answer = false;
        }
    }

    if pruned == 0 {
        return content.to_string();
    }

    // Check if any questions remain
    let has_questions = result_lines
        .iter()
        .any(|l| l.trim_start().starts_with("- ["));

    if has_questions {
        format!(
            "{}{}\n{}",
            before_marker,
            REVIEW_QUEUE_MARKER,
            result_lines.join("\n")
        )
    } else {
        // Remove entire Review Queue section including heading and separator
        let mut body = before_marker.to_string();
        loop {
            let trimmed = body.trim_end();
            if trimmed.ends_with("## Review Queue") {
                body = trimmed.trim_end_matches("## Review Queue").to_string();
            } else if trimmed.ends_with("---") {
                body = trimmed.trim_end_matches("---").to_string();
            } else {
                body = trimmed.to_string();
                break;
            }
        }
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body
    }
}

/// Check if a question line's description matches any in the valid set.
fn question_description_matches(line: &str, valid: &HashSet<String>) -> bool {
    // Question format: "- [ ] `@q[type]` Description text"
    // Extract description after the @q[...] tag
    if let Some(pos) = line.find("]` ") {
        let raw_desc = &line[pos + 3..];
        // Strip "Line N: " prefix to match generator descriptions
        let (_, stripped) = extract_line_ref_and_strip(raw_desc);
        if valid.contains(&stripped) {
            return true;
        }
        // For conflict questions whose line numbers may have shifted,
        // also try matching with the trailing (line:N) stripped.
        let normalized = normalize_conflict_desc(&stripped);
        if normalized != stripped {
            return valid.iter().any(|v| normalize_conflict_desc(v) == normalized);
        }
        return false;
    }
    // If we can't parse it, keep it (conservative)
    true
}

/// Normalize the review queue section to prevent format degradation.
///
/// This function:
/// (a) Merges duplicate `## Review Queue` headers into one
/// (b) Removes orphaned `@q[...]` markers outside the review queue section
/// (c) Strips empty blockquote lines (`>` with only whitespace) not part of an answer
/// (d) Removes the entire section if no questions remain
pub fn normalize_review_section(content: &str) -> String {
    let Some(marker_pos) = content.find(REVIEW_QUEUE_MARKER) else {
        // No review section — just strip orphaned @q markers from body
        return strip_orphaned_markers(content);
    };

    let before_marker = &content[..marker_pos];
    let after_marker = &content[marker_pos + REVIEW_QUEUE_MARKER.len()..];

    // (b) Strip orphaned @q markers from body (before the review section)
    // Find where the review section heading starts (## Review Queue before marker)
    let section_start = find_review_section_start(before_marker);
    let (body, section_header) = before_marker.split_at(section_start);
    let clean_body = strip_orphaned_markers(body);

    // (a) Remove duplicate ## Review Queue headers from section_header
    // Keep only the last one (closest to marker)
    let clean_header = dedup_review_headers(section_header);

    // (c) Strip empty blockquote lines not part of an answer in the queue content
    let clean_queue = strip_orphaned_blockquotes(after_marker);

    // (d) Check if any questions remain
    let has_questions = clean_queue
        .lines()
        .any(|l| l.trim_start().starts_with("- ["));

    if !has_questions {
        return clean_body.trim_end().to_string() + "\n";
    }

    format!(
        "{}{}{}{}",
        clean_body, clean_header, REVIEW_QUEUE_MARKER, clean_queue
    )
}

/// Find the start position of the review section heading (## Review Queue + surrounding whitespace).
fn find_review_section_start(before_marker: &str) -> usize {
    // Look backwards for `## Review Queue` and any preceding separator (---)
    let lines: Vec<&str> = before_marker.lines().collect();
    let mut section_start_line = lines.len();

    for (i, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim();
        if trimmed == "## Review Queue" {
            section_start_line = i;
            // Continue backwards past blank lines, separators, and duplicate headers
            let mut j = i;
            while j > 0 {
                let prev = lines[j - 1].trim();
                if prev.is_empty() || prev == "---" || prev == "## Review Queue" {
                    j -= 1;
                    section_start_line = j;
                } else {
                    break;
                }
            }
            break;
        }
    }

    if section_start_line >= lines.len() {
        return before_marker.len();
    }

    // Convert line index to byte offset
    let mut offset = 0;
    for line in lines.iter().take(section_start_line) {
        offset += line.len() + 1; // +1 for newline
    }
    offset
}

/// Remove orphaned `@q[...]` markers from content (outside review section).
fn strip_orphaned_markers(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            // Only strip from non-question lines (question lines start with `- [`)
            if line.trim_start().starts_with("- [") && line.contains("`@q[") {
                line.to_string()
            } else {
                INLINE_QUESTION_MARKER.replace_all(line, "").to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove duplicate `## Review Queue` headers, keeping only one.
fn dedup_review_headers(section_header: &str) -> String {
    let mut seen_header = false;
    let mut lines: Vec<&str> = Vec::new();

    for line in section_header.lines() {
        if line.trim() == "## Review Queue" {
            if !seen_header {
                seen_header = true;
                lines.push(line);
            }
            // Skip duplicates
        } else {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

/// Strip empty blockquote lines that aren't part of an answer.
/// Keeps blockquote lines that follow a question line (answer placeholders).
fn strip_orphaned_blockquotes(queue_content: &str) -> String {
    let lines: Vec<&str> = queue_content.lines().collect();
    let mut result: Vec<&str> = Vec::new();
    let mut prev_is_question = false;

    for line in &lines {
        let trimmed = line.trim();
        let is_question = trimmed.starts_with("- [");
        let is_empty_blockquote = trimmed
            .strip_prefix('>')
            .is_some_and(|rest| rest.trim().is_empty());

        if is_empty_blockquote && !prev_is_question {
            // Orphaned empty blockquote — skip
            continue;
        }

        result.push(line);
        prev_is_question = is_question;
    }

    result.join("\n")
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
                .map(|n| format!("Line {n}: "))
                .unwrap_or_default();
            let type_tag = match q.question_type {
                QuestionType::Temporal => "temporal",
                QuestionType::Conflict => "conflict",
                QuestionType::Missing => "missing",
                QuestionType::Ambiguous => "ambiguous",
                QuestionType::Stale => "stale",
                QuestionType::Duplicate => "duplicate",
            };
            {
                write_str!(
                    questions_text,
                    "\n- [ ] `@q[{}]` {}{}\n  > \n",
                    type_tag,
                    line_ref,
                    q.description
                );
            }
        }

        result.insert_str(insert_pos, &questions_text);
    }

    normalize_review_section(&result)
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
        assert_eq!(result[0].line_ref, Some(5));
        // Description has "Line N:" prefix stripped to match generator output
        assert_eq!(
            result[0].description,
            "\"Started at Acme Corp\" - when did this role begin?"
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

    // ============================================================================
    // normalize_review_section Tests
    // ============================================================================

    #[test]
    fn test_normalize_merges_duplicate_headers() {
        let content = "# Doc\n\nContent\n\n---\n\n## Review Queue\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` question\n  > \n";
        let result = normalize_review_section(content);
        assert_eq!(result.matches("## Review Queue").count(), 1);
        assert!(result.contains("- [ ] `@q[temporal]` question"));
    }

    #[test]
    fn test_normalize_removes_orphaned_q_markers() {
        let content = "# Doc\n\n- Fact here `@q[stale]`\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` question\n  > \n";
        let result = normalize_review_section(content);
        // Orphaned marker removed from body
        assert!(result.contains("- Fact here"));
        assert!(!result.contains("- Fact here `@q[stale]`"));
        // Question in review section preserved
        assert!(result.contains("`@q[temporal]`"));
    }

    #[test]
    fn test_normalize_strips_orphaned_blockquotes() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` question\n  > \n> \n> \n- [ ] `@q[missing]` another\n  > \n";
        let result = normalize_review_section(content);
        // The empty blockquotes between questions (not after a question) are stripped
        // But answer placeholders after questions are kept
        assert!(result.contains("- [ ] `@q[temporal]` question"));
        assert!(result.contains("- [ ] `@q[missing]` another"));
    }

    #[test]
    fn test_normalize_removes_section_when_no_questions() {
        let content =
            "# Doc\n\nContent here\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\n";
        let result = normalize_review_section(content);
        assert!(!result.contains("Review Queue"));
        assert!(!result.contains("factbase:review"));
        assert!(result.contains("Content here"));
    }

    #[test]
    fn test_normalize_no_review_section_strips_orphans() {
        let content = "# Doc\n\n- Fact `@q[stale]` here\n";
        let result = normalize_review_section(content);
        assert!(result.contains("- Fact here"));
        assert!(!result.contains("@q[stale]"));
    }

    #[test]
    fn test_normalize_preserves_valid_section() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 5: when?\n  > \n";
        let result = normalize_review_section(content);
        assert!(result.contains("## Review Queue"));
        assert!(result.contains("<!-- factbase:review -->"));
        assert!(result.contains("- [ ] `@q[temporal]` Line 5: when?"));
    }

    // ============================================================================
    // Prune Stale Questions Tests
    // ============================================================================

    #[test]
    fn test_prune_removes_stale_unanswered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` \"Old fact\" - when was this true?\n  > \n";
        let valid = HashSet::new(); // no valid questions — the trigger is gone
        let result = prune_stale_questions(content, &valid, false);
        assert!(!result.contains("Old fact"), "Stale question should be removed");
    }

    #[test]
    fn test_prune_keeps_valid_unanswered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` \"Current fact\" - when was this true?\n  > \n";
        let mut valid = HashSet::new();
        valid.insert("\"Current fact\" - when was this true?".to_string());
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("Current fact"), "Valid question should be kept");
    }

    #[test]
    fn test_prune_keeps_answered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [x] `@q[temporal]` \"Old fact\" - when was this true?\n  > 2024\n";
        let valid = HashSet::new(); // not in valid set, but answered
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("Old fact"), "Answered question should be kept");
    }

    #[test]
    fn test_prune_keeps_cross_check_without_llm() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[stale]` Cross-check with doc2: fact is outdated\n  > \n";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("Cross-check"), "Cross-check kept when no LLM");
    }

    #[test]
    fn test_prune_removes_cross_check_with_llm() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[stale]` Cross-check with doc2: fact is outdated\n  > \n";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, true);
        assert!(!result.contains("Cross-check"), "Cross-check pruned when LLM ran");
    }

    #[test]
    fn test_prune_removes_entire_section_when_empty() {
        let content = "# Doc\n\nContent here.\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` stale question\n  > \n";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, false);
        assert!(!result.contains("Review Queue"), "Empty section should be removed");
        assert!(result.contains("Content here"), "Body should be preserved");
    }

    #[test]
    fn test_prune_no_review_section() {
        let content = "# Doc\n\nJust content.";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, false);
        assert_eq!(result, content);
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
        // Should only strip the trailing (line:N), not other parenthesized content
        let desc = "some (note) text (line:10)";
        assert_eq!(normalize_conflict_desc(desc), "some (note) text");
    }

    #[test]
    fn test_normalize_conflict_desc_strips_pattern_tag() {
        assert_eq!(
            normalize_conflict_desc(
                "\"Role A\" overlaps with \"Role B\" (line:5) [pattern:concurrent_roles]"
            ),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    #[test]
    fn test_normalize_conflict_desc_pattern_tag_without_line_ref() {
        assert_eq!(
            normalize_conflict_desc("\"Role A\" overlaps with \"Role B\" [pattern:promotion]"),
            "\"Role A\" overlaps with \"Role B\""
        );
    }
}
