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
        repo_id: repo_filter,
        doc_id: params.doc_id.clone(),
        question_type: params.question_type.clone(),
        status_filter: status_filter.clone(),
        limit,
        offset: params.offset,
    };

    let (questions, total_answered, total_unanswered, total_deferred) =
        db.query_review_questions_db(&db_params)?;

    let returned = questions.len();
    let total = total_answered + total_unanswered + total_deferred;

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
        db.sync_review_questions("bbb222", &[make_question(QuestionType::Stale, "Is this current? (line 3)")]).unwrap();

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
        let mut answered_q = ReviewQuestion::new(QuestionType::Temporal, None, "When did this happen? (line 4)".to_string());
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
}
