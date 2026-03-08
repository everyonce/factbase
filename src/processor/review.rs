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

    let lines: Vec<&str> = queue_content.lines().collect();
    let mut result_lines: Vec<&str> = Vec::new();
    let mut skip_answer = false;
    let mut pruned = 0usize;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let is_question = trimmed.starts_with("- [");

        if is_question {
            let is_answered = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");
            let is_cross_check = trimmed.contains("Cross-check");

            // Check if this unchecked question has a blockquote answer (deferred/believed)
            let has_answer = !is_answered && {
                let mut j = i + 1;
                // Skip empty lines between question and potential blockquote
                while j < lines.len() && lines[j].trim().is_empty() {
                    j += 1;
                }
                // A real answer is a blockquote with non-empty content after '>'
                j < lines.len() && {
                    let t = lines[j].trim();
                    t.starts_with('>') && t.len() > 1 && !t[1..].trim().is_empty()
                }
            };

            // Keep: answered, deferred (has blockquote answer), cross-check, or valid description
            if is_answered
                || has_answer
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
            i += 1;
            continue;
        } else {
            result_lines.push(line);
            skip_answer = false;
        }
        i += 1;
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

/// Find the byte offset of a bare `## Review Queue` heading (without a marker).
/// Returns `None` if no such heading exists or if the marker is already present.
fn find_bare_review_heading(content: &str) -> Option<usize> {
    let mut offset = 0;
    for line in content.lines() {
        if line.trim() == "## Review Queue" {
            return Some(offset);
        }
        offset += line.len() + 1; // +1 for newline
    }
    None
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

/// Merge duplicate `## Review Queue` sections into one.
///
/// If a document has multiple review queue sections (with or without markers),
/// this consolidates all questions into a single section. Returns the cleaned
/// content.
pub fn merge_duplicate_review_sections(content: &str) -> String {
    // Count occurrences of the heading
    let heading_count = content
        .lines()
        .filter(|l| l.trim() == "## Review Queue")
        .count();
    let marker_count = content.matches(REVIEW_QUEUE_MARKER).count();

    if heading_count <= 1 && marker_count <= 1 {
        return content.to_string();
    }

    // Extract all question lines from anywhere in the review sections
    let body_end = crate::patterns::body_end_offset(content);
    let review_area = &content[body_end..];
    let mut questions: Vec<String> = Vec::new();
    let mut in_question = false;

    for line in review_area.lines() {
        let trimmed = line.trim();
        if trimmed == "## Review Queue"
            || trimmed == REVIEW_QUEUE_MARKER
            || trimmed == "---"
            || trimmed.is_empty()
        {
            in_question = false;
            continue;
        }
        if trimmed.starts_with("- [") {
            in_question = true;
            questions.push(line.to_string());
        } else if in_question && trimmed.starts_with('>') {
            questions.push(line.to_string());
            in_question = false;
        } else {
            in_question = false;
        }
    }

    // Rebuild: body + single review section
    let mut body = content[..body_end].trim_end().to_string();
    if !body.ends_with('\n') {
        body.push('\n');
    }

    if questions.is_empty() {
        return body;
    }

    body.push_str("\n---\n\n## Review Queue\n\n");
    body.push_str(REVIEW_QUEUE_MARKER);
    body.push('\n');
    for line in &questions {
        body.push_str(line);
        body.push('\n');
    }
    body
}

/// Append review questions to a document's Review Queue section.
/// Creates the section if it doesn't exist. Handles pre-existing
/// `## Review Queue` headings without the marker comment.
pub fn append_review_questions(content: &str, questions: &[ReviewQuestion]) -> String {
    // First, merge any duplicate review sections from prior bugs
    let mut result = merge_duplicate_review_sections(content);

    // Check if Review Queue section exists (by marker)
    if !result.contains(REVIEW_QUEUE_MARKER) {
        // Check for bare `## Review Queue` heading without marker
        if let Some(heading_pos) = find_bare_review_heading(&result) {
            // Insert marker after the heading line
            let after_heading = heading_pos
                + result[heading_pos..]
                    .find('\n')
                    .map(|n| n + 1)
                    .unwrap_or(result.len() - heading_pos);
            // Skip blank lines after heading
            let mut insert_at = after_heading;
            while insert_at < result.len()
                && result.as_bytes().get(insert_at) == Some(&b'\n')
            {
                insert_at += 1;
            }
            result.insert_str(insert_at, &format!("{}\n", REVIEW_QUEUE_MARKER));
        } else {
            // No review section at all — create one
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str("\n---\n\n## Review Queue\n\n");
            result.push_str(REVIEW_QUEUE_MARKER);
            result.push('\n');
        }
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
            let type_tag = q.question_type.as_str();
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

/// Ensure content contains the review marker section.
/// If the marker is missing, appends a blank review section.
/// Returns the (possibly modified) content and whether it was changed.
pub fn ensure_review_section(content: &str) -> (String, bool) {
    if content.contains(REVIEW_QUEUE_MARKER) {
        return (content.to_string(), false);
    }
    let mut result = content.to_string();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&format!(
        "\n{}\n## Review Queue\n",
        REVIEW_QUEUE_MARKER
    ));
    (result, true)
}

/// Recover the review section from DB content into disk content.
/// If disk content lacks the marker but db_content has it, extracts the
/// review section (marker + everything after) from db_content and appends
/// it to disk content. Also handles the case where disk has the marker but
/// no questions while DB has questions (e.g. marker was inserted without
/// syncing questions). Returns the merged content and whether it changed.
pub fn recover_review_section(disk_content: &str, db_content: &str) -> (String, bool) {
    if disk_content.contains(REVIEW_QUEUE_MARKER) {
        // Disk has marker — check if it's missing questions that the DB has
        let disk_qs = parse_review_queue(disk_content).unwrap_or_default();
        let db_qs = parse_review_queue(db_content).unwrap_or_default();
        let db_unanswered = db_qs.iter().filter(|q| !q.answered).count();
        if disk_qs.is_empty() && db_unanswered > 0 {
            // Strip disk's empty review section, replace with DB's
            let body = crate::patterns::content_body(disk_content);
            let mut result = body.to_string();
            if let Some(review) = crate::patterns::extract_review_queue_section(db_content) {
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str(review);
                return (result, true);
            }
        }
        return (disk_content.to_string(), false);
    }
    // Try to recover from DB content
    if let Some(marker_pos) = db_content.find(REVIEW_QUEUE_MARKER) {
        // Find the start of the review section (look for --- or ## Review Queue before marker)
        let before_marker = &db_content[..marker_pos];
        let section_start = find_review_section_start(before_marker);
        let review_section = &db_content[section_start..];
        let mut result = disk_content.to_string();
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(review_section);
        return (result, true);
    }
    // Neither has the marker — add a blank one
    ensure_review_section(disk_content)
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
    fn test_prune_preserves_deferred_question_with_answer() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[stale]` Old fact is stale\n> believed: Still accurate per Wikipedia\n";
        let valid = HashSet::new(); // not in valid set, but has a deferred answer
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("Old fact is stale"), "Deferred question should be preserved");
        assert!(result.contains("believed: Still accurate"), "Believed answer should be preserved");
    }

    #[test]
    fn test_prune_preserves_deferred_question_with_defer_note() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` When was this true?\n> defer: could not find source\n";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("When was this true?"), "Deferred question should be preserved");
        assert!(result.contains("defer: could not find source"), "Defer note should be preserved");
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
                "\"Role A\" overlaps with \"Role B\" (line:5) [pattern:parallel_overlap]"
            ),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    #[test]
    fn test_normalize_conflict_desc_pattern_tag_without_line_ref() {
        assert_eq!(
            normalize_conflict_desc("\"Role A\" overlaps with \"Role B\" [pattern:same_entity_transition]"),
            "\"Role A\" overlaps with \"Role B\""
        );
    }

    // ============================================================================
    // Duplicate Review Queue Section Tests
    // ============================================================================

    #[test]
    fn test_append_to_existing_heading_without_marker() {
        // Document has ## Review Queue heading but no marker
        let content = "# Doc\n\n- Fact\n\n---\n\n## Review Queue\n\n";
        let questions = vec![ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(3),
            description: "when was this true?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
        }];
        let result = append_review_questions(content, &questions);
        let heading_count = result.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Should have exactly one ## Review Queue heading, got:\n{result}");
        assert!(result.contains(REVIEW_QUEUE_MARKER), "Should contain marker");
        assert!(result.contains("@q[temporal]"), "Should contain the question");
    }

    #[test]
    fn test_append_twice_no_duplicate_sections() {
        let content = "# Doc\n\n- Fact without temporal tag\n";
        let q1 = vec![ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(3),
            description: "when was this true?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
        }];
        let after_first = append_review_questions(content, &q1);
        let heading_count = after_first.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "First append: one heading");

        // Second append with a different question
        let q2 = vec![ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(3),
            description: "what is the source?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
        }];
        let after_second = append_review_questions(&after_first, &q2);
        let heading_count = after_second.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Second append: still one heading, got:\n{after_second}");
        let marker_count = after_second.matches(REVIEW_QUEUE_MARKER).count();
        assert_eq!(marker_count, 1, "Should have exactly one marker");
    }

    #[test]
    fn test_merge_duplicate_review_sections() {
        // Document with two complete review sections
        let content = "# Doc\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` question one\n  > \n\n\
                       ---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[missing]` question two\n  > \n";
        let result = merge_duplicate_review_sections(content);
        let heading_count = result.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Should merge to one heading, got:\n{result}");
        let marker_count = result.matches(REVIEW_QUEUE_MARKER).count();
        assert_eq!(marker_count, 1, "Should have one marker");
        assert!(result.contains("question one"), "Should preserve first question");
        assert!(result.contains("question two"), "Should preserve second question");
    }

    #[test]
    fn test_merge_duplicate_review_sections_no_duplicates() {
        let content = "# Doc\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` question one\n  > \n";
        let result = merge_duplicate_review_sections(content);
        assert_eq!(result, content, "No change when no duplicates");
    }

    #[test]
    fn test_merge_duplicate_headings_without_markers() {
        // Two headings but only one marker
        let content = "# Doc\n\n- Fact\n\n## Review Queue\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` question\n  > \n";
        let result = merge_duplicate_review_sections(content);
        let heading_count = result.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Should merge duplicate headings, got:\n{result}");
    }

    #[test]
    fn test_ensure_review_section_already_present() {
        let content = "# Doc\n\n<!-- factbase:review -->\n## Review Queue\n";
        let (result, changed) = ensure_review_section(content);
        assert!(!changed);
        assert_eq!(result, content);
    }

    #[test]
    fn test_ensure_review_section_missing() {
        let content = "# Doc\n\nSome content\n";
        let (result, changed) = ensure_review_section(content);
        assert!(changed);
        assert!(result.contains(REVIEW_QUEUE_MARKER));
        assert!(result.contains("## Review Queue"));
        assert!(result.starts_with("# Doc\n\nSome content\n"));
    }

    #[test]
    fn test_ensure_review_section_no_trailing_newline() {
        let content = "# Doc\n\nSome content";
        let (result, changed) = ensure_review_section(content);
        assert!(changed);
        assert!(result.contains(REVIEW_QUEUE_MARKER));
    }

    #[test]
    fn test_recover_review_section_disk_has_marker() {
        let disk = "# Doc\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` q1\n  > \n";
        let db = "# Doc\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` q1\n  > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(!changed);
        assert_eq!(result, disk);
    }

    #[test]
    fn test_recover_review_section_from_db() {
        let disk = "# Doc\n\nSome content\n";
        let db = "# Doc\n\nSome content\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When was this?\n  > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(changed);
        assert!(result.contains(REVIEW_QUEUE_MARKER));
        assert!(result.contains("When was this?"));
        assert!(result.starts_with("# Doc\n\nSome content\n"));
    }

    #[test]
    fn test_recover_review_section_neither_has_marker() {
        let disk = "# Doc\n\nContent\n";
        let db = "# Doc\n\nContent\n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(changed);
        assert!(result.contains(REVIEW_QUEUE_MARKER));
        assert!(result.contains("## Review Queue"));
    }

    #[test]
    fn test_recover_review_section_disk_has_marker_but_empty() {
        let disk = "# Doc\n\nContent\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n";
        let db = "# Doc\n\nContent\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When did this happen?\n  > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(changed);
        assert!(result.contains("When did this happen?"));
        assert!(result.starts_with("# Doc\n\nContent\n"));
    }

    #[test]
    fn test_recover_review_section_disk_has_marker_and_questions() {
        // When disk already has questions, don't replace them
        let disk = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Existing q\n  > \n";
        let db = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Existing q\n  > \n- [ ] `@q[missing]` Another q\n  > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(!changed);
        assert_eq!(result, disk);
    }

    #[test]
    fn test_recover_review_section_disk_empty_db_all_answered() {
        // When DB only has answered questions, don't sync
        let disk = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n";
        let db = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered q\n  > done\n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(!changed);
        assert_eq!(result, disk);
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
