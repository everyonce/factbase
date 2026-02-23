//! Review question answering MCP tools.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_str_arg_required, get_u64_arg_required};
use crate::processor::{content_hash, parse_review_queue};
use crate::ProgressReporter;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::instrument;

/// Modifies a question in the review queue content.
///
/// When `defer` is false, marks the question as answered by changing `[ ]` to `[x]`.
/// When `defer` is true, keeps `[ ]` unchecked but writes the note as a blockquote,
/// so the question remains unanswered but carries context for the next reviewer.
fn modify_question_in_queue(
    queue_content: &str,
    question_index: usize,
    answer: &str,
    defer: bool,
) -> Option<String> {
    let mut new_queue_lines: Vec<String> = Vec::new();
    let mut current_question_idx = 0;
    let mut lines = queue_content.lines().peekable();
    let mut modified = false;

    while let Some(line) = lines.next() {
        // Check if this is a question line
        if line.trim().starts_with("- [") && line.contains("`@q[") {
            if current_question_idx == question_index {
                if defer {
                    // Keep checkbox unchecked for deferred questions
                    new_queue_lines.push(line.to_string());
                } else {
                    let modified_line = line.replacen("- [ ]", "- [x]", 1);
                    new_queue_lines.push(modified_line);
                }

                // Skip any existing empty lines or blockquotes after this question
                while let Some(&next) = lines.peek() {
                    let trimmed = next.trim();
                    if trimmed.is_empty() || trimmed.starts_with('>') {
                        lines.next();
                    } else {
                        break;
                    }
                }

                // Add the answer/note as a blockquote
                new_queue_lines.push(format!("> {answer}"));
                modified = true;
            } else {
                new_queue_lines.push(line.to_string());
            }
            current_question_idx += 1;
        } else {
            new_queue_lines.push(line.to_string());
        }
    }

    if modified {
        Some(new_queue_lines.join("\n"))
    } else {
        None
    }
}

/// Marks a review question as answered.
///
/// Modifies the document file to check the question checkbox and add the answer
/// as a blockquote. Run `factbase review --apply` to process answered questions.
///
/// # Arguments (from JSON)
/// - `doc_id` (required): Document ID (6-char hex)
/// - `question_index` (required): Zero-based index of question in review queue
/// - `answer` (required): Answer text (cannot be empty)
///
/// # Returns
/// JSON with `success`, `doc_id`, `question_index`, `question_type`,
/// `description`, `answer`, and `message`.
///
/// # Errors
/// - `FactbaseError::NotFound` if document or review queue doesn't exist
/// - `FactbaseError::Parse` if question already answered or index invalid
#[instrument(name = "mcp_answer_question", skip(db, args))]
pub fn answer_question(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_id = get_str_arg_required(args, "doc_id")?;
    let question_index = get_u64_arg_required(args, "question_index")? as usize;
    let answer = get_str_arg_required(args, "answer")?;

    // Validate answer is not empty
    let answer = answer.trim();
    if answer.is_empty() {
        return Err(FactbaseError::parse("answer cannot be empty"));
    }

    // Detect defer prefix
    let (defer, answer_text) = if let Some(rest) = answer
        .strip_prefix("defer:")
        .or_else(|| answer.strip_prefix("DEFER:"))
    {
        let note = rest.trim();
        if note.is_empty() {
            return Err(FactbaseError::parse(
                "defer: requires a note explaining why (e.g., 'defer: no matching records in Salesforce')",
            ));
        }
        (true, note)
    } else {
        (false, answer)
    };

    // Get the document
    let doc = db.require_document(&doc_id)?;

    // Parse the review queue
    let questions = parse_review_queue(&doc.content).ok_or_else(|| {
        FactbaseError::not_found(format!("No Review Queue found in document: {doc_id}"))
    })?;

    // Validate question index
    if question_index >= questions.len() {
        return Err(FactbaseError::parse(format!(
            "Invalid question_index: {}. Document has {} questions.",
            question_index,
            questions.len()
        )));
    }

    let question = &questions[question_index];

    // Check if already answered (deferred questions can be re-answered or re-deferred)
    if question.answered {
        return Err(FactbaseError::parse(format!(
            "Question {question_index} is already answered"
        )));
    }

    // Find and modify the question in the document content
    let content = &doc.content;
    let marker = "<!-- factbase:review -->";
    let marker_pos = content
        .find(marker)
        .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[marker.len()..];

    // Modify the question using the extracted helper
    let modified_queue =
        modify_question_in_queue(queue_content, question_index, answer_text, defer)
            .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;

    // Reconstruct the document
    let new_content = format!("{before_marker}{marker}{modified_queue}");

    // Write to file
    let file_path = PathBuf::from(&doc.file_path);
    if !file_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "File not found: {}",
            file_path.display()
        )));
    }
    fs::write(&file_path, &new_content)?;

    // Sync updated content back to database so subsequent queries see the answer
    let new_hash = content_hash(&new_content);
    db.update_document_content(&doc_id, &new_content, &new_hash)?;

    let type_str = question.question_type.as_str();

    if defer {
        Ok(serde_json::json!({
            "success": true,
            "doc_id": doc_id,
            "question_index": question_index,
            "question_type": type_str,
            "description": question.description,
            "deferred": true,
            "note": answer_text,
            "message": "Question deferred with note. It remains in the review queue for future resolution."
        }))
    } else {
        Ok(serde_json::json!({
            "success": true,
            "doc_id": doc_id,
            "question_index": question_index,
            "question_type": type_str,
            "description": question.description,
            "answer": answer_text,
            "message": "Question answered. Call apply_review_answers to apply changes to the document."
        }))
    }
}

/// Answers multiple review questions atomically.
///
/// Validates all answers first, then applies them. If any validation fails,
/// no changes are made (all-or-nothing semantics).
///
/// # Arguments (from JSON)
/// - `answers` (required): Array of objects with `doc_id`, `question_index`, `answer`
///
/// # Limits
/// - Maximum 50 answers per call
///
/// # Returns
/// JSON with `success`, `answered` count, `results` array, and `message`.
///
/// # Errors
/// - `FactbaseError::NotFound` if any document doesn't exist
/// - `FactbaseError::Parse` if any question already answered or index invalid
#[instrument(name = "mcp_bulk_answer_questions", skip(db, args, progress))]
pub fn bulk_answer_questions(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let answers = args
        .get("answers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| FactbaseError::parse("answers array is required"))?;

    // Limit to 50 answers per call
    if answers.len() > 50 {
        return Err(FactbaseError::parse("Maximum 50 answers per call"));
    }

    if answers.is_empty() {
        return Ok(serde_json::json!({
            "success": true,
            "answered": 0,
            "message": "No answers to process"
        }));
    }

    // Parse and validate all answers first
    let mut parsed_answers: Vec<(String, usize, String)> = Vec::new();
    for (i, answer) in answers.iter().enumerate() {
        let doc_id = answer
            .get("doc_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse(format!("answers[{i}]: doc_id is required")))?;
        let question_index = answer
            .get("question_index")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                FactbaseError::parse(format!("answers[{i}]: question_index is required"))
            })? as usize;
        let answer_text = answer
            .get("answer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse(format!("answers[{i}]: answer is required")))?
            .trim();

        if answer_text.is_empty() {
            return Err(FactbaseError::parse(format!(
                "answers[{i}]: answer cannot be empty"
            )));
        }

        parsed_answers.push((doc_id.to_string(), question_index, answer_text.to_string()));
    }

    // Group answers by document for efficient processing
    let mut by_doc: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    for (doc_id, question_index, answer_text) in parsed_answers {
        by_doc
            .entry(doc_id)
            .or_default()
            .push((question_index, answer_text));
    }

    // Validate all documents and questions exist before making any changes
    let mut doc_contents: HashMap<String, (crate::models::Document, String)> = HashMap::new();
    for (doc_id, answers_for_doc) in &by_doc {
        let doc = db.require_document(doc_id)?;

        let questions = parse_review_queue(&doc.content).ok_or_else(|| {
            FactbaseError::not_found(format!("No Review Queue found in document: {doc_id}"))
        })?;

        // Validate all question indices
        for (question_index, _) in answers_for_doc {
            if *question_index >= questions.len() {
                return Err(FactbaseError::parse(format!(
                    "Invalid question_index {} for document {}. Document has {} questions.",
                    question_index,
                    doc_id,
                    questions.len()
                )));
            }
            if questions[*question_index].answered {
                return Err(FactbaseError::parse(format!(
                    "Question {question_index} in document {doc_id} is already answered"
                )));
            }
        }

        doc_contents.insert(doc_id.clone(), (doc, questions.len().to_string()));
    }

    // Now apply all changes
    let mut results: Vec<Value> = Vec::new();
    let total_docs = by_doc.len();
    for (i, (doc_id, answers_for_doc)) in by_doc.iter().enumerate() {
        let (doc, _) = doc_contents
            .get(doc_id)
            .ok_or_else(|| FactbaseError::internal(format!("missing doc_contents for {doc_id}")))?;

        progress.report(
            i + 1,
            total_docs,
            &format!(
                "Answering {} question(s) in {}",
                answers_for_doc.len(),
                doc_id
            ),
        );

        // Sort answers by question index in descending order to avoid index shifting
        let mut sorted_answers = answers_for_doc.clone();
        sorted_answers.sort_by(|a, b| b.0.cmp(&a.0));

        let mut content = doc.content.clone();
        let marker = "<!-- factbase:review -->";

        for (question_index, answer_text) in &sorted_answers {
            let marker_pos = content
                .find(marker)
                .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;

            let (before_marker, after_marker) = content.split_at(marker_pos);
            let queue_content = &after_marker[marker.len()..];

            let defer = answer_text.starts_with("defer:") || answer_text.starts_with("DEFER:");
            let text = if defer {
                answer_text
                    .strip_prefix("defer:")
                    .or_else(|| answer_text.strip_prefix("DEFER:"))
                    .unwrap_or(answer_text)
                    .trim()
            } else {
                answer_text
            };

            // Use the extracted helper
            let modified_queue =
                modify_question_in_queue(queue_content, *question_index, text, defer)
                    .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;

            content = format!("{before_marker}{marker}{modified_queue}");
        }

        // Write to file
        let file_path = PathBuf::from(&doc.file_path);
        if !file_path.exists() {
            return Err(FactbaseError::not_found(format!(
                "File not found: {}",
                file_path.display()
            )));
        }
        fs::write(&file_path, &content)?;

        // Sync updated content back to database
        let new_hash = content_hash(&content);
        db.update_document_content(doc_id, &content, &new_hash)?;

        for (question_index, answer_text) in answers_for_doc {
            results.push(serde_json::json!({
                "doc_id": doc_id,
                "question_index": question_index,
                "answer": answer_text
            }));
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "answered": results.len(),
        "results": results,
        "message": format!("Answered {} question(s). Run `factbase review --apply` to process.", results.len())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modify_question_marks_as_answered() {
        let queue_content = r#"
## Review Queue

- [ ] `@q[temporal]` Line 5: When was this role held?
  > 
- [ ] `@q[missing]` Line 8: What is the source?
  > 
"#;
        let result = modify_question_in_queue(queue_content, 0, "Started 2020, ended 2022", false);
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("- [x] `@q[temporal]`"));
        assert!(modified.contains("> Started 2020, ended 2022"));
        // Second question should remain unchanged
        assert!(modified.contains("- [ ] `@q[missing]`"));
    }

    #[test]
    fn test_modify_question_second_question() {
        let queue_content = r#"
## Review Queue

- [ ] `@q[temporal]` Line 5: When was this role held?
  > 
- [ ] `@q[missing]` Line 8: What is the source?
  > 
"#;
        let result = modify_question_in_queue(queue_content, 1, "LinkedIn profile", false);
        assert!(result.is_some());
        let modified = result.unwrap();
        // First question should remain unchanged
        assert!(modified.contains("- [ ] `@q[temporal]`"));
        // Second question should be answered
        assert!(modified.contains("- [x] `@q[missing]`"));
        assert!(modified.contains("> LinkedIn profile"));
    }

    #[test]
    fn test_modify_question_replaces_existing_blockquote() {
        let queue_content = r#"
## Review Queue

- [ ] `@q[temporal]` Line 5: When was this role held?
  > old placeholder text
- [ ] `@q[missing]` Line 8: What is the source?
"#;
        let result = modify_question_in_queue(queue_content, 0, "New answer", false);
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("> New answer"));
        assert!(!modified.contains("old placeholder text"));
    }

    #[test]
    fn test_modify_question_invalid_index_returns_none() {
        let queue_content = r#"
## Review Queue

- [ ] `@q[temporal]` Line 5: When was this role held?
"#;
        let result = modify_question_in_queue(queue_content, 5, "Answer", false);
        assert!(result.is_none());
    }

    #[test]
    fn test_modify_question_empty_queue_returns_none() {
        let queue_content = r#"
## Review Queue

No questions here.
"#;
        let result = modify_question_in_queue(queue_content, 0, "Answer", false);
        assert!(result.is_none());
    }

    #[test]
    fn test_modify_question_preserves_other_content() {
        let queue_content = r#"
## Review Queue

Some intro text here.

- [ ] `@q[temporal]` Line 5: When was this role held?
  > 

Some footer text.
"#;
        let result = modify_question_in_queue(queue_content, 0, "2020-2022", false);
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("Some intro text here."));
        assert!(modified.contains("Some footer text."));
        assert!(modified.contains("> 2020-2022"));
    }

    #[test]
    fn test_modify_question_defer_keeps_unchecked() {
        let queue_content = r#"
## Review Queue

- [ ] `@q[conflict]` Line 42: CEO conflict
  > 
"#;
        let result = modify_question_in_queue(
            queue_content,
            0,
            "Searched Salesforce, no matching records",
            true,
        );
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("- [ ] `@q[conflict]`"));
        assert!(modified.contains("> Searched Salesforce, no matching records"));
    }
}
