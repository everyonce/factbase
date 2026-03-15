//! Review question answering service.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::processor::{
    append_review_questions, content_hash, is_callout_review, parse_review_queue,
    unwrap_review_callout, wrap_review_callout,
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

/// Recover inline review questions from the DB `review_questions` table when
/// the document's inline `@q[]` markers are missing.
///
/// If the DB has unanswered questions for this doc but the content has none,
/// injects them into the content, writes to disk, and updates the DB document.
/// Returns the (possibly updated) content.
fn recover_questions_from_db_table(
    db: &Database,
    doc_id: &str,
    file_path: &std::path::Path,
    content: &str,
) -> Result<String, FactbaseError> {
    let db_questions = db.get_review_questions_for_doc(doc_id)?;
    if db_questions.is_empty() {
        return Ok(content.to_string());
    }
    let use_callout = is_callout_review(content);
    let updated = append_review_questions(content, &db_questions, use_callout);
    fs::write(file_path, &updated)?;
    let new_hash = content_hash(&updated);
    db.update_document_content(doc_id, &updated, &new_hash)?;
    Ok(updated)
}

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
    let mut questions = parse_review_queue(&content).unwrap_or_default();

    // Fallback: if inline questions are missing, recover from the DB review_questions table
    if questions.is_empty() {
        content = recover_questions_from_db_table(db, &params.doc_id, &file_path, &content)?;
        questions = parse_review_queue(&content).unwrap_or_default();
    }

    if questions.is_empty() {
        return Err(FactbaseError::not_found(format!(
            "No review queue in document {} — it may have been cleaned up or not yet generated. Run check_repository to regenerate.",
            params.doc_id
        )));
    }

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

    // Auto-stamp footnote for dismissed weak-source questions
    new_content = stamp_weak_source_footnote(&new_content, question, &answer_text, defer);

    fs::write(&file_path, &new_content)?;
    let new_hash = content_hash(&new_content);
    db.update_document_content(&params.doc_id, &new_content, &new_hash)?;
    // Sync review question status to DB
    let db_status = if defer {
        if answer_text.starts_with("believed: ") { "believed" } else { "deferred" }
    } else {
        "verified"
    };
    let _ = db.update_review_question_status(
        &params.doc_id,
        params.question_index,
        db_status,
        Some(&answer_text),
    );

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

        let mut questions = parse_review_queue(&disk_content).unwrap_or_default();

        // Fallback: if inline questions are missing, recover from the DB review_questions table
        if questions.is_empty() {
            disk_content = recover_questions_from_db_table(db, doc_id, &file_path, &disk_content)?;
            questions = parse_review_queue(&disk_content).unwrap_or_default();
        }

        if questions.is_empty() {
            return Err(FactbaseError::not_found(format!(
                "No review queue in document {doc_id} — it may have been cleaned up or not yet generated. Run check_repository to regenerate."
            )));
        }
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
            // Auto-stamp footnote for dismissed weak-source questions
            if *qi < questions.len() {
                content = stamp_weak_source_footnote(&content, &questions[*qi], &text, defer);
            }
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

    // Phase 4: Sync review question statuses to DB
    for (doc_id, _, content) in &pending_writes {
        if let Some(questions) = crate::processor::parse_review_queue(content) {
            let _ = db.sync_review_questions(doc_id, &questions);
        }
    }

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

/// Stamp `<!-- ✓ -->` on the footnote definition line when a weak-source question
/// is dismissed via a definitive answer (VALID or dismiss prefix).
/// This prevents the footnote from being re-flagged on subsequent scans.
fn stamp_weak_source_footnote(
    content: &str,
    question: &crate::models::ReviewQuestion,
    answer_text: &str,
    defer: bool,
) -> String {
    use crate::models::QuestionType;
    if defer || question.question_type != QuestionType::WeakSource {
        return content.to_string();
    }
    let lower = answer_text.trim().to_lowercase();
    if !lower.starts_with("valid") && !lower.starts_with("dismiss") {
        return content.to_string();
    }
    let Some(num) = extract_footnote_num_from_desc(&question.description) else {
        return content.to_string();
    };
    let prefix = format!("[^{num}]:");
    const MARKER: &str = "<!-- ✓ -->";
    content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with(prefix.as_str()) && !line.contains(MARKER) {
                format!("{line} {MARKER}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract the first footnote number `N` from `[^N]` in a question description.
fn extract_footnote_num_from_desc(description: &str) -> Option<&str> {
    let start = description.find("[^")?;
    let rest = &description[start + 2..];
    let end = rest.find(']')?;
    let num = &rest[..end];
    if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
        Some(num)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer_question_db_only_questions_fallback() {
        // Regression test: answer op should work when questions exist only in the
        // review_questions DB table, not in the document's inline @q[] markers.
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("myrepo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let doc_file = repo_dir.join("test.md");
        // Document has NO inline review section
        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact [^1]\n\n---\n[^1]: Some source\n";
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

        // Simulate: questions exist only in the DB table (not inline)
        db.sync_review_questions(
            "abc123",
            &[crate::models::ReviewQuestion::new(
                crate::models::QuestionType::WeakSource,
                None,
                "Citation [^1]: weak tier".to_string(),
            )],
        )
        .unwrap();

        // This should NOT fail with "Document has 0 questions"
        let result = answer_question(
            &db,
            &AnswerQuestionParams {
                doc_id: "abc123".into(),
                question_index: 0,
                answer: "VALID: primary source".into(),
                confidence: None,
            },
        )
        .unwrap();
        assert_eq!(result["success"], true);
        assert!(result.get("skipped").is_none());

        // The question should now be inline in the file
        let disk = std::fs::read_to_string(&doc_file).unwrap();
        assert!(disk.contains("@q[weak-source]"), "question should be injected inline: {disk}");
    }

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

    fn make_weak_source_doc(dir: &std::path::Path, footnote_num: u32) -> (crate::database::Database, String) {
        let repo_dir = dir.join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let content = format!(
            "<!-- factbase:ws001 -->\n# Test\n\n- Fact [^{footnote_num}]\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^{footnote_num}]: weak tier\n  > \n\n---\n[^{footnote_num}]: Some source\n"
        );
        std::fs::write(repo_dir.join("test.md"), &content).unwrap();
        let db_path = dir.join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(), name: "r1".into(), path: repo_dir,
            perspective: None, created_at: chrono::Utc::now(),
            last_indexed_at: None, last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "ws001".into(), repo_id: "r1".into(),
            file_path: "test.md".into(), title: "Test".into(),
            content: content.clone(), ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();
        (db, content)
    }

    #[test]
    fn test_weak_source_valid_answer_stamps_footnote() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _) = make_weak_source_doc(dir.path(), 1);
        let result = answer_question(&db, &AnswerQuestionParams {
            doc_id: "ws001".into(), question_index: 0,
            answer: "VALID: sufficient per policy".into(), confidence: None,
        }).unwrap();
        assert_eq!(result["success"], true);
        let disk = std::fs::read_to_string(dir.path().join("repo/test.md")).unwrap();
        assert!(disk.contains("[^1]: Some source <!-- ✓ -->"), "footnote should be stamped: {disk}");
    }

    #[test]
    fn test_weak_source_dismiss_answer_stamps_footnote() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _) = make_weak_source_doc(dir.path(), 2);
        answer_question(&db, &AnswerQuestionParams {
            doc_id: "ws001".into(), question_index: 0,
            answer: "dismiss: internal source".into(), confidence: None,
        }).unwrap();
        let disk = std::fs::read_to_string(dir.path().join("repo/test.md")).unwrap();
        assert!(disk.contains("[^2]: Some source <!-- ✓ -->"), "footnote should be stamped: {disk}");
    }

    #[test]
    fn test_weak_source_believed_answer_does_not_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _) = make_weak_source_doc(dir.path(), 3);
        answer_question(&db, &AnswerQuestionParams {
            doc_id: "ws001".into(), question_index: 0,
            answer: "probably ok".into(), confidence: Some("believed".into()),
        }).unwrap();
        let disk = std::fs::read_to_string(dir.path().join("repo/test.md")).unwrap();
        assert!(!disk.contains("<!-- ✓ -->"), "footnote should NOT be stamped for believed: {disk}");
    }

    #[test]
    fn test_non_weak_source_dismiss_does_not_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let content = "<!-- factbase:ns001 -->\n# Test\n\n- Fact [^1]\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Line 4: When?\n  > \n\n---\n[^1]: Some source\n";
        std::fs::write(repo_dir.join("test.md"), content).unwrap();
        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(), name: "r1".into(), path: repo_dir,
            perspective: None, created_at: chrono::Utc::now(),
            last_indexed_at: None, last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "ns001".into(), repo_id: "r1".into(),
            file_path: "test.md".into(), title: "Test".into(),
            content: content.into(), ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();
        answer_question(&db, &AnswerQuestionParams {
            doc_id: "ns001".into(), question_index: 0,
            answer: "dismiss".into(), confidence: None,
        }).unwrap();
        let disk = std::fs::read_to_string(dir.path().join("repo/test.md")).unwrap();
        assert!(!disk.contains("<!-- ✓ -->"), "non-weak-source should NOT stamp: {disk}");
    }

    #[test]
    fn test_weak_source_no_duplicate_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let content = "<!-- factbase:dup001 -->\n# Test\n\n- Fact [^1]\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[weak-source]` Citation [^1]: weak tier\n  > \n\n---\n[^1]: Some source <!-- ✓ -->\n";
        std::fs::write(repo_dir.join("test.md"), content).unwrap();
        let db_path = dir.path().join("test.db");
        let db = crate::database::Database::new(&db_path).unwrap();
        let repo = crate::models::Repository {
            id: "r1".into(), name: "r1".into(), path: repo_dir,
            perspective: None, created_at: chrono::Utc::now(),
            last_indexed_at: None, last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();
        let doc = crate::models::Document {
            id: "dup001".into(), repo_id: "r1".into(),
            file_path: "test.md".into(), title: "Test".into(),
            content: content.into(), ..crate::models::Document::test_default()
        };
        db.upsert_document(&doc).unwrap();
        answer_question(&db, &AnswerQuestionParams {
            doc_id: "dup001".into(), question_index: 0,
            answer: "VALID: already accepted".into(), confidence: None,
        }).unwrap();
        let disk = std::fs::read_to_string(dir.path().join("repo/test.md")).unwrap();
        assert_eq!(disk.matches("<!-- ✓ -->").count(), 1, "should not duplicate marker: {disk}");
    }
}
