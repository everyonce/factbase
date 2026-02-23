//! Core lint execution logic.
//!
//! # Module Organization
//!
//! - `aggregate` - Aggregate checks across multiple documents (duplicates)
//! - `links` - Link checking (orphan, broken)
//! - `review` - Review question generation

mod aggregate;
mod links;
mod review;

pub use aggregate::check_duplicates;
pub use links::check_document_links;
pub use review::generate_review_questions;

/// Result of linting a single document for links.
pub struct LinkCheckResult {
    pub warnings: usize,
    pub errors: usize,
    pub fixed: usize,
    pub broken_links: Vec<String>,
}

/// Options for review question generation.
pub struct ReviewQuestionOptions {
    pub min_similarity: f32,
    pub dry_run: bool,
    pub export_mode: bool,
    pub is_table_format: bool,
    pub max_age: Option<i64>,
}
