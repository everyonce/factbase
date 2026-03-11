//! Pruning and stripping of review questions.
//!
//! Handles removing answered questions, pruning stale questions whose trigger
//! conditions no longer exist, and related filtering operations.

use std::collections::HashSet;

use crate::patterns::REVIEW_QUEUE_MARKER;

use super::callout::{unwrap_review_callout, wrap_review_callout};
use super::parse::{extract_line_ref_and_strip, normalize_conflict_desc};

/// Remove unanswered questions whose trigger conditions no longer exist.
///
/// Strip only answered `[x]`/`[X]` questions (and their blockquote answers)
/// from the review queue. Returns `(pruned_content, count_removed)`.
/// Unlike `prune_stale_questions`, this preserves ALL unanswered and deferred questions.
pub fn strip_answered_questions(content: &str) -> (String, usize) {
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let (result, count) = strip_answered_questions_inner(&unwrapped);
    if was_callout && count > 0 {
        (wrap_review_callout(&result), count)
    } else {
        (result, count)
    }
}

fn strip_answered_questions_inner(content: &str) -> (String, usize) {
    let Some(marker_pos) = content.find(REVIEW_QUEUE_MARKER) else {
        return (content.to_string(), 0);
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

        if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
            skip_answer = true;
            pruned += 1;
        } else if skip_answer && trimmed.starts_with('>') {
            // skip blockquote answer of a pruned question
        } else {
            result_lines.push(line);
            skip_answer = false;
        }
        i += 1;
    }

    if pruned == 0 {
        return (content.to_string(), 0);
    }

    let has_questions = result_lines
        .iter()
        .any(|l| l.trim_start().starts_with("- ["));

    let output = if has_questions {
        format!(
            "{}{}\n{}",
            before_marker,
            REVIEW_QUEUE_MARKER,
            result_lines.join("\n")
        )
    } else {
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
    };

    (output, pruned)
}

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
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = prune_stale_questions_inner(&unwrapped, valid_descriptions, had_deep_check);
    if was_callout {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn prune_stale_questions_inner(
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

            // Keep: deferred (has blockquote answer), cross-check, or valid description.
            // Answered ([x]) questions are always pruned — their answers live in the DB.
            if is_answered {
                skip_answer = true;
                pruned += 1;
            } else if has_answer
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prune_removes_stale_unanswered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` \"Old fact\" - when was this true?\n  > \n";
        let valid = HashSet::new();
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
    fn test_prune_removes_answered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [x] `@q[temporal]` \"Old fact\" - when was this true?\n  > 2024\n";
        let valid = HashSet::new();
        let result = prune_stale_questions(content, &valid, false);
        assert!(!result.contains("Old fact"), "Answered question should be pruned");
        assert!(!result.contains("Review Queue"), "Empty review section should be removed");
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
        let valid = HashSet::new();
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
    fn test_prune_mixed_state_answered_removed_deferred_kept() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [x] `@q[temporal]` \"Answered fact\" - when?\n\
                       - [ ] `@q[stale]` Deferred fact\n> defer: could not confirm\n\
                       - [ ] `@q[missing]` Valid unanswered\n";
        let mut valid = HashSet::new();
        valid.insert("Valid unanswered".to_string());
        let result = prune_stale_questions(content, &valid, false);
        assert!(!result.contains("Answered fact"), "Answered [x] question should be pruned");
        assert!(result.contains("Deferred fact"), "Deferred question should be preserved");
        assert!(result.contains("Valid unanswered"), "Valid unanswered question should be preserved");
    }

    #[test]
    fn test_strip_answered_removes_checked() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [x] `@q[temporal]` answered question\n  > 2024\n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 1);
        assert!(!result.contains("answered question"));
        assert!(!result.contains("Review Queue"));
    }

    #[test]
    fn test_strip_answered_removes_uppercase_x() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [X] `@q[temporal]` answered question\n  > 2024\n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 1);
        assert!(!result.contains("answered question"));
    }

    #[test]
    fn test_strip_answered_preserves_unanswered() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` unanswered question\n  > \n\
                       - [x] `@q[missing]` answered one\n  > done\n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 1);
        assert!(result.contains("unanswered question"));
        assert!(!result.contains("answered one"));
    }

    #[test]
    fn test_strip_answered_preserves_deferred() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` deferred question\n  > defer: need more info\n\
                       - [x] `@q[missing]` answered one\n  > done\n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 1);
        assert!(result.contains("deferred question"));
        assert!(result.contains("defer: need more info"));
        assert!(!result.contains("answered one"));
    }

    #[test]
    fn test_strip_answered_noop_when_none() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n\
                       - [ ] `@q[temporal]` unanswered question\n  > \n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 0);
        assert_eq!(result, content);
    }

    #[test]
    fn test_strip_answered_noop_no_review_section() {
        let content = "# Doc\n\nJust content.\n";
        let (result, count) = strip_answered_questions(content);
        assert_eq!(count, 0);
        assert_eq!(result, content);
    }
}
