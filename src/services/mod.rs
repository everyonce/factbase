//! Transport-independent service layer.
//!
//! Business logic shared between MCP tools and web API endpoints.
//! Services accept typed parameters and return `Result<Value, FactbaseError>`.
//! Transport layers (MCP, HTTP) handle arg parsing and response formatting.

pub mod entity;
pub mod review;
pub mod status;

pub use entity::{
    get_document_stats, get_entity, get_perspective, list_entities, list_repositories,
};
pub use review::{
    answer_question, answer_questions, bulk_answer_questions, get_deferred_items, get_review_queue,
    reset_deferred_questions, AnswerQuestionParams, BulkAnswerItem as ServiceBulkAnswerItem,
    ReviewQueueParams,
};
pub use status::kb_status;
