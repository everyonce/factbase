//! Question generation for the review system.
//!
//! Generates review questions for facts:
//! - `@q[temporal]` for facts missing temporal tags or stale ongoing roles
//! - `@q[conflict]` for overlapping date ranges or contradictory facts
//! - `@q[missing]` for facts without source references
//! - `@q[ambiguous]` for unclear phrasing that needs clarification
//! - `@q[stale]` for facts with old sources or old `@t[~...]` dates
//! - `@q[duplicate]` for documents with high similarity to other documents
//!
//! # Module Organization
//!
//! - `temporal` - Temporal question generation (`@q[temporal]`)
//! - `conflict` - Conflict question generation (`@q[conflict]`)
//! - `missing` - Missing source question generation (`@q[missing]`)
//! - `ambiguous` - Ambiguous question generation (`@q[ambiguous]`)
//! - `stale` - Stale question generation (`@q[stale]`)
//! - `duplicate` - Duplicate question generation (`@q[duplicate]`)
//! - `fields` - Field detection and required field questions
//!
//! # Public API
//!
//! ## Question Generators
//! - [`generate_temporal_questions`] - Generate temporal questions
//! - [`generate_conflict_questions`] - Generate conflict questions
//! - [`generate_missing_questions`] - Generate missing source questions
//! - [`generate_source_quality_questions`] - Generate questions for untraceable sources
//! - [`generate_ambiguous_questions`] - Generate ambiguous questions
//! - [`generate_stale_questions`] - Generate stale questions
//! - [`generate_duplicate_questions`] - Generate duplicate questions
//!
//! ## Field Detection
//! - [`detect_document_fields`] - Detect fields in a document
//! - [`generate_required_field_questions`] - Generate missing required field questions

mod ambiguous;
mod conflict;
pub mod cross_validate;
mod duplicate;
pub(crate) mod facts;
mod fields;
pub mod check;
mod missing;
mod stale;
mod temporal;

use crate::patterns::FACT_LINE_REGEX;

// Re-export all public items
pub use ambiguous::generate_ambiguous_questions;
pub use conflict::generate_conflict_questions;
pub use duplicate::generate_duplicate_questions;
pub use fields::{detect_document_fields, generate_required_field_questions};
pub use missing::{generate_missing_questions, generate_source_quality_questions};
pub use stale::generate_stale_questions;
pub use temporal::generate_temporal_questions;

/// Iterate over fact lines in content, yielding `(line_number, line, fact_text)`.
///
/// Filters to list-item lines matching `FACT_LINE_REGEX`, extracts fact text,
/// and skips lines with empty fact text. Line numbers are 1-indexed.
pub(crate) fn iter_fact_lines(content: &str) -> impl Iterator<Item = (usize, &str, String)> {
    // Stop before the review queue section — its content is not document facts
    let end = content
        .find(crate::patterns::REVIEW_QUEUE_MARKER)
        .unwrap_or(content.len());
    let body = &content[..end];
    body.lines().enumerate().filter_map(|(line_idx, line)| {
        if !FACT_LINE_REGEX.is_match(line) {
            return None;
        }
        let fact_text = extract_fact_text(line);
        if fact_text.is_empty() {
            return None;
        }
        Some((line_idx + 1, line, fact_text))
    })
}

/// Extract the fact text from a list item line, removing the list marker.
/// Used by multiple submodules.
pub(crate) fn extract_fact_text(line: &str) -> String {
    let trimmed = line.trim();

    // Remove list markers: -, *, 1., 1)
    let text = if let Some(rest) = trimmed.strip_prefix("- ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("* ") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
        // Handle numbered lists: "1. " or "1) "
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(rest) = rest.strip_prefix(". ") {
            rest
        } else if let Some(rest) = rest.strip_prefix(") ") {
            rest
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    // Truncate long facts for readability
    let text = text.trim();
    if text.len() > 80 {
        let mut end = 77;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &text[..end])
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_fact_text_dash() {
        assert_eq!(extract_fact_text("- Simple fact"), "Simple fact");
    }

    #[test]
    fn test_extract_fact_text_asterisk() {
        assert_eq!(extract_fact_text("* Another fact"), "Another fact");
    }

    #[test]
    fn test_extract_fact_text_numbered_dot() {
        assert_eq!(extract_fact_text("1. Numbered fact"), "Numbered fact");
    }

    #[test]
    fn test_extract_fact_text_numbered_paren() {
        assert_eq!(extract_fact_text("2) Paren fact"), "Paren fact");
    }

    #[test]
    fn test_extract_fact_text_indented() {
        assert_eq!(extract_fact_text("  - Indented fact"), "Indented fact");
    }

    #[test]
    fn test_extract_fact_text_truncates_long() {
        let long_fact = "- ".to_string() + &"x".repeat(100);
        let result = extract_fact_text(&long_fact);
        assert!(result.len() <= 80);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_iter_fact_lines_basic() {
        let content = "# Title\n\nParagraph\n\n- Fact one\n- Fact two\n* Fact three";
        let results: Vec<_> = iter_fact_lines(content).collect();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 5); // line number
        assert_eq!(results[0].2, "Fact one"); // fact text
        assert_eq!(results[1].0, 6);
        assert_eq!(results[2].0, 7);
    }

    #[test]
    fn test_iter_fact_lines_skips_non_facts() {
        let content = "# Title\n\nParagraph text\n\n- Only fact";
        let results: Vec<_> = iter_fact_lines(content).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 5);
    }

    #[test]
    fn test_iter_fact_lines_preserves_raw_line() {
        let content = "- Fact with @t[2024..] tag";
        let results: Vec<_> = iter_fact_lines(content).collect();
        assert_eq!(results[0].1, "- Fact with @t[2024..] tag");
    }

    #[test]
    fn test_extract_fact_text_multibyte_truncation() {
        // The en-dash '–' is 3 bytes; ensure truncation doesn't panic
        let long_fact = "- Participant in GenAI EBA - Physician Advisor weekly sync (every Wednesday, 3–4 PM CT) @t[2026-01..] [^1]";
        let result = extract_fact_text(long_fact);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 83); // 80 chars + "..."
    }
}
