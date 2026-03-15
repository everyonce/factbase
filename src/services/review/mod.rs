//! Review queue service — transport-independent business logic.
//!
//! Shared by MCP tools and web API endpoints.

mod answer;
pub mod helpers;
mod queue;

pub use answer::{answer_question, bulk_answer_questions, AnswerQuestionParams, BulkAnswerItem};
pub use helpers::{
    count_question_types, count_queue_questions, format_question_json, modify_question_in_queue,
    resolve_confidence,
};
pub use queue::{get_deferred_items, get_review_queue, ReviewQueueParams};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::processor::strip_deferred_answers_by_type;
use crate::ProgressReporter;
use serde_json::Value;

/// Reset deferred/believed questions of a given type back to open status.
///
/// Updates the DB and strips blockquote answers from the markdown files so the
/// resolve loop will re-evaluate each question with current tools and policy.
pub fn reset_deferred_questions(
    db: &Database,
    question_type: &str,
    repo_id: Option<&str>,
    _progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let (count, affected) = db.reset_deferred_questions_by_type(question_type, repo_id)?;

    let mut files_updated = 0usize;
    let mut file_errors: Vec<String> = Vec::new();

    for (_doc_id, file_path) in &affected {
        match std::fs::read_to_string(file_path) {
            Ok(content) => {
                let (new_content, stripped) =
                    strip_deferred_answers_by_type(&content, question_type);
                if stripped > 0 {
                    if let Err(e) = std::fs::write(file_path, new_content) {
                        file_errors.push(format!("{file_path}: {e}"));
                    } else {
                        files_updated += 1;
                    }
                }
            }
            Err(e) => {
                file_errors.push(format!("{file_path}: {e}"));
            }
        }
    }

    let mut result = serde_json::json!({
        "success": true,
        "reset": count,
        "files_updated": files_updated,
        "message": format!(
            "Reset {count} deferred/believed {question_type} question(s) to open across {files_updated} file(s)."
        )
    });
    if !file_errors.is_empty() {
        result["file_errors"] = Value::Array(
            file_errors
                .into_iter()
                .map(Value::String)
                .collect(),
        );
    }
    Ok(result)
}

/// Unified answer dispatch: routes to single, bulk, or bulk-dismiss based on params.
pub fn answer_questions(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    // Bulk dismiss path: status='dismiss' with optional filters
    if args.get("status").and_then(|v| v.as_str()) == Some("dismiss") {
        let doc_id_filter = args.get("doc_id").and_then(|v| v.as_str());
        let type_filter = args.get("question_type").and_then(|v| v.as_str());
        let desc_filter = args.get("description_filter").and_then(|v| v.as_str());
        let rows = db.bulk_update_review_question_status(
            doc_id_filter,
            type_filter,
            desc_filter,
            "dismissed",
        )?;
        return Ok(serde_json::json!({
            "success": true,
            "dismissed": rows,
            "message": format!("Dismissed {rows} review question(s).")
        }));
    }

    if args.get("answers").is_some() {
        // Bulk path: parse from Value (MCP compat)
        let answers_arr = args
            .get("answers")
            .and_then(|v| v.as_array())
            .ok_or_else(|| FactbaseError::parse("answers array is required"))?;
        let items: Vec<BulkAnswerItem> = answers_arr
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Ok(BulkAnswerItem {
                    doc_id: a
                        .get("doc_id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .ok_or_else(|| {
                            FactbaseError::parse(format!("answers[{i}]: doc_id is required"))
                        })?,
                    question_index: a.get("question_index").and_then(Value::as_u64).ok_or_else(
                        || {
                            FactbaseError::parse(format!(
                                "answers[{i}]: question_index is required"
                            ))
                        },
                    )? as usize,
                    answer: a
                        .get("answer")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .ok_or_else(|| {
                            FactbaseError::parse(format!("answers[{i}]: answer is required"))
                        })?,
                    confidence: a
                        .get("confidence")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                })
            })
            .collect::<Result<Vec<_>, FactbaseError>>()?;
        bulk_answer_questions(db, &items, progress)
    } else {
        let doc_id = args
            .get("doc_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse("Missing doc_id parameter"))?;
        let question_index = args
            .get("question_index")
            .and_then(Value::as_u64)
            .ok_or_else(|| FactbaseError::parse("Missing question_index parameter"))?
            as usize;
        let answer_text = args
            .get("answer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FactbaseError::parse("Missing answer parameter"))?;
        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_str())
            .map(String::from);
        answer_question(
            db,
            &AnswerQuestionParams {
                doc_id: doc_id.to_string(),
                question_index,
                answer: answer_text.to_string(),
                confidence,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer_questions_missing_doc_id() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"question_index": 0, "answer": "yes"});
        let result = answer_questions(&db, &args, &ProgressReporter::Silent);
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_questions_missing_answer() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"doc_id": "abc", "question_index": 0});
        let result = answer_questions(&db, &args, &ProgressReporter::Silent);
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_questions_bulk_missing_answers_array() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"answers": "not_an_array"});
        let result = answer_questions(&db, &args, &ProgressReporter::Silent);
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_questions_bulk_missing_fields() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"answers": [{"doc_id": "abc"}]});
        let result = answer_questions(&db, &args, &ProgressReporter::Silent);
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_questions_bulk_empty_array() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"answers": []});
        let result = answer_questions(&db, &args, &ProgressReporter::Silent);
        // Empty array should succeed (no-op)
        assert!(result.is_ok());
    }
}
