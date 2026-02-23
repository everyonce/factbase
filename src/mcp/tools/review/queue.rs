//! Review queue retrieval MCP tool.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_bool_arg, get_str_arg, get_u64_arg};
use crate::models::QuestionType;
use crate::processor::parse_review_queue;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

use super::format_question_json;

/// Parses a type filter string into a QuestionType.
///
/// Returns None for invalid or unrecognized type strings.
fn parse_type_filter(type_str: &str) -> Option<QuestionType> {
    match type_str.to_lowercase().as_str() {
        "temporal" => Some(QuestionType::Temporal),
        "conflict" => Some(QuestionType::Conflict),
        "missing" => Some(QuestionType::Missing),
        "ambiguous" => Some(QuestionType::Ambiguous),
        "stale" => Some(QuestionType::Stale),
        "duplicate" => Some(QuestionType::Duplicate),
        "corruption" => Some(QuestionType::Corruption),
        _ => None,
    }
}

/// Gets pending review questions across documents.
///
/// Parses review queues from document content and aggregates questions.
///
/// # Arguments (from JSON)
/// - `repo` (optional): Filter by repository ID
/// - `doc_id` (optional): Filter by specific document ID
/// - `type` (optional): Filter by question type (temporal, conflict, missing, etc.)
/// - `include_context` (optional): Include surrounding lines from the document for each question (default: false)
///
/// # Returns
/// JSON with `questions` array (doc_id, doc_title, type, description, answered, answer),
/// `total`, `answered`, and `unanswered` counts.
#[instrument(name = "mcp_get_review_queue", skip(db, args, progress))]
pub fn get_review_queue(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_filter = get_str_arg(args, "repo").map(String::from);
    let doc_id_owned = args.get("doc_id").and_then(|v| {
        v.as_str()
            .map(String::from)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    });
    let doc_id_filter = doc_id_owned;
    let type_filter = get_str_arg(args, "type").map(String::from);
    let _include_context = get_bool_arg(args, "include_context", false);
    let status_filter = get_str_arg(args, "status").unwrap_or("unanswered");

    // Parse type filter into QuestionType
    let question_type_filter: Option<QuestionType> =
        type_filter.as_ref().and_then(|t| parse_type_filter(t));

    let mut all_questions: Vec<Value> = Vec::new();
    let mut total_answered = 0;
    let mut total_unanswered = 0;
    let mut total_deferred = 0;

    let limit = get_u64_arg(args, "limit", 10) as usize;
    let offset = get_u64_arg(args, "offset", 0) as usize;

    // Only load documents that have review queues (indexed via has_review_queue flag)
    let mut docs = db.get_documents_with_review_queue(repo_filter.as_deref())?;

    // Fallback: if a specific doc_id is requested but not in the list, fetch it
    // directly (the has_review_queue flag may be stale).
    if let Some(ref filter_id) = doc_id_filter {
        if !docs.iter().any(|d| d.id == *filter_id) {
            if let Ok(Some(doc)) = db.get_document(filter_id) {
                if !doc.is_deleted {
                    docs.push(doc);
                }
            }
        }
    }

    let total_docs = docs.len();

    progress.log(&format!(
        "Processing {total_docs} documents with review queues"
    ));

    let mut matched = 0usize; // count of questions matching all filters (for pagination)
    let mut docs_processed = 0usize;
    let page_filled = |qs: &[Value]| qs.len() >= limit;

    for doc in &docs {
        // Early termination: once page is filled, skip remaining docs
        // (totals will reflect only documents processed so far)
        if page_filled(&all_questions) {
            break;
        }

        // Skip if doc_id filter doesn't match
        if let Some(ref filter_id) = doc_id_filter {
            if &doc.id != filter_id {
                continue;
            }
        }

        docs_processed += 1;

        // Report progress every 50 documents
        if total_docs >= 50 && docs_processed.is_multiple_of(50) {
            progress.report(docs_processed, total_docs, &doc.title);
        }

        // Parse review queue from document
        if let Some(questions) = parse_review_queue(&doc.content) {
            for (idx, q) in questions.iter().enumerate() {
                // Apply type filter
                if let Some(ref filter_type) = question_type_filter {
                    if &q.question_type != filter_type {
                        continue;
                    }
                }

                // Classify: answered, deferred (unchecked but has answer/note), unanswered
                let is_deferred = q.is_deferred();
                if q.answered {
                    total_answered += 1;
                } else if is_deferred {
                    total_deferred += 1;
                } else {
                    total_unanswered += 1;
                }

                // Apply status filter
                let include = match status_filter {
                    "all" => true,
                    "answered" => q.answered,
                    "deferred" => is_deferred,
                    _ => !q.answered && !is_deferred, // "unanswered" (default)
                };
                if !include {
                    continue;
                }

                // Paginate over matched questions
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

    // Indicate when totals are approximate due to early termination
    if docs_processed < total_docs {
        if let Some(obj) = result.as_object_mut() {
            obj.insert("has_more".to_string(), Value::Bool(true));
        }
    }

    Ok(result)
}

/// Gets deferred review items as a focused summary.
///
/// Delegates to `get_review_queue` with `status: "deferred"` and reshapes
/// the response into a concise format for surfacing deferred items.
#[instrument(name = "mcp_get_deferred_items", skip(db, args, progress))]
pub fn get_deferred_items(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    // Build args with status=deferred, preserving caller's repo/type/limit/offset
    let mut deferred_args = args.clone();
    if let Some(obj) = deferred_args.as_object_mut() {
        obj.insert("status".to_string(), serde_json::json!("deferred"));
    }

    let result = get_review_queue(db, &deferred_args, progress)?;

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
    use crate::models::ReviewQuestion;

    #[test]
    fn test_parse_type_filter_valid_types() {
        assert_eq!(parse_type_filter("temporal"), Some(QuestionType::Temporal));
        assert_eq!(parse_type_filter("conflict"), Some(QuestionType::Conflict));
        assert_eq!(parse_type_filter("missing"), Some(QuestionType::Missing));
        assert_eq!(
            parse_type_filter("ambiguous"),
            Some(QuestionType::Ambiguous)
        );
        assert_eq!(parse_type_filter("stale"), Some(QuestionType::Stale));
        assert_eq!(
            parse_type_filter("duplicate"),
            Some(QuestionType::Duplicate)
        );
    }

    #[test]
    fn test_parse_type_filter_case_insensitive() {
        assert_eq!(parse_type_filter("TEMPORAL"), Some(QuestionType::Temporal));
        assert_eq!(parse_type_filter("Conflict"), Some(QuestionType::Conflict));
        assert_eq!(parse_type_filter("MiSsInG"), Some(QuestionType::Missing));
    }

    #[test]
    fn test_parse_type_filter_invalid_returns_none() {
        assert_eq!(parse_type_filter("invalid"), None);
        assert_eq!(parse_type_filter(""), None);
        assert_eq!(parse_type_filter("temp"), None);
        assert_eq!(parse_type_filter("temporalx"), None);
    }

    #[test]
    fn test_format_question_json_with_doc_context() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "When was this role held?".to_string(),
            answered: false,
            answer: None,
            line_number: 10,
        };

        let json = format_question_json(&q, Some(("abc123", "Test Doc")));

        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["doc_title"], "Test Doc");
        assert_eq!(json["type"], "temporal");
        assert_eq!(json["line_ref"], 5);
        assert_eq!(json["description"], "When was this role held?");
        assert_eq!(json["answered"], false);
        assert!(json["answer"].is_null());
    }

    #[test]
    fn test_format_question_json_with_answer() {
        let q = ReviewQuestion {
            question_type: QuestionType::Missing,
            line_ref: None,
            description: "What is the source?".to_string(),
            answered: true,
            answer: Some("LinkedIn profile".to_string()),
            line_number: 20,
        };

        let json = format_question_json(&q, Some(("def456", "Another Doc")));

        assert_eq!(json["doc_id"], "def456");
        assert_eq!(json["type"], "missing");
        assert!(json["line_ref"].is_null());
        assert_eq!(json["answered"], true);
        assert_eq!(json["answer"], "LinkedIn profile");
    }

    #[test]
    fn test_get_deferred_items_returns_only_deferred() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        // Doc with mixed questions: unanswered, answered, and deferred (unchecked with answer)
        let content = "<!-- factbase:aaa111 -->\n# Test\n\nSome fact\n\n## Review Queue\n\n<!-- factbase:review -->\n\n- [ ] `@q[stale]` Is this still current? (line 4)\n- [x] `@q[temporal]` When did this happen? (line 4)\n  > 2024-01\n- [ ] `@q[missing]` What is the source? (line 4)\n  > defer: needs more research\n";
        let mut doc = crate::models::Document::test_default();
        doc.id = "aaa111".to_string();
        doc.title = "Test".to_string();
        doc.content = content.to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let reporter = ProgressReporter::Silent;
        let result = get_deferred_items(&db, &serde_json::json!({}), &reporter).unwrap();

        assert_eq!(result["total_deferred"], 1);
        let items = result["deferred_items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "missing");
        assert!(result["summary"].as_str().unwrap().contains("1 item"));
    }

    #[test]
    fn test_get_deferred_items_empty_when_none() {
        let (db, _tmp) = crate::database::tests::test_db();
        crate::database::tests::test_repo_in_db(&db, "test", std::path::Path::new("/tmp/test"));

        // Doc with only unanswered questions (no deferred)
        let content = "<!-- factbase:bbb222 -->\n# Test\n\n## Review Queue\n\n<!-- factbase:review -->\n\n- [ ] `@q[stale]` Is this current? (line 3)\n";
        let mut doc = crate::models::Document::test_default();
        doc.id = "bbb222".to_string();
        doc.title = "Test".to_string();
        doc.content = content.to_string();
        doc.repo_id = "test".to_string();
        db.upsert_document(&doc).unwrap();

        let reporter = ProgressReporter::Silent;
        let result = get_deferred_items(&db, &serde_json::json!({}), &reporter).unwrap();

        assert_eq!(result["total_deferred"], 0);
        assert!(result["deferred_items"].as_array().unwrap().is_empty());
        assert_eq!(result["summary"], "No deferred items.");
    }
}
