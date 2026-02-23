//! Core lint execution logic.
//!
//! This module contains helper functions extracted from `cmd_check` to improve
//! code organization and maintainability.
//!
//! # Module Organization
//!
//! - `aggregate` - Aggregate checks across multiple documents (duplicates)
//! - `basics` - Basic document property checks (stub, type, stale)
//! - `links` - Link checking (orphan, broken)
//! - `review` - Review question generation
//! - `sources` - Source footnote validation
//! - `temporal` - Temporal tag validation

mod aggregate;
mod basics;
mod links;
mod review;
mod sources;
mod temporal;

#[cfg(test)]
pub(crate) mod test_helpers;

// Re-export all public items for backward compatibility
pub use aggregate::check_duplicates;
pub use basics::check_document_basics;
pub use links::check_document_links;
pub use review::generate_review_questions;
pub use sources::check_source_refs;
pub use temporal::check_temporal_tags;

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
