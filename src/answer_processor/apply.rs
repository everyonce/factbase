//! Apply changes to documents based on interpreted answers.

use chrono::NaiveDate;

use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::patterns::{
    add_or_update_reviewed_marker, REVIEWED_MARKER_REGEX, REVIEW_QUEUE_MARKER, SOURCE_DEF_REGEX,
};
use crate::ReviewQuestion;

use super::{ChangeInstruction, InterpretedAnswer};

/// Format change instructions for LLM prompt
pub fn format_changes_for_llm(instructions: &[InterpretedAnswer]) -> String {
    let mut changes = Vec::with_capacity(instructions.len());

    for (i, ia) in instructions.iter().enumerate() {
        let change_text = match &ia.instruction {
            ChangeInstruction::Dismiss | ChangeInstruction::Defer => continue,
            ChangeInstruction::Delete { line_text } => {
                format!("Delete line containing \"{line_text}\"")
            }
            ChangeInstruction::UpdateTemporal {
                line_text,
                old_tag,
                new_tag,
            } => {
                format!("Line containing \"{line_text}\": change {old_tag} to {new_tag}")
            }
            ChangeInstruction::Split {
                line_text,
                instruction,
            } => {
                format!("Split line containing \"{line_text}\" into separate facts: {instruction}")
            }
            ChangeInstruction::AddTemporal { line_text, tag } => {
                format!("Line containing \"{line_text}\": add {tag} at end")
            }
            ChangeInstruction::AddSource {
                line_text,
                source_info,
            } => {
                format!("Line containing \"{line_text}\": add source reference for {source_info}")
            }
            ChangeInstruction::Generic { description } => description.clone(),
        };

        changes.push(format!("{}. {}", i + 1, change_text));
    }

    changes.join("\n")
}

/// Build the LLM prompt for section rewriting
pub fn build_rewrite_prompt(section: &str, changes: &str) -> String {
    format!(
        r#"Rewrite this section with the exact changes specified.

ORIGINAL:
{section}

CHANGES:
{changes}

RULES:
1. Apply ALL changes exactly as specified
2. Keep all other lines unchanged
3. Preserve existing source references [^N] unless change says to remove
4. Use ONLY these date formats in @t[] tags:
   - Year only: @t[2022]
   - Year-month: @t[2022-03] (NOT "Mar 2022")
   - Range: @t[2020..2022-02]
5. If change says "delete line", remove that line entirely
6. If change says "split into", create separate list items

Output the complete rewritten section only:"#
    )
}

/// Apply changes to a document section using LLM
pub async fn apply_changes_to_section(
    llm: &dyn LlmProvider,
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

    // Build and send prompt to LLM
    let changes = format_changes_for_llm(instructions);
    let prompt = build_rewrite_prompt(section, &changes);

    let response = llm.complete(&prompt).await?;
    let rewritten = response.trim().to_string();

    // Validate response
    if rewritten.is_empty() {
        return Err(FactbaseError::ollama(
            "LLM returned empty response".to_string(),
        ));
    }

    Ok(rewritten)
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
    let mut existing_defs: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
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
        // No existing footnotes — insert before review queue or at end
        let insert_idx = lines
            .iter()
            .position(|l| l.contains(REVIEW_QUEUE_MARKER))
            .unwrap_or(lines.len());
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

    let mut start = 1;
    let mut end = lines.len();

    // Find section start (look backwards for ## heading)
    for i in (0..min_line.saturating_sub(1).min(lines.len())).rev() {
        if lines[i].starts_with("## ") {
            start = i + 1;
            break;
        }
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
            if line.trim_start().starts_with("- ") {
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
            if line_numbers.contains(&line_num) && line.trim_start().starts_with("- ") {
                add_or_update_reviewed_marker(line, date)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove processed questions from Review Queue
pub fn remove_processed_questions(content: &str, processed_indices: &[usize]) -> String {
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
        }
    }

    fn make_answer(instruction: ChangeInstruction) -> InterpretedAnswer {
        InterpretedAnswer {
            question: make_question(Some(5)),
            instruction,
        }
    }

    // ==================== format_changes_for_llm tests ====================

    #[test]
    fn test_format_changes_for_llm() {
        let instructions = vec![
            InterpretedAnswer {
                question: make_question(Some(5)),
                instruction: ChangeInstruction::UpdateTemporal {
                    line_text: "VP at BigCo".to_string(),
                    old_tag: "@t[2022..]".to_string(),
                    new_tag: "@t[2022..2024-03]".to_string(),
                },
            },
            InterpretedAnswer {
                question: make_question(Some(6)),
                instruction: ChangeInstruction::Dismiss,
            },
        ];

        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("VP at BigCo"));
        assert!(result.contains("@t[2022..]"));
        assert!(result.contains("@t[2022..2024-03]"));
        // Dismiss should be skipped
        assert!(!result.contains("Dismiss"));
    }

    #[test]
    fn test_format_changes_for_llm_empty() {
        let instructions: Vec<InterpretedAnswer> = vec![];
        let result = format_changes_for_llm(&instructions);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_changes_for_llm_all_dismiss() {
        let instructions = vec![
            make_answer(ChangeInstruction::Dismiss),
            make_answer(ChangeInstruction::Dismiss),
        ];
        let result = format_changes_for_llm(&instructions);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_changes_for_llm_delete() {
        let instructions = vec![make_answer(ChangeInstruction::Delete {
            line_text: "Old fact to remove".to_string(),
        })];
        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("Delete line"));
        assert!(result.contains("Old fact to remove"));
    }

    #[test]
    fn test_format_changes_for_llm_split() {
        let instructions = vec![make_answer(ChangeInstruction::Split {
            line_text: "Combined fact".to_string(),
            instruction: "separate into two items".to_string(),
        })];
        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("Split line"));
        assert!(result.contains("Combined fact"));
        assert!(result.contains("separate into two items"));
    }

    #[test]
    fn test_format_changes_for_llm_add_temporal() {
        let instructions = vec![make_answer(ChangeInstruction::AddTemporal {
            line_text: "Fact without date".to_string(),
            tag: "@t[2023]".to_string(),
        })];
        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("add @t[2023] at end"));
    }

    #[test]
    fn test_format_changes_for_llm_add_source() {
        let instructions = vec![make_answer(ChangeInstruction::AddSource {
            line_text: "Unsourced fact".to_string(),
            source_info: "LinkedIn profile".to_string(),
        })];
        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("add source reference"));
        assert!(result.contains("LinkedIn profile"));
    }

    #[test]
    fn test_format_changes_for_llm_generic() {
        let instructions = vec![make_answer(ChangeInstruction::Generic {
            description: "Custom change instruction".to_string(),
        })];
        let result = format_changes_for_llm(&instructions);
        assert!(result.contains("Custom change instruction"));
    }

    // ==================== build_rewrite_prompt tests ====================

    #[test]
    fn test_build_rewrite_prompt_structure() {
        let section = "## Career\n- Job 1\n- Job 2";
        let changes = "1. Add @t[2020] to Job 1";
        let result = build_rewrite_prompt(section, changes);

        assert!(result.contains("ORIGINAL:"));
        assert!(result.contains("## Career"));
        assert!(result.contains("CHANGES:"));
        assert!(result.contains("Add @t[2020]"));
        assert!(result.contains("RULES:"));
    }

    #[test]
    fn test_build_rewrite_prompt_contains_rules() {
        let result = build_rewrite_prompt("content", "changes");
        assert!(result.contains("Apply ALL changes"));
        assert!(result.contains("@t[2022]"));
        assert!(result.contains("@t[2022-03]"));
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
        let content = "Line 1\nLine 2\nLine 3\nLine 4";
        let new_section = "New Line 2\nNew Line 3";
        let result = replace_section(content, 2, 3, new_section);
        assert_eq!(result, "Line 1\nNew Line 2\nNew Line 3\nLine 4");
    }

    #[test]
    fn test_replace_section_at_start() {
        let content = "Line 1\nLine 2\nLine 3";
        let new_section = "New Line 1";
        let result = replace_section(content, 1, 1, new_section);
        assert_eq!(result, "New Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_replace_section_at_end() {
        let content = "Line 1\nLine 2\nLine 3";
        let new_section = "New Line 3";
        let result = replace_section(content, 3, 3, new_section);
        assert_eq!(result, "Line 1\nLine 2\nNew Line 3");
    }

    #[test]
    fn test_replace_section_entire_content() {
        let content = "Line 1\nLine 2";
        let new_section = "Completely new";
        let result = replace_section(content, 1, 2, new_section);
        assert_eq!(result, "Completely new");
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

    // ==================== uncheck_deferred_questions tests ====================

    #[test]
    fn test_uncheck_deferred_single() {
        let content = r#"# Doc

<!-- factbase:review -->
- [x] `@q[temporal]` Question 0
> defer"#;
        let result = uncheck_deferred_questions(content, &[0]);
        assert!(result.contains("- [ ] `@q[temporal]` Question 0"));
        assert!(!result.contains("> defer"));
    }

    #[test]
    fn test_uncheck_deferred_preserves_others() {
        let content = r#"# Doc

<!-- factbase:review -->
- [x] `@q[temporal]` Question 0
> dismiss
- [x] `@q[stale]` Question 1
> defer
- [ ] `@q[missing]` Question 2"#;
        let result = uncheck_deferred_questions(content, &[1]);
        // Question 0 unchanged (still checked)
        assert!(result.contains("- [x] `@q[temporal]` Question 0"));
        assert!(result.contains("> dismiss"));
        // Question 1 unchecked, answer removed
        assert!(result.contains("- [ ] `@q[stale]` Question 1"));
        assert!(!result.contains("> defer"));
        // Question 2 unchanged
        assert!(result.contains("- [ ] `@q[missing]` Question 2"));
    }

    #[test]
    fn test_uncheck_deferred_empty_indices() {
        let content = "# Doc\n\n<!-- factbase:review -->\n- [x] Q0\n> answer";
        let result = uncheck_deferred_questions(content, &[]);
        assert_eq!(result, content);
    }

    #[test]
    fn test_uncheck_deferred_no_marker() {
        let content = "# Doc\n\nNo review queue";
        let result = uncheck_deferred_questions(content, &[0]);
        assert_eq!(result, content);
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
}
