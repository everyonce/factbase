//! Review question parsing, normalization, and management.
//!
//! This module handles parsing `@q[...]` review questions from document content,
//! appending new questions, normalizing review sections, pruning stale questions,
//! and converting between plain and callout formats.
//!
//! # Submodules
//!
//! - `callout` — Callout format detection and conversion
//! - `parse` — Question parsing and description helpers
//! - `normalize` — Section normalization and deduplication
//! - `append` — Question appending, section merging, and recovery
//! - `prune` — Answered/stale question removal

mod append;
mod callout;
mod normalize;
mod parse;
mod prune;

pub use append::{
    append_review_questions, ensure_review_section, merge_duplicate_review_sections,
    recover_review_section,
};
pub use callout::{is_callout_review, unwrap_review_callout, wrap_review_callout};
pub use normalize::normalize_review_section;
pub use parse::{normalize_conflict_desc, parse_review_queue};
pub use prune::{prune_stale_questions, strip_answered_questions};
