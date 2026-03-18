//! Answer processing for review --apply command.
//!
//! Processes answered review questions and applies changes to documents.
//!
//! # Module Organization
//!
//! - `interpret` - Answer interpretation and change instruction generation
//! - `temporal` - Date extraction and temporal tag formatting
//! - `apply` - Change application to document sections
//!
//! # Public API
//!
//! ## Types
//! - [`ChangeInstruction`] - Enum of possible change types
//! - [`InterpretedAnswer`] - Interpreted answer with instruction
//!
//! ## Functions
//! - [`interpret_answer`] - Interpret an answer to determine change instruction
//! - [`apply_changes_to_section`] - Apply changes to a section (deletes only; complex changes return error)
//! - [`identify_affected_section`] - Find document section affected by questions
//! - [`replace_section`] - Replace section in document content
//! - [`remove_processed_questions`] - Remove processed questions from review queue
//! - [`uncheck_deferred_questions`] - Uncheck deferred questions (keep in queue)

mod apply;
pub mod apply_all;
pub mod inbox;
mod interpret;
mod temporal;
pub(crate) mod validate;

use crate::ReviewQuestion;

// Re-export public API
pub use apply::{
    apply_changes_to_section, apply_confirmations, apply_source_citations, dedup_titles,
    identify_affected_section, remove_processed_questions, replace_section,
    stamp_citation_accepted, stamp_reviewed_by_text, stamp_reviewed_lines, stamp_reviewed_markers,
    stamp_sequential_by_text, stamp_sequential_lines, uncheck_deferred_questions,
    CITATION_ACCEPTED_MARKER,
};
pub use interpret::{classify_answer, interpret_answer};

/// Classified answer type for deterministic handling.
///
/// Only `Correction` (and complex cases) need LLM rewrite.
/// `SourceCitation` and `Confirmation` can be handled deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerType {
    /// "dismiss", "ignore" — remove question, no changes
    Dismissal,
    /// "defer", "later", "needs ..." — keep question, mark deferred
    Deferral,
    /// Source name + optional date — add/update footnote and temporal tag
    SourceCitation {
        source: String,
        date: Option<String>,
    },
    /// "confirmed", "still accurate", "yes" — refresh last-seen date
    Confirmation,
    /// "correct: ..." or explicit correction — LLM rewrite
    Correction { detail: String },
    /// "delete", "remove" — remove the fact line
    Deletion,
}

/// Represents a change instruction for the LLM
#[derive(Debug, Clone)]
pub enum ChangeInstruction {
    /// Remove question without changes
    Dismiss,
    /// Keep question, mark deferred (uncheck checkbox)
    Defer,
    /// Delete the referenced line
    Delete { line_text: String },
    /// Update temporal tag
    UpdateTemporal {
        line_text: String,
        old_tag: String,
        new_tag: String,
    },
    /// Split fact into multiple lines
    Split {
        line_text: String,
        instruction: String,
    },
    /// Add temporal tag to fact without one
    AddTemporal { line_text: String, tag: String },
    /// Add source reference
    AddSource {
        line_text: String,
        source_info: String,
    },
    /// Generic change that needs LLM interpretation
    Generic { description: String },
}

/// Result of interpreting an answer
#[derive(Debug, Clone)]
pub struct InterpretedAnswer {
    pub question: ReviewQuestion,
    pub instruction: ChangeInstruction,
}
