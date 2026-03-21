//! Review queue retrieval service.

use crate::database::{Database, ReviewQueueDbParams};
use crate::error::FactbaseError;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

use super::helpers::resolve_repo_filter;

/// Typed parameters for review queue retrieval.
#[derive(Debug, Default)]
pub struct ReviewQueueParams {
    pub repo: Option<String>,
    pub doc_id: Option<String>,
    pub question_type: Option<String>,
    pub status: Option<String>,
    pub limit: usize,
    pub offset: usize,
    pub include_context: bool,
}

/// Gets pending review questions — reads from DB index for fast access.
/// Falls back to parsing document content when the DB index is empty but
/// documents with review sections exist (e.g. after external edits that
/// bypassed the scanner).
#[instrument(name = "svc_get_review_queue", skip(db, params, progress))]
pub fn get_review_queue(
    db: &Database,
    params: &ReviewQueueParams,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_filter = resolve_repo_filter(db, params.repo.as_deref())?;
    let status_filter = params.status.as_deref().unwrap_or("unanswered").to_string();
    let limit = if params.limit == 0 { 10 } else { params.limit };

    progress.log("Querying review questions from DB index");

    let db_params = ReviewQueueDbParams {
        repo_id: repo_filter.clone(),
        doc_id: params.doc_id.clone(),
        question_type: params.question_type.clone(),
        status_filter: status_filter.clone(),
        limit,
        offset: params.offset,
    };

    let (questions, total_answered, total_unanswered, total_deferred) =
        db.query_review_questions_db(&db_params)?;

    let total = total_answered + total_unanswered + total_deferred;

    // Fallback: if DB index is empty but documents with review sections exist,
    // re-sync from stored content and re-query. This handles the case where
    // review edits were made externally and the scanner hasn't run yet.
    if total == 0 {
        let docs_to_sync: Vec<crate::models::Document> = if let Some(ref doc_id) = params.doc_id {
            // Targeted fallback: only sync the requested document
            db.get_document(doc_id)?
                .into_iter()
                .filter(|d| crate::patterns::has_review_section(&d.content))
                .collect()
        } else {
            db.get_documents_with_review_queue(repo_filter.as_deref())?
        };
        if !docs_to_sync.is_empty() {
            progress.log("DB review index empty — re-syncing from stored content");
            for doc in &docs_to_sync {
                if let Some(questions) = crate::processor::parse_review_queue(&doc.content) {
                    let _ = db.sync_review_questions(&doc.id, &questions);
                }
            }
            // Re-query after sync
            let (questions2, ans2, unans2, def2) = db.query_review_questions_db(&db_params)?;
            let returned2 = questions2.len();
            let total2 = ans2 + unans2 + def2;
            return Ok(serde_json::json!({
                "questions": questions2,
                "total": total2,
                "returned": returned2,
                "offset": params.offset,
                "limit": limit,
                "answered": ans2,
                "deferred": def2,
                "unanswered": unans2,
                "status_filter": status_filter
            }));
        }
    }

    let returned = questions.len();

    Ok(serde_json::json!({
        "questions": questions,
        "total": total,
        "returned": returned,
        "offset": params.offset,
        "limit": limit,
        "answered": total_answered,
        "deferred": total_deferred,
        "unanswered": total_unanswered,
        "status_filter": status_filter
    }))
}

/// Gets deferred review items as a focused summary.
#[instrument(name = "svc_get_deferred_items", skip(db, progress))]
pub fn get_deferred_items(
    db: &Database,
    params: &ReviewQueueParams,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let deferred_params = ReviewQueueParams {
        status: Some("deferred".to_string()),
        repo: params.repo.clone(),
        doc_id: params.doc_id.clone(),
        question_type: params.question_type.clone(),
        limit: if params.limit == 0 { 10 } else { params.limit },
        offset: params.offset,
        include_context: params.include_context,
    };

    let result = get_review_queue(db, &deferred_params, progress)?;

    let items = result["questions"].as_array().cloned().unwrap_or_default();
    let total = result["deferred"].as_u64().unwrap_or(0);

    let summary = match total {
        0 => "No deferred items.".to_string(),
        1 => "1 item needs human attention.".to_string(),
        n => format!("{n} items need human attention."),
    };

    Ok(serde_json::json!({
        "deferred_items": items,
        "total_deferred": total,
        "summary": summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuestionType, ReviewQuestion};

    fn make_question(qt: QuestionType, desc: &str) -> ReviewQuestion {
        ReviewQuestion::new(qt, None, desc.to_string())
    }

    fn make_deferred_question(qt: QuestionType, desc: &str) -> ReviewQuestion {
        let mut q = ReviewQuestion::new(qt, None, desc.to_string());
        q.answer = Some("defer: needs research".to_string());
        q
    }

    #[test]
    fn test_get_review_queue_empty_db() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));
        let params = ReviewQueueParams {
            limit: 10,
            ..Default::default()
        };
        let result = get_review_queue(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total"], 0);
    }

    #[test]
    fn test_get_deferred_items_empty() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "bbb222".to_string();
        doc.title = "Test".to_string();
        doc.repo_id = "test".to_string();
        doc.content = "---\nfactbase_id: bbb222\n---\n# Test\n\n## Review Queue\n\n<!-- factbase:review -->\n\n- [ ] `@q[stale]` Is this current? (line 3)\n".to_string();
        db.upsert_document(&doc).unwrap();
        // Sync review questions (no deferred)
        db.sync_review_questions(
            "bbb222",
            &[make_question(
                QuestionType::Stale,
                "Is this current? (line 3)",
            )],
        )
        .unwrap();

        let params = ReviewQueueParams {
            limit: 10,
            ..Default::default()
        };
        let result = get_deferred_items(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total_deferred"], 0);
        assert_eq!(result["summary"], "No deferred items.");
    }

    #[test]
    fn test_get_deferred_items_returns_deferred() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test".to_string();
        doc.repo_id = "test".to_string();
        doc.content = "---\nfactbase_id: aaa111\n---\n# Test\n\nSome fact\n".to_string();
        db.upsert_document(&doc).unwrap();

        // Sync: 1 open, 1 answered, 1 deferred
        let mut answered_q = ReviewQuestion::new(
            QuestionType::Temporal,
            None,
            "When did this happen? (line 4)".to_string(),
        );
        answered_q.answered = true;
        answered_q.answer = Some("2024-01".to_string());
        let questions = vec![
            make_question(QuestionType::Stale, "Is this still current? (line 4)"),
            answered_q,
            make_deferred_question(QuestionType::Missing, "What is the source? (line 4)"),
        ];
        db.sync_review_questions("aaa111", &questions).unwrap();

        let params = ReviewQueueParams {
            limit: 10,
            ..Default::default()
        };
        let result = get_deferred_items(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total_deferred"], 1);
        let items = result["deferred_items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "missing");
    }

    /// When DB review index is empty but documents with review sections exist,
    /// get_review_queue should fall back to parsing stored content and re-sync.
    #[test]
    fn test_get_review_queue_fallback_when_db_empty() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "fb0001".to_string();
        doc.repo_id = "test".to_string();
        doc.content = "---\nfactbase_id: fb0001\n---\n# Doc\n\nA fact.\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Is this current? (line 3)\n".to_string();
        db.upsert_document(&doc).unwrap();
        // Intentionally do NOT call sync_review_questions — simulates stale DB index

        let params = ReviewQueueParams {
            limit: 10,
            ..Default::default()
        };
        let result = get_review_queue(&db, &params, &ProgressReporter::Silent).unwrap();
        // Fallback should have found and synced the question
        assert_eq!(
            result["unanswered"], 1,
            "fallback should surface the unanswered question"
        );
        assert_eq!(result["total"], 1);
    }

    /// When doc_id filter is provided and DB index is empty for that doc,
    /// fallback should sync that specific document and return its questions.
    #[test]
    fn test_get_review_queue_fallback_with_doc_id_filter() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "fb0003".to_string();
        doc.repo_id = "test".to_string();
        doc.file_path = "fb0003.md".to_string();
        doc.content = "---\nfactbase_id: fb0003\n---\n# Doc\n\nA fact.\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Is this current? (line 3)\n- [ ] `@q[temporal]` When did this happen? (line 4)\n".to_string();
        db.upsert_document(&doc).unwrap();
        // Intentionally do NOT call sync_review_questions — simulates stale DB index

        // Also add a second doc with questions to ensure filtering works
        let mut doc2 = crate::models::Document::test_default();
        doc2.id = "fb0004".to_string();
        doc2.repo_id = "test".to_string();
        doc2.file_path = "fb0004.md".to_string();
        doc2.content = "---\nfactbase_id: fb0004\n---\n# Doc2\n\nAnother fact.\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` What is the source? (line 3)\n".to_string();
        db.upsert_document(&doc2).unwrap();

        // Query with doc_id filter — should only return questions for fb0003
        let params = ReviewQueueParams {
            doc_id: Some("fb0003".to_string()),
            limit: 10,
            ..Default::default()
        };
        let result = get_review_queue(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(
            result["unanswered"], 2,
            "fallback with doc_id should surface questions for that doc"
        );
        let questions = result["questions"].as_array().unwrap();
        assert!(questions.iter().all(|q| q["doc_id"] == "fb0003"));

        // Query for nonexistent doc_id — should return 0
        let params_none = ReviewQueueParams {
            doc_id: Some("ffffff".to_string()),
            limit: 10,
            ..Default::default()
        };
        let result_none = get_review_queue(&db, &params_none, &ProgressReporter::Silent).unwrap();
        assert_eq!(result_none["total"], 0);
    }

    /// Fallback should not trigger when DB already has questions.
    #[test]
    fn test_get_review_queue_no_fallback_when_db_populated() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let mut doc = crate::models::Document::test_default();
        doc.id = "fb0002".to_string();
        doc.repo_id = "test".to_string();
        doc.content = "---\nfactbase_id: fb0002\n---\n# Doc\n\nA fact.\n\n<!-- factbase:review -->\n- [ ] `@q[stale]` Is this current? (line 3)\n".to_string();
        db.upsert_document(&doc).unwrap();
        db.sync_review_questions(
            "fb0002",
            &[make_question(
                QuestionType::Stale,
                "Is this current? (line 3)",
            )],
        )
        .unwrap();

        let params = ReviewQueueParams {
            limit: 10,
            ..Default::default()
        };
        let result = get_review_queue(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["unanswered"], 1);
    }
}
