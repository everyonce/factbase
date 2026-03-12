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
use crate::ProgressReporter;
use serde_json::Value;

/// Unified answer dispatch: routes to single or bulk based on params.
pub fn answer_questions(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
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
