//! Review question answering service.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::processor::{
    content_hash, is_callout_review, parse_review_queue, unwrap_review_callout, wrap_review_callout,
};
use crate::ProgressReporter;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::instrument;

use super::helpers::{
    count_queue_questions, modify_question_in_queue, resolve_confidence, resolve_doc_path,
};

/// Typed parameters for answering a single question.
#[derive(Debug)]
pub struct AnswerQuestionParams {
    pub doc_id: String,
    pub question_index: usize,
    pub answer: String,
    pub confidence: Option<String>,
}

/// Single item in a bulk answer request.
#[derive(Debug)]
pub struct BulkAnswerItem {
    pub doc_id: String,
    pub question_index: usize,
    pub answer: String,
    pub confidence: Option<String>,
}

/// Marks a review question as answered.
#[instrument(name = "svc_answer_question", skip(db, params))]
pub fn answer_question(
    db: &Database,
    params: &AnswerQuestionParams,
) -> Result<Value, FactbaseError> {
    let answer = params.answer.trim();
    if answer.is_empty() {
        return Err(FactbaseError::parse("answer cannot be empty"));
    }

    let (defer, answer_text) = resolve_confidence(answer, params.confidence.as_deref())?;
    let doc = db.require_document(&params.doc_id)?;
    let file_path = resolve_doc_path(db, &doc)?;
    if !file_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "File not found: {}",
            file_path.display()
        )));
    }
    let mut content = fs::read_to_string(&file_path)?;

    // Recover review section from DB if disk is missing marker or questions
    let (recovered, changed) = crate::processor::recover_review_section(&content, &doc.content);
    if changed {
        content = recovered;
        fs::write(&file_path, &content)?;
    }

    let marker = "<!-- factbase:review -->";
    let questions = parse_review_queue(&content).ok_or_else(|| {
        FactbaseError::not_found(format!(
            "No review queue in document {} — it may have been cleaned up or not yet generated. Run check_repository to regenerate.",
            params.doc_id
        ))
    })?;

    if params.question_index >= questions.len() {
        return Err(FactbaseError::parse(format!(
            "Invalid question_index: {}. Document has {} questions.",
            params.question_index,
            questions.len()
        )));
    }

    let question = &questions[params.question_index];
    if question.answered {
        let type_str = question.question_type.as_str();
        return Ok(serde_json::json!({
            "success": true, "doc_id": params.doc_id, "question_index": params.question_index,
            "question_type": type_str, "description": question.description,
            "skipped": true, "message": "Question already answered — skipped."
        }));
    }

    let was_callout = is_callout_review(&content);
    if was_callout {
        let (unwrapped, _) = unwrap_review_callout(&content);
        content = unwrapped;
    }

    let marker_pos = content
        .find(marker)
        .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;
    let (before_marker, after_marker) = content.split_at(marker_pos);
    let queue_content = &after_marker[marker.len()..];

    let modified_queue =
        modify_question_in_queue(queue_content, params.question_index, &answer_text, defer)
            .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;

    let mut new_content = format!("{before_marker}{marker}{modified_queue}");
    if was_callout {
        new_content = wrap_review_callout(&new_content);
    }

    fs::write(&file_path, &new_content)?;
    let new_hash = content_hash(&new_content);
    db.update_document_content(&params.doc_id, &new_content, &new_hash)?;

    let type_str = question.question_type.as_str();
    if defer {
        let believed = answer_text.starts_with("believed: ");
        let message = if believed {
            "Answer recorded as 'believed' (unverified). It stays in the review queue for human confirmation."
        } else {
            "Question deferred with note. It remains in the review queue for future resolution."
        };
        Ok(serde_json::json!({
            "success": true, "doc_id": params.doc_id, "question_index": params.question_index,
            "question_type": type_str, "description": question.description,
            "deferred": true, "believed": believed, "note": answer_text, "message": message
        }))
    } else {
        Ok(serde_json::json!({
            "success": true, "doc_id": params.doc_id, "question_index": params.question_index,
            "question_type": type_str, "description": question.description,
            "answer": answer_text,
            "message": "Question answered. Use update_document to apply changes to the document."
        }))
    }
}

/// Answers multiple review questions atomically.
#[instrument(name = "svc_bulk_answer_questions", skip(db, items, progress))]
pub fn bulk_answer_questions(
    db: &Database,
    items: &[BulkAnswerItem],
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    if items.len() > 50 {
        return Err(FactbaseError::parse("Maximum 50 answers per call"));
    }
    if items.is_empty() {
        return Ok(
            serde_json::json!({ "success": true, "answered": 0, "skipped": 0, "message": "No answers to process" }),
        );
    }

    // Validate answers
    for (i, item) in items.iter().enumerate() {
        if item.answer.trim().is_empty() {
            return Err(FactbaseError::parse(format!(
                "answers[{i}]: answer cannot be empty"
            )));
        }
    }

    // Group by document
    let mut by_doc: HashMap<String, Vec<(usize, String, Option<String>)>> = HashMap::new();
    for item in items {
        by_doc.entry(item.doc_id.clone()).or_default().push((
            item.question_index,
            item.answer.trim().to_string(),
            item.confidence.clone(),
        ));
    }

    // Validate all documents and questions exist
    let marker = "<!-- factbase:review -->";
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
        for (qi, _, _) in answers_for_doc {
            if *qi >= questions.len() {
                return Err(FactbaseError::parse(format!(
                    "Invalid question_index {} for document {}. Document has {} questions.",
                    qi,
                    doc_id,
                    questions.len()
                )));
            }
        }
        doc_disk_content.insert(doc_id.clone(), (file_path, disk_content));
    }

    // Phase 1: Compute new file contents
    let mut pending_writes: Vec<(String, PathBuf, String)> = Vec::new();
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

        let questions = parse_review_queue(disk_content).unwrap_or_default();
        let mut actionable: Vec<(usize, String, Option<String>)> = Vec::new();
        for (qi, answer_text, confidence) in answers_for_doc {
            if *qi < questions.len() && questions[*qi].answered {
                skipped += 1;
                results.push(
                    serde_json::json!({ "doc_id": doc_id, "question_index": qi, "skipped": true }),
                );
            } else {
                actionable.push((*qi, answer_text.clone(), confidence.clone()));
            }
        }
        if actionable.is_empty() {
            continue;
        }

        actionable.sort_by(|a, b| b.0.cmp(&a.0));

        let mut content = disk_content.clone();
        let was_callout = is_callout_review(&content);
        if was_callout {
            let (unwrapped, _) = unwrap_review_callout(&content);
            content = unwrapped;
        }

        for (qi, answer_text, confidence) in &actionable {
            let marker_pos = content
                .find(marker)
                .ok_or_else(|| FactbaseError::internal("Review Queue marker not found"))?;
            let (before_marker, after_marker) = content.split_at(marker_pos);
            let queue_content = &after_marker[marker.len()..];
            let (defer, text) = resolve_confidence(answer_text, confidence.as_deref())?;
            let modified_queue = modify_question_in_queue(queue_content, *qi, &text, defer)
                .ok_or_else(|| FactbaseError::internal("Failed to find question to modify"))?;
            content = format!("{before_marker}{marker}{modified_queue}");
        }

        if was_callout {
            content = wrap_review_callout(&content);
        }
        pending_writes.push((doc_id.clone(), file_path.clone(), content));

        for (qi, answer_text, _) in answers_for_doc {
            if *qi < questions.len() && questions[*qi].answered {
                continue;
            }
            results.push(serde_json::json!({ "doc_id": doc_id, "question_index": qi, "answer": answer_text }));
        }
    }

    // Phase 2: Write files
    for (_, file_path, content) in &pending_writes {
        fs::write(file_path, content)?;
    }

    // Phase 3: Update DB in transaction
    db.with_transaction(|conn| {
        for (doc_id, _, content) in &pending_writes {
            let new_hash = content_hash(content);
            db.update_document_content_on_conn(conn, doc_id, content, &new_hash)?;
        }
        Ok(())
    })?;

    // Count remaining
    let mut remaining_unanswered = 0usize;
    let mut total_deferred = 0usize;
    let mut total_believed = 0usize;
    let written_ids: std::collections::HashSet<&str> = pending_writes
        .iter()
        .map(|(id, _, _)| id.as_str())
        .collect();
    for (_, _, content) in &pending_writes {
        if let Some(questions) = parse_review_queue(content) {
            count_queue_questions(
                &questions,
                &mut remaining_unanswered,
                &mut total_deferred,
                &mut total_believed,
            );
        }
    }
    let docs_with_queues = db.get_documents_with_review_queue(None).unwrap_or_default();
    for doc in &docs_with_queues {
        if written_ids.contains(doc.id.as_str()) {
            continue;
        }
        if let Some(questions) = parse_review_queue(&doc.content) {
            count_queue_questions(
                &questions,
                &mut remaining_unanswered,
                &mut total_deferred,
                &mut total_believed,
            );
        }
    }

    let answered = results
        .iter()
        .filter(|r| r.get("skipped").is_none())
        .count();
    Ok(serde_json::json!({
        "success": true, "answered": answered, "skipped": skipped, "results": results,
        "remaining_unanswered": remaining_unanswered, "remaining_deferred": total_deferred,
        "remaining_believed": total_believed,
        "message": format!("Answered {} question(s), skipped {} already-answered. {} unanswered remain. Run `factbase review --apply` to process.", answered, skipped, remaining_unanswered)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer_question_basic() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 4: When?\n  > \n";
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

        let result = answer_question(
            &db,
            &AnswerQuestionParams {
                doc_id: "abc123".into(),
                question_index: 0,
                answer: "@t[2020]".into(),
                confidence: None,
            },
        )
        .unwrap();
        assert_eq!(result["success"], true);
        assert!(result.get("skipped").is_none());
    }

    #[test]
    fn test_answer_question_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [x] `@q[stale]` Line 4: Old\n  > Already answered\n";
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

        let result = answer_question(
            &db,
            &AnswerQuestionParams {
                doc_id: "abc123".into(),
                question_index: 0,
                answer: "New answer".into(),
                confidence: None,
            },
        )
        .unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["skipped"], true);
    }

    #[test]
    fn test_bulk_answer_empty() {
        let (db, _tmp) = crate::database::tests::test_db();
        let result = bulk_answer_questions(&db, &[], &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["answered"], 0);
    }

    #[test]
    fn test_bulk_answer_limit() {
        let (db, _tmp) = crate::database::tests::test_db();
        let items: Vec<BulkAnswerItem> = (0..51)
            .map(|i| BulkAnswerItem {
                doc_id: format!("doc{i}"),
                question_index: 0,
                answer: "test".into(),
                confidence: None,
            })
            .collect();
        assert!(bulk_answer_questions(&db, &items, &ProgressReporter::Silent).is_err());
    }

    #[test]
    fn test_answer_question_doc_not_found() {
        let (db, _tmp) = crate::database::tests::test_db();
        let result = answer_question(
            &db,
            &AnswerQuestionParams {
                doc_id: "nonexistent".into(),
                question_index: 0,
                answer: "yes".into(),
                confidence: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_question_with_confidence() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 4: When?\n  > \n";
        std::fs::write(repo_dir.join("test.md"), content).unwrap();
        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(),
            name: "r1".into(),
            path: repo_dir,
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

        let result = answer_question(
            &db,
            &AnswerQuestionParams {
                doc_id: "abc123".into(),
                question_index: 0,
                answer: "2024".into(),
                confidence: Some("believed".into()),
            },
        )
        .unwrap();
        assert_eq!(result["success"], true);
    }

    #[test]
    fn test_bulk_answer_doc_not_found_reports_error() {
        let (db, _tmp) = crate::database::tests::test_db();
        let items = vec![BulkAnswerItem {
            doc_id: "nonexistent".into(),
            question_index: 0,
            answer: "yes".into(),
            confidence: None,
        }];
        // Doc not found propagates as an error
        assert!(bulk_answer_questions(&db, &items, &ProgressReporter::Silent).is_err());
    }
}
