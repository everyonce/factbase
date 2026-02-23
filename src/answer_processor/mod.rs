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
//! - [`format_changes_for_llm`] - Format instructions for LLM prompt
//! - [`build_rewrite_prompt`] - Build LLM prompt for section rewriting
//! - [`apply_changes_to_section`] - Apply changes using LLM
//! - [`identify_affected_section`] - Find document section affected by questions
//! - [`replace_section`] - Replace section in document content
//! - [`remove_processed_questions`] - Remove processed questions from review queue

mod apply;
pub mod inbox;
mod interpret;
mod temporal;

use crate::ReviewQuestion;

// Re-export public API
pub use apply::{
    apply_changes_to_section, build_rewrite_prompt, format_changes_for_llm,
    identify_affected_section, remove_processed_questions, replace_section,
};
pub use interpret::interpret_answer;

/// Represents a change instruction for the LLM
#[derive(Debug, Clone)]
pub enum ChangeInstruction {
    /// Remove question without changes
    Dismiss,
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
#[derive(Debug)]
pub struct InterpretedAnswer {
    pub question: ReviewQuestion,
    pub instruction: ChangeInstruction,
}
