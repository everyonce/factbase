//! Apply changes to documents based on interpreted answers.

use chrono::NaiveDate;

use crate::error::FactbaseError;
use crate::patterns::{
    add_or_update_reviewed_marker, body_end_offset, FACT_LINE_REGEX, REVIEWED_MARKER_REGEX,
    REVIEW_QUEUE_MARKER, SOURCE_DEF_REGEX,
};
use crate::processor::{unwrap_review_callout, wrap_review_callout};
use crate::ReviewQuestion;

use super::{ChangeInstruction, InterpretedAnswer};

/// Apply changes to a document section
pub async fn apply_changes_to_section(
    section: &str,
    instructions: &[InterpretedAnswer],
) -> Result<String, FactbaseError> {
    // Filter out dismiss instructions
    let active_instructions: Vec<_> = instructions
        .iter()
        .filter(|ia| {
            !matches!(
                ia.instruction,
                ChangeInstruction::Dismiss | ChangeInstruction::Defer
            )
        })
        .collect();

    if active_instructions.is_empty() {
        return Ok(section.to_string());
    }

    // Check if all remaining are simple deletes (can handle without LLM)
    let all_deletes = active_instructions
        .iter()
        .all(|ia| matches!(ia.instruction, ChangeInstruction::Delete { .. }));

    if all_deletes {
        return apply_deletes_without_llm(section, &active_instructions);
    }

    // For non-delete changes (Split, Generic) that previously needed LLM:
    // apply any deletes we can, skip the rest. Better than failing the whole document.
    apply_deletes_without_llm(section, &active_instructions)
}

/// Apply delete instructions without LLM
fn apply_deletes_without_llm(
    section: &str,
    instructions: &[&InterpretedAnswer],
) -> Result<String, FactbaseError> {
    let mut lines: Vec<&str> = section.lines().collect();

    for ia in instructions {
        if let ChangeInstruction::Delete { line_text } = &ia.instruction {
            lines.retain(|line| !line.contains(line_text.as_str()));
        }
    }

    Ok(lines.join("\n"))
}

/// Apply source citation footnotes deterministically (no LLM).
///
/// For each (line_text, source_info) pair:
/// 1. Finds the next available footnote number
/// 2. Appends `[^N]` to the matching fact line (before reviewed marker if present)
/// 3. Adds `[^N]: source_info` to the footnotes section
pub fn apply_source_citations(content: &str, sources: &[(&str, &str)]) -> String {
    if sources.is_empty() {
        return content.to_string();
    }

    // Find max existing footnote number and build existing def map for dedup
    let mut max_footnote = 0u32;
    let mut existing_defs: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for line in content.lines() {
        if let Some(cap) = SOURCE_DEF_REGEX.captures(line) {
            if let Ok(num) = cap[1].parse::<u32>() {
                max_footnote = max_footnote.max(num);
                // Normalize: trim the definition text for comparison
                let def_text = cap[2].trim().to_lowercase();
                existing_defs.entry(def_text).or_insert(num);
            }
        }
    }

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let mut footnote_defs: Vec<String> = Vec::new();
    let mut next_num = max_footnote + 1;

    for &(line_text, source_info) in sources {
        // Check if an identical footnote already exists — reuse its number
        let normalized = source_info.trim().to_lowercase();
        let ref_num = if let Some(&existing_num) = existing_defs.get(&normalized) {
            existing_num
        } else {
            // Tentatively assign next number — only commit if line is found
            next_num
        };

        // Find the first line containing line_text (skip if line_text is empty)
        if line_text.is_empty() {
            continue;
        }
        let Some(line) = lines.iter_mut().find(|l| l.contains(line_text)) else {
            continue;
        };
        // Skip if this line already has this footnote ref
        let ref_tag = format!("[^{ref_num}]");
        if line.contains(&ref_tag) {
            continue;
        }
        // Insert before reviewed marker if present, otherwise append
        if let Some(m) = REVIEWED_MARKER_REGEX.find(line.as_str()) {
            let pos = line[..m.start()].trim_end().len();
            line.replace_range(pos..m.start(), &format!(" {ref_tag} "));
        } else {
            line.push_str(&format!(" {ref_tag}"));
        }
        // Commit the footnote definition if it's new
        if let std::collections::hash_map::Entry::Vacant(e) = existing_defs.entry(normalized) {
            e.insert(ref_num);
            footnote_defs.push(format!("[^{ref_num}]: {source_info}"));
            next_num += 1;
        }
    }

    if footnote_defs.is_empty() {
        return content.to_string();
    }

    // Find insertion point for footnote definitions
    let last_def_idx = lines.iter().rposition(|l| SOURCE_DEF_REGEX.is_match(l));

    if let Some(idx) = last_def_idx {
        // Append after last existing footnote definition
        for (i, def) in footnote_defs.into_iter().enumerate() {
            lines.insert(idx + 1 + i, def);
        }
    } else {
        // No existing footnotes — insert before review queue or at end.
        // Use body_end_offset to correctly handle callout-wrapped review sections
        // (the callout header line comes before the marker line).
        let joined = lines.join("\n");
        let byte_offset = body_end_offset(&joined);
        let insert_idx = joined[..byte_offset].matches('\n').count();
        // Add separator and definitions
        let mut to_insert = vec!["".to_string(), "---".to_string()];
        to_insert.extend(footnote_defs);
        for (i, line) in to_insert.into_iter().enumerate() {
            lines.insert(insert_idx + i, line);
        }
    }

    lines.join("\n")
}

/// Apply confirmation temporal tag updates deterministically (no LLM).
///
/// For each `(line_text, old_tag, new_tag)` triple:
/// - If `old_tag` is Some, replaces it with `new_tag` on the matching line
/// - If `old_tag` is None, inserts `new_tag` before footnotes/reviewed markers
pub fn apply_confirmations(content: &str, updates: &[(&str, Option<&str>, &str)]) -> String {
    if updates.is_empty() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    for &(line_text, old_tag, new_tag) in updates {
        if line_text.is_empty() {
            continue;
        }
        let Some(line) = lines.iter_mut().find(|l| l.contains(line_text)) else {
            continue;
        };
        if let Some(old) = old_tag {
            // Replace existing temporal tag
            *line = line.replacen(old, new_tag, 1);
        } else {
            // Add temporal tag — insert before footnote refs, reviewed markers, or end
            let insert_pos = REVIEWED_MARKER_REGEX
                .find(line.as_str())
                .map(|m| m.start())
                .or_else(|| {
                    // Before first footnote reference [^N]
                    line.find("[^")
                })
                .unwrap_or(line.len());
            let pos = line[..insert_pos].trim_end().len();
            line.insert_str(pos, &format!(" {new_tag}"));
        }
    }
    lines.join("\n")
}

/// Identify the section of a document affected by questions
pub fn identify_affected_section(
    content: &str,
    questions: &[ReviewQuestion],
) -> Option<(usize, usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }

    // Find all referenced line numbers
    let mut line_refs: Vec<usize> = questions.iter().filter_map(|q| q.line_ref).collect();

    if line_refs.is_empty() {
        // No line references, return entire content
        return Some((1, lines.len(), content.to_string()));
    }

    line_refs.sort();
    line_refs.dedup();

    // Find section bounds (from ## heading to next ## or end)
    // Safe: line_refs is non-empty after the check above
    let min_line = line_refs.first().copied().unwrap_or(1);
    let max_line = line_refs.last().copied().unwrap_or(lines.len());

    // Determine the minimum start line: skip YAML frontmatter and
    // the document title (# Heading) so they are never sent to the LLM.  This
    // prevents the LLM from duplicating the title in its output.
    let mut min_start = 1; // 1-based
    let fm_lines = crate::patterns::frontmatter_line_count(content);
    if fm_lines > 0 {
        min_start = fm_lines + 1;
    }
    for (i, line) in lines.iter().enumerate().skip(fm_lines) {
        if line.starts_with("# ") {
            min_start = i + 2; // 1-based line after this one
        } else if !line.trim().is_empty() {
            break;
        }
    }

    let mut start = min_start;
    let mut end = lines.len();

    // Find section start (look backwards for ## heading)
    for i in (0..min_line.saturating_sub(1).min(lines.len())).rev() {
        if lines[i].starts_with("## ") {
            start = i + 1;
            break;
        }
    }

    // Ensure start is never before the document header/title
    if start < min_start {
        start = min_start;
    }

    // Find section end (look forwards for ## heading)
    for (i, line) in lines.iter().enumerate().skip(max_line.min(lines.len())) {
        if line.starts_with("## ") {
            end = i;
            break;
        }
    }

    let section_lines: Vec<&str> = lines[start.saturating_sub(1)..end.min(lines.len())].to_vec();
    let section_content = section_lines.join("\n");

    Some((start, end, section_content))
}

/// Replace a section in document content
pub fn replace_section(content: &str, start: usize, end: usize, new_section: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let new_lines_count = new_section.lines().count();
    let mut result = Vec::with_capacity(lines.len() + new_lines_count);

    // Add lines before section
    for line in lines.iter().take(start.saturating_sub(1)) {
        result.push(*line);
    }

    // Add new section
    for line in new_section.lines() {
        result.push(line);
    }

    // Add lines after section
    for line in lines.iter().skip(end) {
        result.push(*line);
    }

    result.join("\n")
}

/// Add reviewed markers to fact lines (list items) in a section.
///
/// Stamps all list-item lines (`- ...`) with `<!-- reviewed:YYYY-MM-DD -->`.
/// Lines that already have a marker get their date updated.
pub fn stamp_reviewed_markers(section: &str, date: &NaiveDate) -> String {
    section
        .lines()
        .map(|line| {
            if FACT_LINE_REGEX.is_match(line) {
                add_or_update_reviewed_marker(line, date)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Add reviewed markers to specific 1-based line numbers in content.
pub fn stamp_reviewed_lines(content: &str, line_numbers: &[usize], date: &NaiveDate) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + 1;
            if line_numbers.contains(&line_num) && FACT_LINE_REGEX.is_match(line) {
                add_or_update_reviewed_marker(line, date)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// HTML comment marker appended to a footnote line when a weak-source question
/// is dismissed after tier-2 evaluation. Prevents the footnote from being
/// re-flagged on subsequent scans.
pub const CITATION_ACCEPTED_MARKER: &str = "<!-- ✓ -->";

/// Stamp `<!-- ✓ -->` on footnote definition lines to permanently suppress weak-source
/// question regeneration for citations that have been evaluated and accepted.
pub fn stamp_citation_accepted(content: &str, line_numbers: &[usize]) -> String {
    if line_numbers.is_empty() {
        return content.to_string();
    }
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + 1;
            if line_numbers.contains(&line_num)
                && line.trim_start().starts_with("[^")
                && !line.contains(CITATION_ACCEPTED_MARKER)
            {
                format!("{line} {CITATION_ACCEPTED_MARKER}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Stamp `<!-- sequential -->` on fact lines to permanently suppress conflict regeneration.
pub fn stamp_sequential_lines(content: &str, line_numbers: &[usize]) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line_num = i + 1;
            if line_numbers.contains(&line_num)
                && FACT_LINE_REGEX.is_match(line)
                && !line.contains("<!-- sequential")
            {
                format!("{line} <!-- sequential -->")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Stamp `<!-- sequential -->` on fact lines matching the given text snippets.
/// This is a text-based fallback for when line numbers may be stale due to
/// content changes between question generation and answer application.
pub fn stamp_sequential_by_text(content: &str, fact_texts: &[&str]) -> String {
    if fact_texts.is_empty() {
        return content.to_string();
    }
    content
        .lines()
        .map(|line| {
            if FACT_LINE_REGEX.is_match(line)
                && !line.contains("<!-- sequential")
                && fact_texts.iter().any(|t| !t.is_empty() && line.contains(t))
            {
                format!("{line} <!-- sequential -->")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Stamp `<!-- reviewed:YYYY-MM-DD -->` on fact lines matching the given text snippets.
/// Text-based fallback for when line numbers may be stale due to content changes
/// between question generation and answer application.
pub fn stamp_reviewed_by_text(content: &str, fact_texts: &[&str], date: &NaiveDate) -> String {
    if fact_texts.is_empty() {
        return content.to_string();
    }
    content
        .lines()
        .map(|line| {
            if FACT_LINE_REGEX.is_match(line)
                && !REVIEWED_MARKER_REGEX.is_match(line)
                && fact_texts.iter().any(|t| !t.is_empty() && line.contains(t))
            {
                add_or_update_reviewed_marker(line, date)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove duplicate `# Title` headings that can result from LLM rewrites.
/// Keeps only the first `# ` heading encountered.
pub fn dedup_titles(content: &str) -> String {
    let mut seen_title = false;
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            if line.starts_with("# ") && !line.starts_with("## ") {
                if seen_title {
                    return false; // drop duplicate
                }
                seen_title = true;
            }
            true
        })
        .collect();
    lines.join("\n")
}

/// Remove processed questions from Review Queue
pub fn remove_processed_questions(content: &str, processed_indices: &[usize]) -> String {
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = remove_processed_questions_inner(&unwrapped, processed_indices);
    if was_callout && result != unwrapped {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn remove_processed_questions_inner(content: &str, processed_indices: &[usize]) -> String {
    let Some(marker_pos) = content.find(REVIEW_QUEUE_MARKER) else {
        return content.to_string();
    };

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[REVIEW_QUEUE_MARKER.len()..];

    let mut result_lines: Vec<&str> = Vec::with_capacity(queue_content.lines().count());
    let mut current_question_idx = 0;
    let mut skip_answer = false;

    for line in queue_content.lines() {
        let is_question = line.trim_start().starts_with("- [");

        if is_question {
            if processed_indices.contains(&current_question_idx) {
                skip_answer = true;
            } else {
                result_lines.push(line);
                skip_answer = false;
            }
            current_question_idx += 1;
        } else if skip_answer && line.trim_start().starts_with('>') {
            // Skip answer line for processed question
            continue;
        } else {
            result_lines.push(line);
            skip_answer = false;
        }
    }

    // Check if queue is now empty
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
        // Strip trailing ## Review Queue heading, --- separator, and whitespace
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

/// Uncheck deferred questions in the Review Queue (convert `[x]` → `[ ]`).
///
/// Deferred questions stay in the queue but become unanswered again,
/// so they surface as pending items for future review.
pub fn uncheck_deferred_questions(content: &str, deferred_indices: &[usize]) -> String {
    if deferred_indices.is_empty() {
        return content.to_string();
    }
    let (unwrapped, was_callout) = unwrap_review_callout(content);
    let result = uncheck_deferred_questions_inner(&unwrapped, deferred_indices);
    if was_callout && result != unwrapped {
        wrap_review_callout(&result)
    } else {
        result
    }
}

fn uncheck_deferred_questions_inner(content: &str, deferred_indices: &[usize]) -> String {
    let Some(marker_pos) = content.find(REVIEW_QUEUE_MARKER) else {
        return content.to_string();
    };

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[REVIEW_QUEUE_MARKER.len()..];

    let mut result_lines: Vec<String> = Vec::with_capacity(queue_content.lines().count());
    let mut current_question_idx = 0;
    let mut skip_answer = false;

    for line in queue_content.lines() {
        let is_question = line.trim_start().starts_with("- [");

        if is_question {
            if deferred_indices.contains(&current_question_idx) {
                // Uncheck: replace `- [x]` with `- [ ]`
                result_lines.push(line.replacen("- [x]", "- [ ]", 1));
                skip_answer = true;
            } else {
                result_lines.push(line.to_string());
                skip_answer = false;
            }
            current_question_idx += 1;
        } else if skip_answer && line.trim_start().starts_with('>') {
            // Remove the answer line for deferred questions
            continue;
        } else {
            result_lines.push(line.to_string());
            skip_answer = false;
        }
    }

    format!(
        "{}{}\n{}",
        before_marker,
        REVIEW_QUEUE_MARKER,
        result_lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuestionType;

    fn make_question(line_ref: Option<usize>) -> ReviewQuestion {
        ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref,
            description: "test".to_string(),
            answered: true,
            answer: None,
            line_number: 10,
            confidence: None,
            confidence_reason: None,
            agent_reasoning: None,
        }
    }

    fn make_answer(instruction: ChangeInstruction) -> InterpretedAnswer {
        InterpretedAnswer {
            question: make_question(Some(5)),
            instruction,
        }
    }

    // ==================== identify_affected_section tests ====================

    #[test]
    fn test_identify_affected_section() {
        let content = "# Title\n\n## Career\n- Job 1\n- Job 2\n\n## Education\n- School";
        let questions = vec![make_question(Some(4))];

        let result = identify_affected_section(content, &questions);
        assert!(result.is_some());
        let (start, end, section) = result.unwrap();
        assert_eq!(start, 3);
        assert_eq!(end, 6);
        assert!(section.contains("## Career"));
        assert!(section.contains("Job 1"));
    }

    #[test]
    fn test_identify_affected_section_no_line_refs() {
        let content = "# Title\n\nSome content";
        let questions = vec![make_question(None)];

        let result = identify_affected_section(content, &questions);
        assert!(result.is_some());
        let (start, end, section) = result.unwrap();
        assert_eq!(start, 1);
        assert_eq!(end, 3);
        assert_eq!(section, content);
    }

    #[test]
    fn test_identify_affected_section_empty_content() {
        let content = "";
        let questions = vec![make_question(Some(1))];

        let result = identify_affected_section(content, &questions);
        assert!(result.is_none());
    }

    // ==================== replace_section tests ====================

    #[test]
    fn test_replace_section() {
        assert_eq!(
            replace_section(
                "Line 1\nLine 2\nLine 3\nLine 4",
                2,
                3,
                "New Line 2\nNew Line 3"
            ),
            "Line 1\nNew Line 2\nNew Line 3\nLine 4"
        );
        assert_eq!(
            replace_section("Line 1\nLine 2\nLine 3", 1, 1, "New Line 1"),
            "New Line 1\nLine 2\nLine 3"
        );
        assert_eq!(
            replace_section("Line 1\nLine 2\nLine 3", 3, 3, "New Line 3"),
            "Line 1\nLine 2\nNew Line 3"
        );
        assert_eq!(
            replace_section("Line 1\nLine 2", 1, 2, "Completely new"),
            "Completely new"
        );
    }

    // ==================== remove_processed_questions tests ====================

    #[test]
    fn test_remove_processed_questions_single() {
        let content = r#"# Doc

<!-- factbase:review -->
- [ ] `@q[temporal]` Question 1
- [x] `@q[temporal]` Question 2
> Answer 2
- [ ] `@q[missing]` Question 3"#;

        let result = remove_processed_questions(content, &[1]);
        assert!(result.contains("Question 1"));
        assert!(!result.contains("Question 2"));
        assert!(!result.contains("Answer 2"));
        assert!(result.contains("Question 3"));
    }

    #[test]
    fn test_remove_processed_questions_all() {
        let content = r#"# Doc

Content here

<!-- factbase:review -->
- [x] `@q[temporal]` Question 1
> Answer 1"#;

        let result = remove_processed_questions(content, &[0]);
        assert!(!result.contains("factbase:review"));
        assert!(!result.contains("Question 1"));
        assert!(result.contains("Content here"));
    }

    #[test]
    fn test_remove_processed_questions_multiple_non_contiguous() {
        let content = r#"# Doc

<!-- factbase:review -->
- [x] `@q[temporal]` Question 0
> Answer 0
- [ ] `@q[temporal]` Question 1
- [x] `@q[missing]` Question 2
> Answer 2
- [ ] `@q[stale]` Question 3"#;

        let result = remove_processed_questions(content, &[0, 2]);
        assert!(!result.contains("Question 0"));
        assert!(result.contains("Question 1"));
        assert!(!result.contains("Question 2"));
        assert!(result.contains("Question 3"));
    }

    #[test]
    fn test_remove_processed_questions_no_marker() {
        let content = "# Doc\n\nNo review queue here";
        let result = remove_processed_questions(content, &[0]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_remove_processed_questions_strips_heading_and_separator() {
        let content = "# Doc\n\nContent\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Question\n> Answer\n";
        let result = remove_processed_questions(content, &[0]);
        assert!(!result.contains("Review Queue"));
        assert!(!result.contains("---"));
        assert!(result.contains("Content"));
    }

    // ==================== stamp_reviewed_markers tests ====================

    #[test]
    fn test_stamp_reviewed_markers_stamps_list_items() {
        let section = "## Career\n- Job 1 @t[2020..]\n- Job 2 @t[2022..]\nSome text";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_markers(section, &date);
        assert!(result.contains("- Job 1 @t[2020..] <!-- reviewed:2026-02-15 -->"));
        assert!(result.contains("- Job 2 @t[2022..] <!-- reviewed:2026-02-15 -->"));
        assert!(result.contains("## Career"));
        assert!(result.contains("Some text"));
        // Non-list lines should not get markers
        assert!(!result.contains("## Career <!-- reviewed"));
        assert!(!result.contains("Some text <!-- reviewed"));
        // Asterisk facts also stamped
        let section2 = "* Fact A\n* Fact B\nNot a fact";
        let result2 = stamp_reviewed_markers(section2, &date);
        assert!(result2.contains("* Fact A <!-- reviewed:2026-02-15 -->"));
        assert!(result2.contains("* Fact B <!-- reviewed:2026-02-15 -->"));
        assert!(!result2.contains("Not a fact <!-- reviewed"));
    }

    #[test]
    fn test_stamp_reviewed_markers_updates_existing() {
        let section = "- Fact one <!-- reviewed:2020-01-01 -->\n- Fact two";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_markers(section, &date);
        assert!(result.contains("- Fact one <!-- reviewed:2026-02-15 -->"));
        assert!(result.contains("- Fact two <!-- reviewed:2026-02-15 -->"));
        // Should not have double markers
        assert_eq!(result.matches("reviewed:").count(), 2);
    }

    #[test]
    fn test_stamp_reviewed_markers_empty_section() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_markers("", &date);
        assert_eq!(result, "");
    }

    // ==================== stamp_reviewed_lines tests ====================

    #[test]
    fn test_stamp_reviewed_lines_specific_lines() {
        let content = "# Title\n\n## Career\n- Job 1\n- Job 2\n- Job 3";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_lines(content, &[4], &date);
        assert!(result.contains("- Job 1 <!-- reviewed:2026-02-15 -->"));
        assert!(!result.contains("- Job 2 <!-- reviewed"));
        assert!(!result.contains("- Job 3 <!-- reviewed"));
    }

    #[test]
    fn test_stamp_reviewed_lines_skips_non_list_items() {
        let content = "# Title\n\n## Career\n- Job 1";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        // Line 1 is "# Title" - not a list item, should not be stamped
        let result = stamp_reviewed_lines(content, &[1], &date);
        assert!(!result.contains("<!-- reviewed"));
    }

    // ==================== stamp_reviewed_lines with non-dash facts ====================

    #[test]
    fn test_stamp_reviewed_lines_alternate_markers() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        // Asterisk facts
        let content = "# Title\n\n* Fact one\n* Fact two";
        let result = stamp_reviewed_lines(content, &[3], &date);
        assert!(result.contains("* Fact one <!-- reviewed:2026-02-15 -->"));
        assert!(!result.contains("* Fact two <!-- reviewed"));
        // Numbered facts
        let content2 = "# Title\n\n1. First fact\n2. Second fact";
        let result2 = stamp_reviewed_lines(content2, &[3, 4], &date);
        assert!(result2.contains("1. First fact <!-- reviewed:2026-02-15 -->"));
        assert!(result2.contains("2. Second fact <!-- reviewed:2026-02-15 -->"));
    }

    // ==================== stamp_reviewed_by_text tests ====================

    #[test]
    fn test_stamp_reviewed_by_text_matches_fact() {
        let content =
            "# Title\n\n- VP of Engineering @t[2020..]\n- Director of Sales @t[2018..2020]";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_by_text(content, &["VP of Engineering"], &date);
        assert!(result.contains("VP of Engineering @t[2020..] <!-- reviewed:2026-02-15 -->"));
        assert!(!result.contains("Director of Sales <!-- reviewed"));
    }

    #[test]
    fn test_stamp_reviewed_by_text_skips_already_reviewed() {
        let content = "- Fact one <!-- reviewed:2025-01-01 -->\n- Fact two";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_by_text(content, &["Fact one", "Fact two"], &date);
        // Already-reviewed line should keep its existing marker
        assert!(result.contains("<!-- reviewed:2025-01-01 -->"));
        assert!(result.contains("Fact two <!-- reviewed:2026-02-15 -->"));
    }

    #[test]
    fn test_stamp_reviewed_by_text_empty_texts() {
        let content = "- Fact one\n- Fact two";
        let date = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let result = stamp_reviewed_by_text(content, &[], &date);
        assert_eq!(result, content);
    }

    // ==================== uncheck_deferred_questions tests ====================

    #[test]
    fn test_uncheck_deferred() {
        // Single deferred
        let c1 = "# Doc\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Question 0\n> defer";
        let r1 = uncheck_deferred_questions(c1, &[0]);
        assert!(r1.contains("- [ ] `@q[temporal]` Question 0") && !r1.contains("> defer"));

        // Preserves others
        let c2 = "# Doc\n\n<!-- factbase:review -->\n- [x] `@q[temporal]` Question 0\n> dismiss\n- [x] `@q[stale]` Question 1\n> defer\n- [ ] `@q[missing]` Question 2";
        let r2 = uncheck_deferred_questions(c2, &[1]);
        assert!(r2.contains("- [x] `@q[temporal]` Question 0"));
        assert!(r2.contains("- [ ] `@q[stale]` Question 1"));
        assert!(!r2.contains("> defer"));

        // Empty indices / no marker
        assert_eq!(
            uncheck_deferred_questions(
                "# Doc\n\n<!-- factbase:review -->\n- [x] Q0\n> answer",
                &[]
            ),
            "# Doc\n\n<!-- factbase:review -->\n- [x] Q0\n> answer"
        );
        assert_eq!(
            uncheck_deferred_questions("# Doc\n\nNo review queue", &[0]),
            "# Doc\n\nNo review queue"
        );
    }

    // ==================== apply_source_citations tests ====================

    #[test]
    fn test_source_citation_new_footnote_no_existing() {
        let content = "# Doc\n\n## Career\n- VP at Acme @t[2020..]";
        let result = apply_source_citations(content, &[("VP at Acme", "LinkedIn, 2026-02-15")]);
        assert!(result.contains("- VP at Acme @t[2020..] [^1]"));
        assert!(result.contains("[^1]: LinkedIn, 2026-02-15"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_source_citation_appends_after_existing() {
        let content = "# Doc\n\n- Fact one [^1]\n\n---\n[^1]: Original source";
        let result = apply_source_citations(content, &[("Fact one", "New source, 2026-01")]);
        assert!(result.contains("- Fact one [^1] [^2]"));
        assert!(result.contains("[^1]: Original source"));
        assert!(result.contains("[^2]: New source, 2026-01"));
    }

    #[test]
    fn test_source_citation_multiple() {
        let content = "# Doc\n\n- Fact A\n- Fact B";
        let result =
            apply_source_citations(content, &[("Fact A", "Source A"), ("Fact B", "Source B")]);
        assert!(result.contains("- Fact A [^1]"));
        assert!(result.contains("- Fact B [^2]"));
        assert!(result.contains("[^1]: Source A"));
        assert!(result.contains("[^2]: Source B"));
    }

    #[test]
    fn test_source_citation_before_reviewed_marker() {
        let content = "- Fact here <!-- reviewed:2026-01-01 -->";
        let result = apply_source_citations(content, &[("Fact here", "Source info")]);
        assert!(result.contains("[^1] <!-- reviewed:2026-01-01 -->"));
        assert!(result.contains("- Fact here [^1]"));
    }

    #[test]
    fn test_source_citation_before_review_queue() {
        let content = "# Doc\n\n- Fact one\n\n<!-- factbase:review -->\n- [ ] Question";
        let result = apply_source_citations(content, &[("Fact one", "Source")]);
        // Footnotes should appear before the review queue marker
        let footnote_pos = result.find("[^1]: Source").unwrap();
        let review_pos = result.find("<!-- factbase:review -->").unwrap();
        assert!(footnote_pos < review_pos);
    }

    #[test]
    fn test_source_citation_empty_sources() {
        let content = "# Doc\n\n- Fact";
        let result = apply_source_citations(content, &[]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_source_citation_line_not_found() {
        let content = "# Doc\n\n- Fact A";
        let result = apply_source_citations(content, &[("Nonexistent line", "Source")]);
        assert_eq!(result, content);
    }

    // =========================================================================
    // apply_confirmations tests
    // =========================================================================

    #[test]
    fn test_confirmation_update_existing_tag() {
        let content = "- VP at BigCo @t[~2024-01]";
        let result = apply_confirmations(
            content,
            &[("VP at BigCo", Some("@t[~2024-01]"), "@t[~2026-02-15]")],
        );
        assert_eq!(result, "- VP at BigCo @t[~2026-02-15]");
    }

    #[test]
    fn test_confirmation_add_tag_no_existing() {
        let content = "- VP at BigCo";
        let result = apply_confirmations(content, &[("VP at BigCo", None, "@t[~2026-02-15]")]);
        assert_eq!(result, "- VP at BigCo @t[~2026-02-15]");
    }

    #[test]
    fn test_confirmation_add_tag_before_footnote() {
        let content = "- VP at BigCo [^1]";
        let result = apply_confirmations(content, &[("VP at BigCo", None, "@t[~2026-02-15]")]);
        assert_eq!(result, "- VP at BigCo @t[~2026-02-15] [^1]");
    }

    #[test]
    fn test_confirmation_add_tag_before_reviewed_marker() {
        let content = "- VP at BigCo <!-- reviewed:2025-01-01 -->";
        let result = apply_confirmations(content, &[("VP at BigCo", None, "@t[~2026-02-15]")]);
        assert!(result.contains("@t[~2026-02-15] <!-- reviewed:2025-01-01 -->"));
    }

    #[test]
    fn test_confirmation_empty_updates() {
        let content = "- Some fact";
        let result = apply_confirmations(content, &[]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_confirmation_line_not_found() {
        let content = "- Fact A";
        let result =
            apply_confirmations(content, &[("Nonexistent", Some("@t[~2024]"), "@t[~2026]")]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_confirmation_multiple_updates() {
        let content = "- Fact A @t[~2024-01]\n- Fact B";
        let result = apply_confirmations(
            content,
            &[
                ("Fact A", Some("@t[~2024-01]"), "@t[~2026-02]"),
                ("Fact B", None, "@t[~2026-02]"),
            ],
        );
        assert!(result.contains("- Fact A @t[~2026-02]"));
        assert!(result.contains("- Fact B @t[~2026-02]"));
    }

    // --- stamp_sequential_lines tests ---

    #[test]
    fn test_stamp_sequential_lines_basic() {
        let content = "# Person\n\n- VP at Acme @t[2020..2023]\n- Director at Acme @t[2018..2020]";
        let result = stamp_sequential_lines(content, &[3, 4]);
        assert!(result.contains("<!-- sequential -->"));
        assert!(result
            .lines()
            .nth(2)
            .unwrap()
            .contains("<!-- sequential -->"));
        assert!(result
            .lines()
            .nth(3)
            .unwrap()
            .contains("<!-- sequential -->"));
        // Asterisk facts also stamped
        let content2 = "# Title\n\n* Fact one @t[2020..2022]\n* Fact two @t[2022..]";
        let result2 = stamp_sequential_lines(content2, &[3]);
        assert!(result2.contains("* Fact one @t[2020..2022] <!-- sequential -->"));
        assert!(!result2.contains("* Fact two @t[2022..] <!-- sequential"));
    }

    #[test]
    fn test_stamp_sequential_lines_no_double_stamp() {
        let content = "- VP at Acme @t[2020..2023] <!-- sequential -->";
        let result = stamp_sequential_lines(content, &[1]);
        assert_eq!(result.matches("<!-- sequential").count(), 1);
    }

    #[test]
    fn test_stamp_sequential_lines_skips_non_fact_lines() {
        let content = "# Title\n\n- Fact line";
        let result = stamp_sequential_lines(content, &[1]);
        assert!(!result.lines().next().unwrap().contains("<!-- sequential"));
    }

    // --- stamp_sequential_by_text tests ---

    #[test]
    fn test_stamp_sequential_by_text_basic() {
        let content = "# Person\n\n- VP at Acme @t[2020..2023]\n- Director at Acme @t[2018..2020]";
        let result = stamp_sequential_by_text(content, &["VP at Acme", "Director at Acme"]);
        assert!(result
            .lines()
            .nth(2)
            .unwrap()
            .contains("<!-- sequential -->"));
        assert!(result
            .lines()
            .nth(3)
            .unwrap()
            .contains("<!-- sequential -->"));
        // Asterisk facts also stamped
        let content2 = "* Fact A @t[2020..2022]\n* Fact B @t[2022..]";
        let result2 = stamp_sequential_by_text(content2, &["Fact A"]);
        assert!(result2.contains("* Fact A @t[2020..2022] <!-- sequential -->"));
        assert!(!result2.contains("* Fact B @t[2022..] <!-- sequential"));
    }

    #[test]
    fn test_stamp_sequential_by_text_no_double_stamp() {
        let content = "- VP at Acme @t[2020..2023] <!-- sequential -->";
        let result = stamp_sequential_by_text(content, &["VP at Acme"]);
        assert_eq!(result.matches("<!-- sequential").count(), 1);
    }

    #[test]
    fn test_stamp_sequential_by_text_skips_non_fact_lines() {
        let content = "# VP at Acme\n\n- Other fact";
        let result = stamp_sequential_by_text(content, &["VP at Acme"]);
        assert!(!result.lines().next().unwrap().contains("<!-- sequential"));
    }

    #[test]
    fn test_stamp_sequential_by_text_empty_texts() {
        let content = "- Fact line";
        let result = stamp_sequential_by_text(content, &[]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_stamp_sequential_by_text_ignores_empty_strings() {
        let content = "- Fact line";
        let result = stamp_sequential_by_text(content, &[""]);
        assert!(!result.contains("<!-- sequential"));
    }

    // --- dedup_titles tests ---

    #[test]
    fn test_stamp_citation_accepted_stamps_footnote_line() {
        let content = "# Doc\n\n- Fact [^1]\n\n---\n[^1]: Phonetool lookup, 2026-02-10";
        let result = stamp_citation_accepted(content, &[6]);
        assert!(result.contains("[^1]: Phonetool lookup, 2026-02-10 <!-- ✓ -->"));
    }

    #[test]
    fn test_stamp_citation_accepted_no_double_stamp() {
        let content = "[^1]: Phonetool lookup <!-- ✓ -->";
        let result = stamp_citation_accepted(content, &[1]);
        assert_eq!(result.matches("<!-- ✓ -->").count(), 1);
    }

    #[test]
    fn test_stamp_citation_accepted_skips_non_footnote_lines() {
        let content = "# Title\n\n- Fact line";
        let result = stamp_citation_accepted(content, &[1, 3]);
        assert!(!result.contains("<!-- ✓ -->"));
    }

    #[test]
    fn test_stamp_citation_accepted_empty_line_numbers() {
        let content = "[^1]: Some source";
        let result = stamp_citation_accepted(content, &[]);
        assert_eq!(result, content);
    }

    // --- dedup_titles tests ---

    #[test]
    fn test_dedup_titles_removes_duplicate() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n# Title\n\n- Fact";
        let result = dedup_titles(content);
        assert_eq!(result.matches("# Title").count(), 1);
        assert!(result.contains("- Fact"));
    }

    #[test]
    fn test_dedup_titles_preserves_single() {
        let content = "---\nfactbase_id: abc123\n---\n# Title\n\n- Fact";
        let result = dedup_titles(content);
        assert_eq!(result, content);
    }

    #[test]
    fn test_dedup_titles_preserves_h2_headings() {
        let content = "# Title\n\n## Section A\n## Section B";
        let result = dedup_titles(content);
        assert_eq!(result, content);
    }

    #[test]
    fn test_dedup_titles_no_title() {
        let content = "---\nfactbase_id: abc123\n---\n\n- Fact";
        let result = dedup_titles(content);
        assert_eq!(result, content);
    }

    // --- identify_affected_section excludes header/title ---

    #[test]
    fn test_identify_section_excludes_header_and_title() {
        let content =
            "---\nfactbase_id: abc123\n---\n# My Document\n\n- Fact on line 4\n- Fact on line 5";
        let questions = vec![make_question(Some(4))];
        let result = identify_affected_section(content, &questions);
        assert!(result.is_some());
        let (start, _end, section) = result.unwrap();
        // Section should NOT include the factbase header or title
        assert!(
            !section.contains("<!-- factbase:"),
            "Section should not contain factbase header"
        );
        assert!(
            !section.contains("# My Document"),
            "Section should not contain title"
        );
        assert!(section.contains("Fact on line 4"));
        assert!(
            start >= 3,
            "Start should be after header+title, got {start}"
        );
    }

    // --- apply_changes_to_section tests ---

    #[tokio::test]
    async fn test_apply_changes_to_section_all_dismissed() {
        let section = "- Fact 1\n- Fact 2";
        let instructions = vec![make_answer(ChangeInstruction::Dismiss)];
        let result = apply_changes_to_section(section, &instructions)
            .await
            .unwrap();
        assert_eq!(result, section);
    }

    #[tokio::test]
    async fn test_apply_changes_to_section_delete() {
        let section = "- Fact 1\n- Fact 2\n- Fact 3";
        let instructions = vec![make_answer(ChangeInstruction::Delete {
            line_text: "Fact 2".to_string(),
        })];
        let result = apply_changes_to_section(section, &instructions)
            .await
            .unwrap();
        assert!(result.contains("Fact 1"));
        assert!(!result.contains("Fact 2"));
        assert!(result.contains("Fact 3"));
    }

    #[tokio::test]
    async fn test_apply_changes_to_section_split_does_not_error() {
        let section = "- Fact 1\n- Combined fact\n- Fact 3";
        let instructions = vec![make_answer(ChangeInstruction::Split {
            line_text: "Combined fact".to_string(),
            instruction: "separate into two".to_string(),
        })];
        // Should succeed (skip the split) instead of erroring
        let result = apply_changes_to_section(section, &instructions)
            .await
            .unwrap();
        assert_eq!(result, section);
    }

    #[tokio::test]
    async fn test_apply_changes_to_section_generic_does_not_error() {
        let section = "- Fact 1\n- Fact 2";
        let instructions = vec![make_answer(ChangeInstruction::Generic {
            description: "some complex change".to_string(),
        })];
        // Should succeed (skip the generic) instead of erroring
        let result = apply_changes_to_section(section, &instructions)
            .await
            .unwrap();
        assert_eq!(result, section);
    }

    #[tokio::test]
    async fn test_apply_changes_to_section_mixed_delete_and_split() {
        let section = "- Fact 1\n- Delete me\n- Split me\n- Fact 4";
        let instructions = vec![
            make_answer(ChangeInstruction::Delete {
                line_text: "Delete me".to_string(),
            }),
            make_answer(ChangeInstruction::Split {
                line_text: "Split me".to_string(),
                instruction: "separate".to_string(),
            }),
        ];
        // Should apply the delete and skip the split
        let result = apply_changes_to_section(section, &instructions)
            .await
            .unwrap();
        assert!(!result.contains("Delete me"));
        assert!(result.contains("Split me")); // split skipped, line preserved
        assert!(result.contains("Fact 1"));
        assert!(result.contains("Fact 4"));
    }

    // ==================== callout format tests ====================

    #[test]
    fn test_remove_processed_questions_callout() {
        let content = "# Doc\n\nContent.\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [x] `@q[temporal]` Q0\n>   > answer\n> - [ ] `@q[stale]` Q1\n>   > \n";
        let result = remove_processed_questions(content, &[0]);
        assert!(
            result.contains("> [!review]- Review Queue"),
            "should preserve callout format"
        );
        assert!(!result.contains("Q0"), "should remove processed question");
        assert!(result.contains("Q1"), "should keep unprocessed question");
    }

    #[test]
    fn test_remove_processed_questions_callout_all_removed() {
        let content = "# Doc\n\nContent.\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [x] `@q[temporal]` Q0\n>   > answer\n";
        let result = remove_processed_questions(content, &[0]);
        assert!(
            !result.contains("Review Queue"),
            "should remove entire section"
        );
        assert!(result.contains("Content."));
    }

    #[test]
    fn test_uncheck_deferred_questions_callout() {
        let content = "# Doc\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [x] `@q[temporal]` Q0\n>   > defer\n> - [x] `@q[stale]` Q1\n>   > dismiss\n";
        let result = uncheck_deferred_questions(content, &[0]);
        assert!(
            result.contains("> [!review]- Review Queue"),
            "should preserve callout format"
        );
        // Q0 should be unchecked and answer removed
        assert!(result.contains("Q0"));
        assert!(!result.contains("defer"));
        // Q1 should be unchanged
        assert!(result.contains("- [x]") || result.contains("Q1"));
    }

    #[test]
    fn test_apply_source_citations_callout() {
        let content = "# Doc\n\n- Fact one\n\n> [!review]- Review Queue\n> <!-- factbase:review -->\n> - [ ] `@q[missing]` Q\n>   > \n";
        let result = apply_source_citations(content, &[("Fact one", "Source info")]);
        assert!(result.contains("[^1]: Source info"));
        // Footnotes must appear before the callout review section
        let footnote_pos = result.find("[^1]: Source info").unwrap();
        let review_pos = result.find("> [!review]- Review Queue").unwrap();
        assert!(
            footnote_pos < review_pos,
            "footnotes must be before callout review section"
        );
    }

    #[test]
    fn test_remove_processed_questions_callout_roundtrip() {
        // Start with callout (new format, no marker inside) → remove one question → verify still callout
        let content = "# Doc\n\nContent.\n\n> [!review]- Review Queue\n> - [x] `@q[temporal]` Q0\n>   > answer\n> - [ ] `@q[stale]` Q1\n>   > \n";
        let result = remove_processed_questions(content, &[0]);
        assert!(result.contains("> [!review]- Review Queue"));
        assert!(
            !result.contains("> <!-- factbase:review -->"),
            "New format should not have marker inside callout"
        );
        assert!(result.contains("> - [ ] `@q[stale]` Q1"));
    }
}
