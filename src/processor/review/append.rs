//! Review question appending, merging, and section management.
//!
//! Handles appending new questions to documents, merging duplicate review
//! sections, ensuring review sections exist, and recovering review sections.

use std::collections::HashSet;

use crate::models::ReviewQuestion;
use crate::patterns::{REVIEW_CALLOUT_HEADER, REVIEW_QUEUE_MARKER};

use super::callout::{is_callout_review, unwrap_review_callout, wrap_review_callout};
use super::normalize::normalize_review_section_inner;
use super::parse::parse_review_queue;

/// Merge duplicate `## Review Queue` sections into one.
///
/// If a document has multiple review queue sections (with or without markers),
/// this consolidates all questions into a single section. Returns the cleaned
/// content.
pub fn merge_duplicate_review_sections(content: &str) -> String {
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = merge_duplicate_review_sections_inner(&unwrapped);
    if was_callout {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn merge_duplicate_review_sections_inner(content: &str) -> String {
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
pub fn append_review_questions(
    content: &str,
    questions: &[ReviewQuestion],
    use_callout: bool,
) -> String {
    // Detect existing callout format — preserve it regardless of use_callout
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = append_review_questions_inner(&unwrapped, questions, use_callout);
    if was_callout || use_callout {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn append_review_questions_inner(
    content: &str,
    questions: &[ReviewQuestion],
    _use_callout: bool,
) -> String {
    // First, merge any duplicate review sections from prior bugs
    let mut result = merge_duplicate_review_sections_inner(content);

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

        // Collect existing question descriptions to skip duplicates
        let existing_qs = parse_review_queue(&result).unwrap_or_default();
        let existing: HashSet<&str> = existing_qs
            .iter()
            .map(|q| q.description.as_str())
            .collect();

        let mut questions_text = String::new();

        for q in questions {
            if existing.contains(q.description.as_str()) {
                continue;
            }
            let line_ref = q
                .line_ref
                .map(|n| format!("Line {n}: "))
                .unwrap_or_default();
            let type_tag = q.question_type.as_str();
            {
                crate::write_str!(
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

    normalize_review_section_inner(&result)
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

/// Ensure content contains the review marker section.
/// If the marker is missing, appends a blank review section.
/// Returns the (possibly modified) content and whether it was changed.
pub fn ensure_review_section(content: &str, use_callout: bool) -> (String, bool) {
    if content.contains(REVIEW_QUEUE_MARKER) {
        return (content.to_string(), false);
    }
    let mut result = content.to_string();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    if use_callout {
        result.push_str(&format!(
            "\n{}\n> {}\n",
            REVIEW_CALLOUT_HEADER, REVIEW_QUEUE_MARKER
        ));
    } else {
        result.push_str(&format!(
            "\n{}\n## Review Queue\n",
            REVIEW_QUEUE_MARKER
        ));
    }
    (result, true)
}

/// Recover the review section from DB content into disk content.
/// If disk content lacks the marker but db_content has it, extracts the
/// review section (marker + everything after) from db_content and appends
/// it to disk content. Also handles the case where disk has the marker but
/// no questions while DB has questions (e.g. marker was inserted without
/// syncing questions). Returns the merged content and whether it changed.
pub fn recover_review_section(disk_content: &str, db_content: &str) -> (String, bool) {
    if disk_content.contains(REVIEW_QUEUE_MARKER) || is_callout_review(disk_content) {
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
    // Try to recover from DB content using extract_review_queue_section
    // which correctly handles both plain and callout-wrapped review sections.
    if let Some(review_section) = crate::patterns::extract_review_queue_section(db_content) {
        let mut result = disk_content.to_string();
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(review_section);
        return (result, true);
    }
    // Neither has the marker — add a blank one
    ensure_review_section(disk_content, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuestionType, ReviewQuestion};
    use crate::patterns::REVIEW_QUEUE_MARKER;

    #[test]
    fn test_append_to_existing_heading_without_marker() {
        let content = "# Doc\n\n- Fact\n\n---\n\n## Review Queue\n\n";
        let questions = vec![ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(3),
            description: "when was this true?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
            confidence: None,
            confidence_reason: None,
        }];
        let result = append_review_questions(content, &questions, false);
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
            confidence: None,
            confidence_reason: None,
        }];
        let after_first = append_review_questions(content, &q1, false);
        let heading_count = after_first.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "First append: one heading");

        let q2 = vec![ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: Some(3),
            description: "what is the source?".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
            confidence: None,
            confidence_reason: None,
        }];
        let after_second = append_review_questions(&after_first, &q2, false);
        let heading_count = after_second.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Second append: still one heading, got:\n{after_second}");
        let marker_count = after_second.matches(REVIEW_QUEUE_MARKER).count();
        assert_eq!(marker_count, 1, "Should have exactly one marker");
    }

    #[test]
    fn test_merge_duplicate_review_sections() {
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
        let content = "# Doc\n\n- Fact\n\n## Review Queue\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` question\n  > \n";
        let result = merge_duplicate_review_sections(content);
        let heading_count = result.lines().filter(|l| l.trim() == "## Review Queue").count();
        assert_eq!(heading_count, 1, "Should merge duplicate headings, got:\n{result}");
    }

    #[test]
    fn test_ensure_review_section_already_present() {
        let content = "# Doc\n\n<!-- factbase:review -->\n## Review Queue\n";
        let (result, changed) = ensure_review_section(content, false);
        assert!(!changed);
        assert_eq!(result, content);
    }

    #[test]
    fn test_ensure_review_section_missing() {
        let content = "# Doc\n\nSome content\n";
        let (result, changed) = ensure_review_section(content, false);
        assert!(changed);
        assert!(result.contains(REVIEW_QUEUE_MARKER));
        assert!(result.contains("## Review Queue"));
        assert!(result.starts_with("# Doc\n\nSome content\n"));
    }

    #[test]
    fn test_ensure_review_section_no_trailing_newline() {
        let content = "# Doc\n\nSome content";
        let (result, changed) = ensure_review_section(content, false);
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
        let disk = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Existing q\n  > \n";
        let db = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Existing q\n  > \n- [ ] `@q[missing]` Another q\n  > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(!changed);
        assert_eq!(result, disk);
    }

    #[test]
    fn test_recover_review_section_disk_empty_db_all_answered() {
        let disk = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n";
        let db = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Answered q\n  > done\n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(!changed);
        assert_eq!(result, disk);
    }

    #[test]
    fn test_recover_review_section_callout_from_db() {
        let disk = "# Doc\n\nSome content\n";
        let db = "# Doc\n\nSome content\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(changed);
        assert!(result.contains("> [!info]- Review Queue"), "should preserve callout format from DB");
        assert!(result.contains("> <!-- factbase:review -->"));
        assert!(result.contains("When?"));
    }

    #[test]
    fn test_recover_review_section_callout_disk_empty() {
        let disk = "# Doc\n\nContent\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n";
        let db = "# Doc\n\nContent\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let (result, changed) = recover_review_section(disk, db);
        assert!(changed);
        assert!(result.contains("When?"));
        assert!(result.contains("> [!info]- Review Queue"));
    }

    #[test]
    fn test_append_duplicate_question_is_noop() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Source lacks detail\n  > \n";
        let questions = vec![ReviewQuestion::new(
            QuestionType::Missing,
            None,
            "Source lacks detail".to_string(),
        )];
        let result = append_review_questions(content, &questions, false);
        assert_eq!(result.matches("Source lacks detail").count(), 1);
    }
}
