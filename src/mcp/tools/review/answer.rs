//! Review question answering MCP tools.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::helpers::resolve_doc_path;
use crate::mcp::tools::{get_str_arg, get_str_arg_required, get_u64_arg_required};
use crate::processor::{content_hash, is_callout_review, parse_review_queue, unwrap_review_callout, wrap_review_callout};
use crate::ProgressReporter;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::instrument;

/// Resolve confidence from args: "believed" answers are stored as deferred.
/// Returns (is_defer, answer_text) after applying confidence logic.
fn resolve_confidence(answer: &str, confidence: Option<&str>) -> Result<(bool, String), FactbaseError> {
    let lower = answer.to_lowercase();
    let explicit_defer = lower.starts_with("defer:");

    if explicit_defer {
        let note = answer["defer:".len()..].trim();
        if note.is_empty() {
            return Err(FactbaseError::parse(
                "defer: requires a note explaining why (e.g., 'defer: no matching records found')",
            ));
        }
        return Ok((true, note.to_string()));
    }

    match confidence {
        Some("believed") => Ok((true, format!("believed: {answer}"))),
        _ => Ok((false, answer.to_string())),
    }
}

/// Counts review queue questions into unanswered/deferred/believed buckets.
/// Matches the classification logic used by get_review_queue.
fn count_queue_questions(
    questions: &[crate::models::ReviewQuestion],
    unanswered: &mut usize,
    deferred: &mut usize,
    believed: &mut usize,
) {
    for q in questions {
        if q.answered {
            // skip — already applied
        } else if q.is_deferred() {
            if q.is_believed() {
                *believed += 1;
            }
            *deferred += 1;
        } else {
            *unanswered += 1;
        }
    }
}

pub(crate) fn modify_question_in_queue(
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
    let confidence = get_str_arg(args, "confidence");

    // Validate answer is not empty
    let answer = answer.trim();
    if answer.is_empty() {
        return Err(FactbaseError::parse("answer cannot be empty"));
    }

    // Resolve confidence: believed → defer, defer: prefix → defer, else → answer
    let (defer, answer_text) = resolve_confidence(answer, confidence)?;

    // Get the document (for file_path metadata)
    let doc = db.require_document(&doc_id)?;

    // Resolve absolute path via repo root so we read/write the same file
    // that the agent will later update via update_document.
    let file_path = resolve_doc_path(db, &doc)?;
    if !file_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "File not found: {}",
            file_path.display()
        )));
    }
    let mut content = fs::read_to_string(&file_path)?;

    // Recover review section from DB if disk is missing marker or questions
    let marker = "<!-- factbase:review -->";
    let (recovered, changed) =
        crate::processor::recover_review_section(&content, &doc.content);
    if changed {
        content = recovered;
        fs::write(&file_path, &content)?;
    }

    // Parse the review queue
    let questions = parse_review_queue(&content).ok_or_else(|| {
        FactbaseError::not_found(format!(
            "No review queue in document {doc_id} — it may have been cleaned up or not yet generated. Run check_repository to regenerate."
        ))
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

    // Idempotent: if already answered, skip silently
    if question.answered {
        let type_str = question.question_type.as_str();
        return Ok(serde_json::json!({
            "success": true,
            "doc_id": doc_id,
            "question_index": question_index,
            "question_type": type_str,
            "description": question.description,
            "skipped": true,
            "message": "Question already answered — skipped."
        }));
    }

    // Unwrap callout format so modify_question_in_queue sees plain lines
    let was_callout = is_callout_review(&content);
    if was_callout {
        let (unwrapped, _) = unwrap_review_callout(&content);
        content = unwrapped;
    }

    // Find and modify the question in the document content
    let marker_pos = content
        .find(marker)
        .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;

    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[marker.len()..];

    // Modify the question using the extracted helper
    let modified_queue =
        modify_question_in_queue(queue_content, question_index, &answer_text, defer)
            .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;

    // Reconstruct the document, re-wrapping callout if needed
    let mut new_content = format!("{before_marker}{marker}{modified_queue}");
    if was_callout {
        new_content = wrap_review_callout(&new_content);
    }

    fs::write(&file_path, &new_content)?;

    // Sync updated content back to database so subsequent queries see the answer
    let new_hash = content_hash(&new_content);
    db.update_document_content(&doc_id, &new_content, &new_hash)?;

    let type_str = question.question_type.as_str();

    if defer {
        let believed = answer_text.starts_with("believed: ");
        let message = if believed {
            "Answer recorded as 'believed' (unverified). It stays in the review queue for human confirmation."
        } else {
            "Question deferred with note. It remains in the review queue for future resolution."
        };
        Ok(serde_json::json!({
            "success": true,
            "doc_id": doc_id,
            "question_index": question_index,
            "question_type": type_str,
            "description": question.description,
            "deferred": true,
            "believed": believed,
            "note": answer_text,
            "message": message
        }))
    } else {
        Ok(serde_json::json!({
            "success": true,
            "doc_id": doc_id,
            "question_index": question_index,
            "question_type": type_str,
            "description": question.description,
            "answer": answer_text,
            "message": "Question answered. Use update_document to apply changes to the document."
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
/// JSON with `success`, `answered` count, `skipped` count, `results` array, and `message`.
///
/// # Errors
/// - `FactbaseError::NotFound` if any document doesn't exist
/// - `FactbaseError::Parse` if any question index is invalid
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
            "skipped": 0,
            "message": "No answers to process"
        }));
    }

    // Parse and validate all answers first
    let mut parsed_answers: Vec<(String, usize, String, Option<String>)> = Vec::new();
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

        let confidence = answer.get("confidence").and_then(|v| v.as_str()).map(String::from);

        parsed_answers.push((doc_id.to_string(), question_index, answer_text.to_string(), confidence));
    }

    // Group answers by document for efficient processing
    let mut by_doc: HashMap<String, Vec<(usize, String, Option<String>)>> = HashMap::new();
    for (doc_id, question_index, answer_text, confidence) in parsed_answers {
        by_doc
            .entry(doc_id)
            .or_default()
            .push((question_index, answer_text, confidence));
    }

    // Validate all documents and questions exist before making any changes.
    // Resolve absolute paths via repo root so we read/write the same files
    // that the agent will later update via update_document.
    let mut doc_disk_content: HashMap<String, (PathBuf, String)> = HashMap::new();
    for (doc_id, answers_for_doc) in &by_doc {
        let doc = db.require_document(doc_id)?;
        let file_path = resolve_doc_path(db, &doc)?;
        if !file_path.exists() {
            return Err(FactbaseError::not_found(format!(
                "File not found: {}",
                file_path.display()
            )));
        }
        let mut disk_content = fs::read_to_string(&file_path)?;

        // Recover review section from DB if disk is missing marker or questions
        let (recovered, changed) =
            crate::processor::recover_review_section(&disk_content, &doc.content);
        if changed {
            disk_content = recovered;
            fs::write(&file_path, &disk_content)?;
        }

        let questions = parse_review_queue(&disk_content).ok_or_else(|| {
            FactbaseError::not_found(format!(
                "No review queue in document {doc_id} — it may have been cleaned up or not yet generated. Run check_repository to regenerate."
            ))
        })?;

        // Validate question indices; skip already-answered (idempotent)
        for (question_index, _, _) in answers_for_doc {
            if *question_index >= questions.len() {
                return Err(FactbaseError::parse(format!(
                    "Invalid question_index {} for document {}. Document has {} questions.",
                    question_index,
                    doc_id,
                    questions.len()
                )));
            }
        }

        doc_disk_content.insert(doc_id.clone(), (file_path, disk_content));
    }

    // Phase 1: Compute all new file contents in memory (no side effects)
    let mut pending_writes: Vec<(String, PathBuf, String)> = Vec::new(); // (doc_id, path, new_content)
    let mut results: Vec<Value> = Vec::new();
    let mut skipped = 0usize;
    let total_docs = by_doc.len();
    for (i, (doc_id, answers_for_doc)) in by_doc.iter().enumerate() {
        let (file_path, disk_content) = doc_disk_content
            .get(doc_id)
            .ok_or_else(|| FactbaseError::internal(format!("missing disk content for {doc_id}")))?;

        progress.report(
            i + 1,
            total_docs,
            &format!(
                "Answering {} question(s) in {}",
                answers_for_doc.len(),
                doc_id
            ),
        );

        // Determine which questions are already answered so we can skip them
        let questions = parse_review_queue(disk_content).unwrap_or_default();
        let mut actionable: Vec<(usize, String, Option<String>)> = Vec::new();
        for (question_index, answer_text, confidence) in answers_for_doc {
            if *question_index < questions.len() && questions[*question_index].answered {
                skipped += 1;
                results.push(serde_json::json!({
                    "doc_id": doc_id,
                    "question_index": question_index,
                    "skipped": true
                }));
            } else {
                actionable.push((*question_index, answer_text.clone(), confidence.clone()));
            }
        }

        if actionable.is_empty() {
            continue;
        }

        // Sort answers by question index in descending order to avoid index shifting
        actionable.sort_by(|a, b| b.0.cmp(&a.0));

        let mut content = disk_content.clone();
        let marker = "<!-- factbase:review -->";

        // Unwrap callout format so modify_question_in_queue sees plain lines
        let was_callout = is_callout_review(&content);
        if was_callout {
            let (unwrapped, _) = unwrap_review_callout(&content);
            content = unwrapped;
        }

        for (question_index, answer_text, confidence) in &actionable {
            let marker_pos = content
                .find(marker)
                .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;

            let (before_marker, after_marker) = content.split_at(marker_pos);
            let queue_content = &after_marker[marker.len()..];

            let (defer, text) = resolve_confidence(answer_text, confidence.as_deref())?;

            let modified_queue =
                modify_question_in_queue(queue_content, *question_index, &text, defer)
                    .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;

            content = format!("{before_marker}{marker}{modified_queue}");
        }

        // Re-wrap callout if it was originally in callout format
        if was_callout {
            content = wrap_review_callout(&content);
        }

        pending_writes.push((doc_id.clone(), file_path.clone(), content));

        for (question_index, answer_text, _) in answers_for_doc {
            if *question_index < questions.len() && questions[*question_index].answered {
                continue; // already counted as skipped above
            }
            results.push(serde_json::json!({
                "doc_id": doc_id,
                "question_index": question_index,
                "answer": answer_text
            }));
        }
    }

    // Phase 2: Write all files to disk (filesystem is source of truth)
    for (_, file_path, content) in &pending_writes {
        fs::write(file_path, content)?;
    }

    // Phase 3: Update all DB records in a single transaction
    db.with_transaction(|conn| {
        for (doc_id, _, content) in &pending_writes {
            let new_hash = content_hash(content);
            db.update_document_content_on_conn(conn, doc_id, content, &new_hash)?;
        }
        Ok(())
    })?;

    // Count remaining unanswered questions across all docs.
    // Use pending_writes content for docs modified in this batch (guaranteed fresh),
    // fall back to DB content for the rest.
    let mut remaining_unanswered = 0usize;
    let mut total_deferred = 0usize;
    let mut total_believed = 0usize;
    let written_ids: std::collections::HashSet<&str> =
        pending_writes.iter().map(|(id, _, _)| id.as_str()).collect();
    // Count from freshly-written content
    for (_, _, content) in &pending_writes {
        if let Some(questions) = parse_review_queue(content) {
            count_queue_questions(&questions, &mut remaining_unanswered, &mut total_deferred, &mut total_believed);
        }
    }
    // Count from DB for docs not in this batch
    let docs_with_queues = db.get_documents_with_review_queue(None).unwrap_or_default();
    for doc in &docs_with_queues {
        if written_ids.contains(doc.id.as_str()) {
            continue; // already counted from pending_writes
        }
        if let Some(questions) = parse_review_queue(&doc.content) {
            count_queue_questions(&questions, &mut remaining_unanswered, &mut total_deferred, &mut total_believed);
        }
    }

    let answered = results.iter().filter(|r| r.get("skipped").is_none()).count();
    Ok(serde_json::json!({
        "success": true,
        "answered": answered,
        "skipped": skipped,
        "results": results,
        "remaining_unanswered": remaining_unanswered,
        "remaining_deferred": total_deferred,
        "remaining_believed": total_believed,
        "message": format!("Answered {} question(s), skipped {} already-answered. {} unanswered remain. Run `factbase review --apply` to process.", answered, skipped, remaining_unanswered)
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

    /// Roundtrip test: modify two questions, then parse — both should be recognized as answered.
    /// Reproduces the bug where review apply finds 0 answered questions after
    /// answer_questions succeeds.
    #[test]
    fn test_modify_then_parse_roundtrip_two_answers() {
        use crate::processor::parse_review_queue;

        // Simulate a document with a review queue (exact format from append_review_questions)
        let original_content = "\
<!-- factbase:abc123 -->\n\
# Test Person\n\
\n\
- CEO at Acme Corp\n\
- CTO at Acme Corp\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[conflict]` Line 4: CEO vs CTO — which role is current?\n\
  > \n\
- [ ] `@q[conflict]` Line 5: Overlapping roles at same company\n\
  > \n";

        let marker = "<!-- factbase:review -->";

        // Simulate bulk_answer_questions: process in descending index order
        let mut content = original_content.to_string();
        for &(idx, answer) in &[(1usize, "CTO is current, CEO was 2018-2020"), (0, "CEO ended 2020, CTO started 2020")] {
            let marker_pos = content.find(marker).unwrap();
            let (before_marker, after_marker) = content.split_at(marker_pos);
            let queue_content = &after_marker[marker.len()..];
            let modified_queue = modify_question_in_queue(queue_content, idx, answer, false)
                .expect("modify should succeed");
            content = format!("{before_marker}{marker}{modified_queue}");
        }

        // Now parse the modified content — both questions should be answered
        let questions = parse_review_queue(&content)
            .expect("should have review queue");
        assert_eq!(questions.len(), 2, "should have 2 questions");
        assert!(questions[0].answered, "question 0 should be answered");
        assert!(questions[0].answer.is_some(), "question 0 should have answer text");
        assert!(questions[1].answered, "question 1 should be answered");
        assert!(questions[1].answer.is_some(), "question 1 should have answer text");
    }

    /// Verify resolve_doc_path joins repo root with relative file_path.
    #[test]
    fn test_resolve_doc_path_uses_repo_root() {
        use crate::mcp::tools::helpers::resolve_doc_path;

        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(repo_dir.join("people")).unwrap();
        let doc_file = repo_dir.join("people/test.md");
        std::fs::write(&doc_file, "# Test").unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            repo_id: "r1".into(),
            file_path: "people/test.md".into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let abs = resolve_doc_path(&db, &doc).unwrap();
        assert_eq!(abs, doc_file);
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

    #[test]
    fn test_resolve_confidence_verified_default() {
        let (defer, text) = resolve_confidence("@t[2020] per Wikipedia", None).unwrap();
        assert!(!defer);
        assert_eq!(text, "@t[2020] per Wikipedia");
    }

    #[test]
    fn test_resolve_confidence_verified_explicit() {
        let (defer, text) = resolve_confidence("@t[2020] per Wikipedia", Some("verified")).unwrap();
        assert!(!defer);
        assert_eq!(text, "@t[2020] per Wikipedia");
    }

    #[test]
    fn test_resolve_confidence_believed_becomes_defer() {
        let (defer, text) = resolve_confidence("Still accurate based on training data", Some("believed")).unwrap();
        assert!(defer);
        assert_eq!(text, "believed: Still accurate based on training data");
    }

    #[test]
    fn test_resolve_confidence_defer_prefix_takes_precedence() {
        let (defer, text) = resolve_confidence("defer: searched web, no results", Some("verified")).unwrap();
        assert!(defer);
        assert_eq!(text, "searched web, no results");
    }

    #[test]
    fn test_resolve_confidence_defer_empty_note_errors() {
        let result = resolve_confidence("defer:", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_believed_answer_stays_in_queue() {
        // A believed answer should be stored as deferred (unchecked) so it
        // won't be picked up by review apply.
        use crate::processor::parse_review_queue;

        let original_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[stale]` Line 4: Source is 200 days old\n\
  > \n";

        let marker = "<!-- factbase:review -->";
        let marker_pos = original_content.find(marker).unwrap();
        let (before_marker, after_marker) = original_content.split_at(marker_pos);
        let queue_content = &after_marker[marker.len()..];

        // Simulate believed confidence: resolve_confidence returns defer=true
        let (defer, text) = resolve_confidence("Still accurate from training data", Some("believed")).unwrap();
        assert!(defer);

        let modified_queue = modify_question_in_queue(queue_content, 0, &text, defer).unwrap();
        let content = format!("{before_marker}{marker}{modified_queue}");

        // Parse: question should be deferred (unchecked with answer), NOT answered
        let questions = parse_review_queue(&content).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(!questions[0].answered, "believed answer must NOT be marked as answered");
        assert!(questions[0].is_deferred(), "believed answer should be deferred");
        assert!(questions[0].answer.as_ref().unwrap().contains("believed:"));
    }

    #[test]
    fn test_verified_answer_gets_applied() {
        // A verified answer should be marked as answered (checked) so
        // review apply will process it.
        use crate::processor::parse_review_queue;

        let original_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[stale]` Line 4: Source is 200 days old\n\
  > \n";

        let marker = "<!-- factbase:review -->";
        let marker_pos = original_content.find(marker).unwrap();
        let (before_marker, after_marker) = original_content.split_at(marker_pos);
        let queue_content = &after_marker[marker.len()..];

        // Simulate verified confidence (default)
        let (defer, text) = resolve_confidence("@t[2020] per Wikipedia (https://example.com)", None).unwrap();
        assert!(!defer);

        let modified_queue = modify_question_in_queue(queue_content, 0, &text, defer).unwrap();
        let content = format!("{before_marker}{marker}{modified_queue}");

        let questions = parse_review_queue(&content).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(questions[0].answered, "verified answer must be marked as answered");
        assert!(questions[0].answer.is_some());
    }

    #[test]
    fn test_defer_stored_with_reasoning() {
        use crate::processor::parse_review_queue;

        let original_content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
---\n\
\n\
## Review Queue\n\
\n\
<!-- factbase:review -->\n\
- [ ] `@q[stale]` Line 4: Source is 200 days old\n\
  > \n";

        let marker = "<!-- factbase:review -->";
        let marker_pos = original_content.find(marker).unwrap();
        let (before_marker, after_marker) = original_content.split_at(marker_pos);
        let queue_content = &after_marker[marker.len()..];

        let (defer, text) = resolve_confidence("defer: searched web for 'entity fact 2026', no confirming results", None).unwrap();
        assert!(defer);

        let modified_queue = modify_question_in_queue(queue_content, 0, &text, defer).unwrap();
        let content = format!("{before_marker}{marker}{modified_queue}");

        let questions = parse_review_queue(&content).unwrap();
        assert_eq!(questions.len(), 1);
        assert!(!questions[0].answered, "deferred question must not be answered");
        assert!(questions[0].is_deferred());
        assert!(questions[0].answer.as_ref().unwrap().contains("searched web"));
    }

    #[test]
    fn test_answer_question_recovers_missing_marker() {
        // Disk file has no review marker, but DB content does.
        // answer_question should recover the review section from DB.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");

        // Disk file: no review marker
        let disk_content = "<!-- factbase:abc123 -->\n# Test Entity\n\n- Some fact\n";
        std::fs::write(&doc_file, disk_content).unwrap();

        // DB content: has review marker with a question
        let db_content = "<!-- factbase:abc123 -->\n# Test Entity\n\n- Some fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Line 4: Source is old\n  > \n";

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Test Entity".into(),
            content: db_content.into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "doc_id": "abc123",
            "question_index": 0,
            "answer": "@t[~2024] confirmed"
        });
        let result = answer_question(&db, &args).unwrap();
        assert_eq!(result["success"], true);

        // Verify the disk file now has the marker
        let updated = std::fs::read_to_string(&doc_file).unwrap();
        assert!(updated.contains("<!-- factbase:review -->"));
    }

    #[test]
    fn test_answer_question_idempotent_already_answered() {
        // Answering an already-answered question should return success with skipped=true
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");

        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[stale]` Line 4: Source is old\n  > Already answered\n";
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Test".into(),
            content: content.into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "doc_id": "abc123",
            "question_index": 0,
            "answer": "New answer"
        });
        let result = answer_question(&db, &args).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["skipped"], true);
    }

    #[test]
    fn test_bulk_answer_skips_already_answered() {
        // Bulk answering should skip already-answered questions instead of erroring
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");

        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[stale]` Line 4: Already done\n  > Previous answer\n- [ ] `@q[temporal]` Line 4: When?\n  > \n";
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Test".into(),
            content: content.into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "answers": [
                {"doc_id": "abc123", "question_index": 0, "answer": "Retry answer"},
                {"doc_id": "abc123", "question_index": 1, "answer": "@t[2020]"}
            ]
        });
        let result = bulk_answer_questions(&db, &args, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["answered"], 1);
        assert_eq!(result["skipped"], 1);
    }

    #[test]
    fn test_bulk_answer_atomic_db_commit() {
        // All DB updates should happen in a single transaction.
        // Verify by checking that after a successful bulk answer,
        // all documents are updated in the DB.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let content_a = "<!-- factbase:aaa111 -->\n# Doc A\n\n- Fact A\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 4: When?\n  > \n";
        let content_b = "<!-- factbase:bbb222 -->\n# Doc B\n\n- Fact B\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` Line 4: Source?\n  > \n";
        std::fs::write(repo_dir.join("a.md"), content_a).unwrap();
        std::fs::write(repo_dir.join("b.md"), content_b).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        for (id, path, content) in [("aaa111", "a.md", content_a), ("bbb222", "b.md", content_b)] {
            let doc = crate::models::Document {
                id: id.into(),
                repo_id: "r1".into(),
                file_path: path.into(),
                title: format!("Doc {id}"),
                content: content.into(),
                ..crate::models::Document::test_default()
            };
            db.upsert_document(&doc).unwrap();
        }

        let args = serde_json::json!({
            "answers": [
                {"doc_id": "aaa111", "question_index": 0, "answer": "@t[2020]"},
                {"doc_id": "bbb222", "question_index": 0, "answer": "LinkedIn"}
            ]
        });
        let result = bulk_answer_questions(&db, &args, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["answered"], 2);

        // Both documents should be updated in DB
        let doc_a = db.get_document("aaa111").unwrap().unwrap();
        assert!(doc_a.content.contains("[x]"), "Doc A should have answered question in DB");
        let doc_b = db.get_document("bbb222").unwrap().unwrap();
        assert!(doc_b.content.contains("[x]"), "Doc B should have answered question in DB");
    }

    #[test]
    fn test_answer_question_callout_format() {
        // When the review section uses callout format (> prefixed lines),
        // answer_question should still work correctly.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");

        // Callout format: review section lines prefixed with `> `
        let content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
> [!info]- Review Queue\n\
> <!-- factbase:review -->\n\
> - [ ] `@q[temporal]` Line 4: When was this true?\n\
>   > \n\
> - [ ] `@q[missing]` Line 4: What is the source?\n\
>   > \n";
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Test Entity".into(),
            content: content.into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "doc_id": "abc123",
            "question_index": 0,
            "answer": "@t[=2024-01] confirmed"
        });
        let result = answer_question(&db, &args).unwrap();
        assert_eq!(result["success"], true);
        assert!(result.get("skipped").is_none(), "should not be skipped");

        // Verify the disk file is still in callout format with the answer applied
        let updated = std::fs::read_to_string(&doc_file).unwrap();
        assert!(updated.contains("> <!-- factbase:review -->"), "should preserve callout format");
        // The answered question should be parseable
        let questions = parse_review_queue(&updated).unwrap();
        assert_eq!(questions.len(), 2);
        assert!(questions[0].answered, "question 0 should be answered");
        assert!(!questions[1].answered, "question 1 should remain unanswered");
    }

    #[test]
    fn test_bulk_answer_callout_format() {
        // Bulk answering should work with callout-format review sections.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");

        let content = "\
<!-- factbase:abc123 -->\n\
# Test Entity\n\
\n\
- Some fact\n\
\n\
> [!info]- Review Queue\n\
> <!-- factbase:review -->\n\
> - [ ] `@q[temporal]` Line 4: When?\n\
>   > \n\
> - [ ] `@q[missing]` Line 4: Source?\n\
>   > \n";
        std::fs::write(&doc_file, content).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Test Entity".into(),
            content: content.into(),
            ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "answers": [
                {"doc_id": "abc123", "question_index": 0, "answer": "@t[2020..2022]"},
                {"doc_id": "abc123", "question_index": 1, "answer": "LinkedIn profile"}
            ]
        });
        let result = bulk_answer_questions(&db, &args, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["answered"], 2);

        // Verify callout format preserved and both questions answered
        let updated = std::fs::read_to_string(&doc_file).unwrap();
        assert!(updated.contains("> <!-- factbase:review -->"), "should preserve callout format");
        let questions = parse_review_queue(&updated).unwrap();
        assert_eq!(questions.len(), 2);
        assert!(questions[0].answered, "question 0 should be answered");
        assert!(questions[1].answered, "question 1 should be answered");
    }

    #[test]
    fn test_bulk_answer_believed_excluded_from_remaining_unanswered() {
        // Believed answers should NOT be counted as unanswered in remaining_unanswered.
        // They should appear in remaining_deferred and remaining_believed instead.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Two docs, each with one question
        let content_a = "<!-- factbase:aaa111 -->\n# Doc A\n\n- Fact A\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 4: When?\n";
        let content_b = "<!-- factbase:bbb222 -->\n# Doc B\n\n- Fact B\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` Line 4: Source?\n";
        std::fs::write(repo_dir.join("a.md"), content_a).unwrap();
        std::fs::write(repo_dir.join("b.md"), content_b).unwrap();

        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir.clone(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        for (id, path, content) in [("aaa111", "a.md", content_a), ("bbb222", "b.md", content_b)] {
            let doc = crate::models::Document {
                id: id.into(),
                repo_id: "r1".into(),
                file_path: path.into(),
                title: format!("Doc {id}"),
                content: content.into(),
                ..crate::models::Document::test_default()
            };
            db.upsert_document(&doc).unwrap();
        }

        // Answer doc A with believed confidence — should be deferred, not unanswered
        let args = serde_json::json!({
            "answers": [
                {"doc_id": "aaa111", "question_index": 0, "answer": "Around 2020", "confidence": "believed"}
            ]
        });
        let result = bulk_answer_questions(&db, &args, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["answered"], 1);
        // Doc B still has 1 unanswered; Doc A is now believed/deferred
        assert_eq!(result["remaining_unanswered"], 1, "believed should not count as unanswered");
        assert_eq!(result["remaining_deferred"], 1, "believed should count as deferred");
        assert_eq!(result["remaining_believed"], 1, "believed should be tracked separately");
    }
}
