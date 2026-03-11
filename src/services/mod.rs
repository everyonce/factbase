//! Transport-independent service layer.
//!
//! Business logic shared between MCP tools and web API endpoints.
//! Services accept typed parameters and return `Result<Value, FactbaseError>`.
//! Transport layers (MCP, HTTP) handle arg parsing and response formatting.

pub mod entity;
pub mod review;

pub use entity::{get_entity, get_perspective, get_document_stats, list_entities, list_repositories};
pub use review::{
    answer_question, answer_questions, bulk_answer_questions, get_deferred_items, get_review_queue,
    AnswerQuestionParams, BulkAnswerItem as ServiceBulkAnswerItem, ReviewQueueParams,
};
