//! Review queue MCP tools.
//!
//! Thin wrappers that parse JSON args and delegate to the service layer.
//!
//! # Public API
//! - `get_review_queue` - Get pending review questions
//! - `answer_question` - Answer a single question
//! - `bulk_answer_questions` - Answer multiple questions atomically
//! - `generate_questions` - Generate review questions for a document
//! - `check_repository` - Run rule-based quality checks

mod check;
mod generate;

pub use check::check_repository;
pub use generate::generate_questions;

// Re-export service types and functions for backward compatibility
pub use crate::services::review::{
    count_question_types, format_question_json, modify_question_in_queue, AnswerQuestionParams,
    BulkAnswerItem, ReviewQueueParams,
};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::services;
use serde_json::Value;

/// Gets pending review questions. Parses JSON args and delegates to service.
pub fn get_review_queue(
    db: &Database,
    args: &Value,
    progress: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let params = ReviewQueueParams {
        repo: crate::mcp::tools::get_str_arg(args, "repo").map(String::from),
        doc_id: args.get("doc_id").and_then(|v| {
            v.as_str()
                .map(String::from)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        }),
        question_type: crate::mcp::tools::get_str_arg(args, "type").map(String::from),
        status: Some(
            crate::mcp::tools::get_str_arg(args, "status")
                .unwrap_or("unanswered")
                .to_string(),
        ),
        limit: crate::mcp::tools::get_u64_arg(args, "limit", 10) as usize,
        offset: crate::mcp::tools::get_u64_arg(args, "offset", 0) as usize,
        include_context: crate::mcp::tools::get_bool_arg(args, "include_context", false),
    };
    services::get_review_queue(db, &params, progress)
}

/// Gets deferred review items. Parses JSON args and delegates to service.
pub fn get_deferred_items(
    db: &Database,
    args: &Value,
    progress: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let params = ReviewQueueParams {
        repo: crate::mcp::tools::get_str_arg(args, "repo").map(String::from),
        doc_id: args
            .get("doc_id")
            .and_then(|v| v.as_str().map(String::from)),
        question_type: crate::mcp::tools::get_str_arg(args, "type").map(String::from),
        status: Some("deferred".to_string()),
        limit: crate::mcp::tools::get_u64_arg(args, "limit", 10) as usize,
        offset: crate::mcp::tools::get_u64_arg(args, "offset", 0) as usize,
        include_context: false,
    };
    services::get_deferred_items(db, &params, progress)
}

/// Marks a review question as answered. Parses JSON args and delegates to service.
pub fn answer_question(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let params = AnswerQuestionParams {
        doc_id: crate::mcp::tools::get_str_arg_required(args, "doc_id")?,
        question_index: crate::mcp::tools::get_u64_arg_required(args, "question_index")? as usize,
        answer: crate::mcp::tools::get_str_arg_required(args, "answer")?,
        confidence: crate::mcp::tools::get_str_arg(args, "confidence").map(String::from),
        agent_suggestion: crate::mcp::tools::get_str_arg(args, "agent_suggestion")
            .map(String::from),
    };
    services::answer_question(db, &params)
}

/// Answers multiple review questions atomically. Parses JSON args and delegates to service.
pub fn bulk_answer_questions(
    db: &Database,
    args: &Value,
    progress: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let answers = args
        .get("answers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| FactbaseError::parse("answers array is required"))?;
    if answers.len() > 50 {
        return Err(FactbaseError::parse("Maximum 50 answers per call"));
    }
    let items: Vec<BulkAnswerItem> = answers
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
                    || FactbaseError::parse(format!("answers[{i}]: question_index is required")),
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
                agent_suggestion: a
                    .get("agent_suggestion")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
        })
        .collect::<Result<Vec<_>, FactbaseError>>()?;
    services::bulk_answer_questions(db, &items, progress)
}

/// Unified answer tool: dispatches to single or bulk based on args.
pub fn answer_questions(
    db: &Database,
    args: &Value,
    progress: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    services::answer_questions(db, args, progress)
}

/// Resets deferred/believed questions of a given type back to open status.
/// Parses JSON args and delegates to service.
pub fn reset_deferred_questions(
    db: &Database,
    args: &Value,
    progress: &crate::ProgressReporter,
) -> Result<Value, FactbaseError> {
    let question_type =
        crate::mcp::tools::get_str_arg(args, "question_type").unwrap_or("weak-source");
    let repo = crate::mcp::tools::get_str_arg(args, "repo").map(String::from);
    services::reset_deferred_questions(db, question_type, repo.as_deref(), progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QuestionType, ReviewQuestion};

    #[test]
    fn test_format_question_json_with_doc_context() {
        let q = ReviewQuestion {
            question_type: QuestionType::Temporal,
            line_ref: Some(5),
            description: "When was this role held?".to_string(),
            answered: false,
            answer: None,
            line_number: 10,
            confidence: None,
            confidence_reason: None,
            agent_reasoning: None,
        };
        let json = format_question_json(&q, Some(("abc123", "Test Doc")));
        assert_eq!(json["doc_id"], "abc123");
        assert_eq!(json["doc_title"], "Test Doc");
        assert_eq!(json["type"], "temporal");
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
            confidence: None,
            confidence_reason: None,
            agent_reasoning: None,
        };
        let json = format_question_json(&q, Some(("def456", "Another Doc")));
        assert_eq!(json["type"], "missing");
        assert_eq!(json["answered"], true);
        assert_eq!(json["answer"], "LinkedIn profile");
    }

    #[test]
    fn test_get_review_queue_parses_args() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"limit": 5});
        let result = get_review_queue(&db, &args, &crate::ProgressReporter::Silent);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_deferred_items_parses_args() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({});
        let result = get_deferred_items(&db, &args, &crate::ProgressReporter::Silent);
        assert!(result.is_ok());
    }

    #[test]
    fn test_answer_question_missing_doc_id() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"question_index": 0, "answer": "yes"});
        let result = answer_question(&db, &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_bulk_answer_over_limit() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let answers: Vec<serde_json::Value> = (0..51)
            .map(|i| serde_json::json!({"doc_id": "abc", "question_index": i, "answer": "yes"}))
            .collect();
        let args = serde_json::json!({"answers": answers});
        let result = bulk_answer_questions(&db, &args, &crate::ProgressReporter::Silent);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("50"));
    }

    #[test]
    fn test_bulk_answer_missing_answers_array() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({});
        let result = bulk_answer_questions(&db, &args, &crate::ProgressReporter::Silent);
        assert!(result.is_err());
    }

    #[test]
    fn test_answer_questions_dispatches_to_service() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"doc_id": "abc", "question_index": 0, "answer": "yes"});
        // Will fail because doc doesn't exist, but it should dispatch correctly
        let result = answer_questions(&db, &args, &crate::ProgressReporter::Silent);
        assert!(result.is_err()); // doc not found
    }
}
