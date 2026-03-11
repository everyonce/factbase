//! Review queue retrieval service.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::QuestionType;
use crate::processor::parse_review_queue;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

use super::helpers::{format_question_json, parse_type_filter, resolve_repo_filter};

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

/// Gets pending review questions across documents.
#[instrument(name = "svc_get_review_queue", skip(db, params, progress))]
pub fn get_review_queue(
    db: &Database,
    params: &ReviewQueueParams,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_filter = resolve_repo_filter(db, params.repo.as_deref())?;
    let type_filter: Option<QuestionType> =
        params.question_type.as_ref().and_then(|t| parse_type_filter(t));
    let status_filter = params.status.as_deref().unwrap_or("unanswered");
    let limit = if params.limit == 0 { 10 } else { params.limit };
    let offset = params.offset;

    let mut all_questions: Vec<Value> = Vec::new();
    let mut total_answered = 0u64;
    let mut total_unanswered = 0u64;
    let mut total_deferred = 0u64;

    let mut docs = db.get_documents_with_review_queue(repo_filter.as_deref())?;

    // Fallback: if a specific doc_id is requested but not in the list, fetch directly
    if let Some(ref filter_id) = params.doc_id {
        if !docs.iter().any(|d| d.id == *filter_id) {
            if let Ok(Some(doc)) = db.get_document(filter_id) {
                if !doc.is_deleted { docs.push(doc); }
            }
        }
    }

    let total_docs = docs.len();
    progress.log(&format!("Processing {total_docs} documents with review queues"));

    let mut matched = 0usize;
    let mut docs_processed = 0usize;
    let page_filled = |qs: &[Value]| qs.len() >= limit;

    for doc in &docs {
        if page_filled(&all_questions) { break; }

        if let Some(ref filter_id) = params.doc_id {
            if &doc.id != filter_id { continue; }
        }

        docs_processed += 1;
        if total_docs >= 50 && docs_processed.is_multiple_of(50) {
            progress.report(docs_processed, total_docs, &doc.title);
        }

        if let Some(questions) = parse_review_queue(&doc.content) {
            for (idx, q) in questions.iter().enumerate() {
                if let Some(ref ft) = type_filter {
                    if &q.question_type != ft { continue; }
                }

                let is_deferred = q.is_deferred();
                if q.answered {
                    total_answered += 1;
                } else if is_deferred {
                    total_deferred += 1;
                } else {
                    total_unanswered += 1;
                }

                let include = match status_filter {
                    "all" => true,
                    "answered" => q.answered,
                    "deferred" => is_deferred,
                    _ => !q.answered && !is_deferred,
                };
                if !include { continue; }

                if matched >= offset && all_questions.len() < limit {
                    let mut qjson = format_question_json(q, Some((&doc.id, &doc.title)));
                    if let Some(obj) = qjson.as_object_mut() {
                        obj.insert("question_index".to_string(), serde_json::json!(idx));
                        if is_deferred {
                            obj.insert("deferred".to_string(), Value::Bool(true));
                        }
                    }
                    all_questions.push(qjson);
                }
                matched += 1;
            }
        }
    }

    let mut result = serde_json::json!({
        "questions": all_questions,
        "total": total_answered + total_deferred + total_unanswered,
        "returned": all_questions.len(),
        "offset": offset,
        "limit": limit,
        "answered": total_answered,
        "deferred": total_deferred,
        "unanswered": total_unanswered,
        "status_filter": status_filter
    });

    if docs_processed < total_docs {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("has_more".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

/// Gets deferred review items as a focused summary.
#[instrument(name = "svc_get_deferred_items", skip(db, progress))]
pub fn get_deferred_items(
    db: &Database,
    params: &ReviewQueueParams,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let mut deferred_params = ReviewQueueParams {
        status: Some("deferred".to_string()),
        repo: params.repo.clone(),
        doc_id: params.doc_id.clone(),
        question_type: params.question_type.clone(),
        limit: params.limit,
        offset: params.offset,
        include_context: params.include_context,
    };
    if deferred_params.limit == 0 { deferred_params.limit = 10; }

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

    #[test]
    fn test_get_review_queue_empty_db() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));
        let params = ReviewQueueParams { limit: 10, ..Default::default() };
        let result = get_review_queue(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total"], 0);
    }

    #[test]
    fn test_get_deferred_items_empty() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let content = "<!-- factbase:bbb222 -->\n# Test\n\n## Review Queue\n\n<!-- factbase:review -->\n\n- [ ] `@q[stale]` Is this current? (line 3)\n";
        let mut doc = crate::models::Document::test_default();
        doc.id = "bbb222".to_string();
        doc.title = "Test".to_string();
        doc.content = content.to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let params = ReviewQueueParams { limit: 10, ..Default::default() };
        let result = get_deferred_items(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total_deferred"], 0);
        assert_eq!(result["summary"], "No deferred items.");
    }

    #[test]
    fn test_get_deferred_items_returns_deferred() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        let content = "<!-- factbase:aaa111 -->\n# Test\n\nSome fact\n\n## Review Queue\n\n<!-- factbase:review -->\n\n- [ ] `@q[stale]` Is this still current? (line 4)\n- [x] `@q[temporal]` When did this happen? (line 4)\n  > 2024-01\n- [ ] `@q[missing]` What is the source? (line 4)\n  > defer: needs more research\n";
        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test".to_string();
        doc.content = content.to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let params = ReviewQueueParams { limit: 10, ..Default::default() };
        let result = get_deferred_items(&db, &params, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["total_deferred"], 1);
        let items = result["deferred_items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "missing");
    }
}
