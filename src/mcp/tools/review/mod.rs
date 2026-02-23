//! Review queue MCP tools.
//!
//! This module provides MCP tools for managing review questions:
//!
//! # Module Organization
//!
//! - `queue` - Review queue retrieval (`get_review_queue`)
//! - `answer` - Question answering (`answer_question`, `bulk_answer_questions`)
//! - `generate` - Question generation (`generate_questions`)
//!
//! # Public API
//!
//! All 4 public functions are re-exported from this module:
//! - `get_review_queue` - Get pending review questions
//! - `answer_question` - Answer a single question
//! - `bulk_answer_questions` - Answer multiple questions atomically
//! - `generate_questions` - Generate review questions for a document

mod answer;
mod generate;
mod queue;

pub use answer::{answer_question, bulk_answer_questions};
pub use generate::generate_questions;
pub use queue::get_review_queue;

use crate::models::ReviewQuestion;
use serde_json::Value;

/// Formats a review question as JSON. When `doc_context` is provided,
/// includes doc_id, doc_title, answered, and answer fields.
pub(crate) fn format_question_json(q: &ReviewQuestion, doc_context: Option<(&str, &str)>) -> Value {
    let mut json = q.to_json();
    if let Some((doc_id, doc_title)) = doc_context {
        let obj = json.as_object_mut().unwrap();
        obj.insert("doc_id".to_string(), Value::String(doc_id.to_string()));
        obj.insert(
            "doc_title".to_string(),
            Value::String(doc_title.to_string()),
        );
        obj.insert("answered".to_string(), Value::Bool(q.answered));
        obj.insert("answer".to_string(), serde_json::json!(q.answer));
    }
    json
}
