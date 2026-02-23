//! Apply changes to documents based on interpreted answers.

use crate::error::FactbaseError;
use crate::llm::LlmProvider;
use crate::patterns::REVIEW_QUEUE_MARKER;
use crate::ReviewQuestion;

use super::{ChangeInstruction, InterpretedAnswer};

/// Format change instructions for LLM prompt
pub fn format_changes_for_llm(instructions: &[InterpretedAnswer]) -> String {
    let mut changes = Vec::with_capacity(instructions.len());

    for (i, ia) in instructions.iter().enumerate() {
        let change_text = match &ia.instruction {
            ChangeInstruction::Dismiss => continue,
            ChangeInstruction::Delete { line_text } => {
                format!("Delete line containing \"{}\"", line_text)
            }
            ChangeInstruction::UpdateTemporal {
                line_text,
                old_tag,
                new_tag,
            } => {
                format!(
                    "Line containing \"{}\": change {} to {}",
                    line_text, old_tag, new_tag
                )
            }
            ChangeInstruction::Split {
                line_text,
                instruction,
            } => {
                format!(
                    "Split line containing \"{}\" into separate facts: {}",
                    line_text, instruction
                )
            }
            ChangeInstruction::AddTemporal { line_text, tag } => {
                format!("Line containing \"{}\": add {} at end", line_text, tag)
            }
            ChangeInstruction::AddSource {
                line_text,
                source_info,
            } => {
                format!(
                    "Line containing \"{}\": add source reference for {}",
                    line_text, source_info
                )
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
{}

CHANGES:
{}

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

Output the complete rewritten section only:"#,
        section, changes
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
        .filter(|ia| !matches!(ia.instruction, ChangeInstruction::Dismiss))
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
    for i in (0..min_line.saturating_sub(1)).rev() {
        if lines[i].starts_with("## ") {
            start = i + 1;
            break;
        }
    }

    // Find section end (look forwards for ## heading)
    for (i, line) in lines.iter().enumerate().skip(max_line) {
        if line.starts_with("## ") {
            end = i;
            break;
        }
    }

    let section_lines: Vec<&str> = lines[start.saturating_sub(1)..end].to_vec();
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
        // Remove entire Review Queue section
        before_marker.trim_end().to_string()
    }
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
}
