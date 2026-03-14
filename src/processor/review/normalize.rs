//! Review section normalization.
//!
//! Handles deduplication of review headers, stripping orphaned markers and
//! blockquotes, deduplicating questions, and removing empty review sections.

use std::collections::HashSet;

use crate::patterns::{INLINE_QUESTION_MARKER, REVIEW_QUESTION_REGEX, REVIEW_QUEUE_MARKER};

use super::callout::{unwrap_review_callout, wrap_review_callout};
use super::parse::extract_line_ref_and_strip;

/// Normalize the review queue section to prevent format degradation.
///
/// This function:
/// (a) Merges duplicate `## Review Queue` headers into one
/// (b) Removes orphaned `@q[...]` markers outside the review queue section
/// (c) Strips empty blockquote lines (`>` with only whitespace) not part of an answer
/// (d) Removes the entire section if no questions remain
pub fn normalize_review_section(content: &str) -> String {
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = normalize_review_section_inner(&unwrapped);
    if was_callout {
        wrap_review_callout(&result)
    } else {
        result
    }
}

pub(super) fn normalize_review_section_inner(content: &str) -> String {
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
    // Ensure body ends with a newline so the separator is never smashed onto
    // the last body line (e.g. `[^5]: citation text---`).
    let clean_body = if clean_body.ends_with('\n') {
        clean_body
    } else {
        clean_body + "\n"
    };

    // (a) Remove duplicate ## Review Queue headers from section_header
    // Keep only the last one (closest to marker)
    let clean_header = dedup_review_headers(section_header);
    // Ensure a blank line precedes the --- separator so it is never joined to
    // the last body line when the original content lacked the blank line.
    let clean_header = if clean_header.starts_with('\n') {
        clean_header
    } else {
        "\n".to_string() + &clean_header
    };

    // (c) Strip empty blockquote lines not part of an answer in the queue content
    let clean_queue = strip_orphaned_blockquotes(after_marker);

    // (c2) Dedup exact duplicate questions by description, keeping last occurrence
    let clean_queue = dedup_review_questions(&clean_queue);

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

/// Remove exact duplicate review questions (same description text).
/// When duplicates exist, keeps the last occurrence (which may have a newer answer).
fn dedup_review_questions(queue_content: &str) -> String {
    let lines: Vec<&str> = queue_content.lines().collect();
    let mut blocks: Vec<(String, Vec<&str>)> = Vec::new();
    let mut non_question_prefix: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some(cap) = REVIEW_QUESTION_REGEX.captures(trimmed) {
            let (_, desc) = extract_line_ref_and_strip(&cap[3]);
            let mut block_lines = vec![lines[i]];
            i += 1;
            while i < lines.len() {
                let next = lines[i].trim();
                if next.starts_with('>') || next.is_empty() {
                    block_lines.push(lines[i]);
                    i += 1;
                    if next.starts_with('>') {
                        break;
                    }
                } else {
                    break;
                }
            }
            blocks.push((desc, block_lines));
        } else {
            non_question_prefix.push(lines[i]);
            i += 1;
        }
    }

    let mut seen = HashSet::new();
    let mut keep = vec![false; blocks.len()];
    for (idx, (desc, _)) in blocks.iter().enumerate().rev() {
        if seen.insert(desc.as_str()) {
            keep[idx] = true;
        }
    }

    let mut result: Vec<&str> = non_question_prefix;
    for (idx, (_, block_lines)) in blocks.iter().enumerate() {
        if keep[idx] {
            result.extend(block_lines);
        }
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_footnote_not_smashed_onto_separator() {
        // Regression: footnote line must be separated from --- by a blank line.
        // Previously `strip_orphaned_markers` dropped the trailing newline from
        // the body, causing `[^5]: citation text---` when section_header had no
        // leading blank line.
        let content = "# Doc\n\n- Fact [^1]\n\n---\n[^1]: citation text\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` when?\n  > \n";
        let result = normalize_review_section(content);
        // The footnote line must not be immediately followed by ---
        assert!(
            !result.contains("citation text---"),
            "footnote smashed onto separator:\n{result}"
        );
        // --- must appear on its own line
        for line in result.lines() {
            if line.trim() == "---" {
                // good
            } else {
                assert!(
                    !line.contains("---"),
                    "--- embedded in non-separator line: {line:?}\nfull:\n{result}"
                );
            }
        }
        // Content and review section must still be present
        assert!(result.contains("[^1]: citation text"));
        assert!(result.contains("## Review Queue"));
        assert!(result.contains("when?"));
    }

    #[test]
    fn test_normalize_footnote_blank_line_before_separator() {
        // When the body ends with a footnote, there must be a blank line before ---.
        let content = "# Doc\n\n- Fact [^1]\n\n---\n[^1]: citation, 2026-03-14\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` source?\n  > \n";
        let result = normalize_review_section(content);
        // Find the footnote line and check the next non-empty line is ---
        let lines: Vec<&str> = result.lines().collect();
        let fn_idx = lines
            .iter()
            .position(|l| l.starts_with("[^1]:"))
            .expect("footnote line missing");
        // There must be a blank line between footnote and ---
        assert!(
            fn_idx + 1 < lines.len() && lines[fn_idx + 1].is_empty(),
            "expected blank line after footnote, got: {:?}\nfull:\n{result}",
            lines.get(fn_idx + 1)
        );
    }

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
        assert!(result.contains("- Fact here"));
        assert!(!result.contains("- Fact here `@q[stale]`"));
        assert!(result.contains("`@q[temporal]`"));
    }

    #[test]
    fn test_normalize_strips_orphaned_blockquotes() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` question\n  > \n> \n> \n- [ ] `@q[missing]` another\n  > \n";
        let result = normalize_review_section(content);
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

    #[test]
    fn test_normalize_strips_exact_duplicates() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Source lacks detail\n  > \n- [ ] `@q[weak-source]` Source lacks detail\n  > \n";
        let result = normalize_review_section(content);
        assert_eq!(result.matches("Source lacks detail").count(), 1);
    }

    #[test]
    fn test_dedup_keeps_different_descriptions() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Source A lacks detail\n  > \n- [ ] `@q[weak-source]` Source B lacks detail\n  > \n";
        let result = normalize_review_section(content);
        assert!(result.contains("Source A lacks detail"));
        assert!(result.contains("Source B lacks detail"));
    }

    #[test]
    fn test_dedup_same_desc_different_answers_keeps_latest() {
        let content = "# Doc\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When did this happen?\n  > \n- [ ] `@q[temporal]` When did this happen?\n  > believed: around 2020\n";
        let result = normalize_review_section(content);
        assert_eq!(result.matches("When did this happen?").count(), 1);
        assert!(
            result.contains("believed: around 2020"),
            "Should keep the last occurrence with the answer"
        );
    }
}
