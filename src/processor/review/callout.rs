//! Callout format conversion for review sections.
//!
//! Handles detection and conversion between plain `## Review Queue` format
//! and Obsidian `> [!review]- Review Queue` callout format.

use crate::patterns::{REVIEW_CALLOUT_HEADER, REVIEW_CALLOUT_HEADER_LEGACY, REVIEW_QUEUE_MARKER};

/// Detect whether the review section uses Obsidian callout format.
///
/// Looks for `> <!-- factbase:review -->` (marker prefixed with `> `).
pub fn is_callout_review(content: &str) -> bool {
    content.contains(&format!("> {REVIEW_QUEUE_MARKER}"))
}

/// Convert callout-wrapped review section to plain format for processing.
///
/// Returns `(unwrapped_content, was_callout)`. If the content doesn't use
/// callout format, returns it unchanged with `false`.
pub fn unwrap_review_callout(content: &str) -> (String, bool) {
    if !is_callout_review(content) {
        return (content.to_string(), false);
    }

    // Find the callout header line (accept both current and legacy header)
    let mut lines: Vec<&str> = content.lines().collect();
    let callout_start = lines
        .iter()
        .position(|l| l.trim() == REVIEW_CALLOUT_HEADER || l.trim() == REVIEW_CALLOUT_HEADER_LEGACY);
    let Some(start_idx) = callout_start else {
        // Has `> <!-- factbase:review -->` but no callout header — strip `> ` from marker line onward
        let marker_line = lines.iter().position(|l| l.trim() == format!("> {REVIEW_QUEUE_MARKER}"));
        if let Some(idx) = marker_line {
            for line in &mut lines[idx..] {
                *line = line.strip_prefix("> ").or_else(|| line.strip_prefix(">")).unwrap_or(line);
            }
            // Insert plain heading before marker
            let heading = vec!["---", "", "## Review Queue", ""];
            let mut result: Vec<&str> = lines[..idx].to_vec();
            result.extend_from_slice(&heading);
            result.extend_from_slice(&lines[idx..]);
            return (result.join("\n"), true);
        }
        return (content.to_string(), false);
    };

    // Strip `> ` prefix from callout header and all subsequent lines
    // Replace callout header with plain heading
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 3);

    // Copy body lines before callout, trimming trailing blank lines
    let mut body_end = start_idx;
    while body_end > 0 && lines[body_end - 1].trim().is_empty() {
        body_end -= 1;
    }
    for line in &lines[..body_end] {
        result.push(line.to_string());
    }

    // Add plain review section header
    result.push(String::new());
    result.push("---".to_string());
    result.push(String::new());
    result.push("## Review Queue".to_string());
    result.push(String::new());

    // Strip `> ` from remaining lines (skip the callout header itself)
    for line in &lines[start_idx + 1..] {
        let stripped = line.strip_prefix("> ").or_else(|| line.strip_prefix(">")).unwrap_or(line);
        result.push(stripped.to_string());
    }

    (result.join("\n"), true)
}

/// Convert plain review section to Obsidian callout format.
///
/// Finds the review section (separator + heading + marker + questions) and
/// wraps it in a collapsed `> [!review]- Review Queue` callout.
pub fn wrap_review_callout(content: &str) -> String {
    if !content.contains(REVIEW_QUEUE_MARKER) {
        return content.to_string();
    }
    // Already in callout format
    if is_callout_review(content) {
        return content.to_string();
    }

    let body_end = crate::patterns::body_end_offset(content);
    let mut body = content[..body_end].trim_end_matches('\n').to_string();
    let review_area = &content[body_end..];

    // Strip trailing `---` separator from body (not needed in callout format)
    let trimmed = body.trim_end();
    if trimmed.ends_with("---") {
        body = trimmed.trim_end_matches("---").trim_end_matches('\n').to_string();
    }

    // Find the marker in the review area and collect lines after it
    let mut review_lines: Vec<&str> = Vec::new();
    let mut found_marker = false;
    for line in review_area.lines() {
        let t = line.trim();
        if t == REVIEW_QUEUE_MARKER {
            found_marker = true;
            review_lines.push(line);
        } else if found_marker {
            review_lines.push(line);
        }
        // Skip separator, heading, blank lines before marker
    }

    if !found_marker {
        return content.to_string();
    }

    let mut result = body;
    result.push_str("\n\n");
    result.push_str(REVIEW_CALLOUT_HEADER);
    result.push('\n');
    for line in &review_lines {
        if line.is_empty() {
            result.push_str(">\n");
        } else {
            result.push_str("> ");
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{
        append_review_questions, ensure_review_section, normalize_review_section,
        parse_review_queue, prune_stale_questions, strip_answered_questions,
    };
    use crate::models::{QuestionType, ReviewQuestion};
    use std::collections::HashSet;

    #[test]
    fn test_is_callout_review_plain() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When?\n  > \n";
        assert!(!is_callout_review(content));
    }

    #[test]
    fn test_is_callout_review_callout() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        assert!(is_callout_review(content));
    }

    #[test]
    fn test_unwrap_callout_plain_unchanged() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When?\n  > \n";
        let (result, was_callout) = unwrap_review_callout(content);
        assert!(!was_callout);
        assert_eq!(result, content);
    }

    #[test]
    fn test_unwrap_callout_strips_prefix() {
        let content = "# Doc\n\nSome fact\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let (result, was_callout) = unwrap_review_callout(content);
        assert!(was_callout);
        assert!(result.contains("## Review Queue"));
        assert!(result.contains("<!-- factbase:review -->"));
        assert!(result.contains("- [ ] `@q[temporal]` When?"));
        assert!(!result.contains("> [!review]- Review Queue"));
    }

    #[test]
    fn test_wrap_callout_plain_to_callout() {
        let content = "# Doc\n\nSome fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When?\n  > \n";
        let result = wrap_review_callout(content);
        assert!(result.contains("> [!review]- Review Queue"));
        assert!(result.contains("> <!-- factbase:review -->"));
        assert!(result.contains("> - [ ] `@q[temporal]` When?"));
        assert!(!result.contains("---\n\n## Review Queue"));
    }

    #[test]
    fn test_wrap_callout_already_callout_noop() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let result = wrap_review_callout(content);
        assert_eq!(result, content);
    }

    #[test]
    fn test_parse_review_queue_callout_format() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When was this true?\n>   > \n";
        let questions = parse_review_queue(content).unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Temporal);
        assert_eq!(questions[0].description, "When was this true?");
    }

    #[test]
    fn test_parse_review_queue_callout_with_answer() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [x] `@q[temporal]` When was this true?\n> > believed: Still accurate as of 2024\n";
        let questions = parse_review_queue(content).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(questions[0].answered);
        assert!(questions[0].answer.as_ref().unwrap().contains("believed"));
    }

    #[test]
    fn test_append_review_questions_callout_new_section() {
        let content = "# Doc\n\nSome fact\n";
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
        let result = append_review_questions(content, &questions, true);
        assert!(result.contains("> [!review]- Review Queue"), "Should have callout header, got:\n{result}");
        assert!(result.contains("> <!-- factbase:review -->"), "Should have callout marker");
        assert!(result.contains("> - [ ] `@q[temporal]`"), "Should have callout-prefixed question");
        assert!(!result.contains("---\n"), "Should not have --- separator");
    }

    #[test]
    fn test_append_review_questions_preserves_existing_callout() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` existing question\n>   > \n";
        let questions = vec![ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: None,
            description: "new question".to_string(),
            answered: false,
            answer: None,
            line_number: 0,
            confidence: None,
            confidence_reason: None,
        }];
        // Even with use_callout=false, existing callout format is preserved
        let result = append_review_questions(content, &questions, false);
        assert!(result.contains("> [!review]- Review Queue"), "Should preserve callout format");
        assert!(result.contains("existing question"), "Should keep existing question");
        assert!(result.contains("new question"), "Should add new question");
    }

    #[test]
    fn test_strip_answered_callout_preserves_format() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [x] `@q[temporal]` first question\n> > believed: yes\n> - [ ] `@q[missing]` second question\n>   > \n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 1);
        assert!(result.contains("> [!review]- Review Queue"), "Should preserve callout format");
        assert!(!result.contains("first question"), "Should strip answered");
        assert!(result.contains("second question"), "Should keep unanswered");
    }

    #[test]
    fn test_normalize_callout_preserves_format() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` question\n>   > \n";
        let result = normalize_review_section(content);
        assert!(result.contains("> [!review]- Review Queue"), "Should preserve callout format");
    }

    #[test]
    fn test_ensure_review_section_callout() {
        let content = "# Doc\n\nSome fact\n";
        let (result, changed) = ensure_review_section(content, true);
        assert!(changed);
        assert!(result.contains("> [!review]- Review Queue"), "Should create callout section");
        assert!(result.contains("> <!-- factbase:review -->"), "Should have callout marker");
    }

    #[test]
    fn test_ensure_review_section_plain() {
        let content = "# Doc\n\nSome fact\n";
        let (result, changed) = ensure_review_section(content, false);
        assert!(changed);
        assert!(result.contains("## Review Queue"), "Should create plain section");
        assert!(!result.contains("> [!info]"), "Should not have callout");
    }

    #[test]
    fn test_roundtrip_unwrap_wrap() {
        let callout = "# Doc\n\nSome fact\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let (unwrapped, was_callout) = unwrap_review_callout(callout);
        assert!(was_callout);
        let rewrapped = wrap_review_callout(&unwrapped);
        // Parse both and compare questions
        let orig_qs = parse_review_queue(callout).unwrap();
        let round_qs = parse_review_queue(&rewrapped).unwrap();
        assert_eq!(orig_qs.len(), round_qs.len());
        assert_eq!(orig_qs[0].question_type, round_qs[0].question_type);
        assert_eq!(orig_qs[0].description, round_qs[0].description);
    }

    #[test]
    fn test_prune_stale_callout_preserves_format() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` valid question\n>   > \n> - [ ] `@q[missing]` stale question\n>   > \n";
        let mut valid = HashSet::new();
        valid.insert("valid question".to_string());
        let result = prune_stale_questions(content, &valid, false);
        assert!(result.contains("> [!review]- Review Queue"), "Should preserve callout");
        assert!(result.contains("valid question"), "Should keep valid");
        assert!(!result.contains("stale question"), "Should prune stale");
    }

    #[test]
    fn test_legacy_callout_header_accepted_on_read() {
        // Documents written with the old [!info] header should still be readable
        let legacy = "# Doc\n\nSome fact\n\n> [!info]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[temporal]` When?\n>   > \n";
        let (unwrapped, was_callout) = unwrap_review_callout(legacy);
        assert!(was_callout, "Legacy [!info] header should be detected as callout");
        assert!(!unwrapped.contains("> [!info]"), "Should strip callout prefix");
        // Re-wrapping should use the new header
        let rewrapped = wrap_review_callout(&unwrapped);
        assert!(rewrapped.contains("> [!review]- Review Queue"), "Re-wrap should use new header");
    }
}
