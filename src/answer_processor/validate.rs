//! Output validation for review answer application.
//!
//! Validates rewritten sections and full documents before writing to disk,
//! preventing document corruption from malformed output.

use crate::output::truncate_str;
use crate::patterns::{FACT_LINE_REGEX, SOURCE_DEF_REGEX};

/// Errors detected during output validation.
#[derive(Debug, Clone)]
pub struct ValidationError {
    #[allow(dead_code)] // used in tests for assertion matching
    pub kind: ValidationErrorKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorKind {
    TitleCorrupted,
    HeaderLost,
    ContentLoss,
    MalformedFootnote,
    MetaTextDetected,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

/// Patterns that indicate raw LLM meta-text leaked into output.
const META_PATTERNS: &[&str] = &[
    "```json",
    "```yaml",
    "\"question_type\":",
    "\"instruction\":",
    "\"change_instruction\":",
    "ChangeInstruction::",
    "AnswerType::",
    "CLASSIFICATION:",
    "ANSWER_TYPE:",
    "OUTPUT:",
    "REWRITTEN SECTION:",
];

/// Shared validation checks: meta-text and footnote definitions.
fn check_common(text: &str, errors: &mut Vec<ValidationError>) {
    check_meta_text(text, errors);
    check_footnote_definitions(text, errors);
}

/// Validate a full document before writing to disk.
///
/// Compares new content against original to catch corruption.
/// Returns a list of validation errors (empty = valid).
pub fn validate_document(original: &str, new_content: &str) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Factbase ID header must be preserved
    let orig_id = extract_header_id(original);
    let new_id = extract_header_id(new_content);
    if let Some(oid) = &orig_id {
        if new_id.as_ref() != Some(oid) {
            errors.push(ValidationError {
                kind: ValidationErrorKind::HeaderLost,
                detail: format!(
                    "Factbase header lost or changed: was '{}', now '{}'",
                    oid,
                    new_id.unwrap_or_default()
                ),
            });
        }
    }

    // Title must be preserved (first # heading)
    let orig_title = crate::patterns::extract_heading_title(original);
    let new_title = crate::patterns::extract_heading_title(new_content);
    if let Some(ot) = &orig_title {
        match &new_title {
            None => {
                errors.push(ValidationError {
                    kind: ValidationErrorKind::TitleCorrupted,
                    detail: format!("Document title lost: was '{}'", ot),
                });
            }
            Some(nt) if nt != ot => {
                errors.push(ValidationError {
                    kind: ValidationErrorKind::TitleCorrupted,
                    detail: format!("Document title changed: '{}' → '{}'", ot, nt),
                });
            }
            _ => {}
        }
    }

    // Fact count should not decrease dramatically
    let orig_facts = count_fact_lines(original);
    let new_facts = count_fact_lines(new_content);
    if orig_facts > 2 && new_facts < orig_facts / 2 {
        errors.push(ValidationError {
            kind: ValidationErrorKind::ContentLoss,
            detail: format!(
                "Fact lines dropped from {} to {} (>50% loss)",
                orig_facts, new_facts
            ),
        });
    }

    check_common(new_content, &mut errors);
    errors
}

fn extract_header_id(content: &str) -> Option<String> {
    // Delegate to DocumentProcessor which handles both HTML comment and YAML frontmatter
    crate::processor::DocumentProcessor::extract_id_static(content)
}

fn count_fact_lines(content: &str) -> usize {
    // Only count fact lines in the document body, excluding the review queue
    // section. Review queue items (e.g. `- [x] @q[temporal] ...`) match the
    // fact-line regex but are not actual facts — removing answered questions
    // would otherwise trigger a false-positive content-loss validation error.
    // Use body_end_offset which handles both plain and callout-wrapped review sections.
    let end = crate::patterns::body_end_offset(content);
    content[..end]
        .lines()
        .filter(|l| FACT_LINE_REGEX.is_match(l))
        .count()
}

fn check_meta_text(text: &str, errors: &mut Vec<ValidationError>) {
    for pattern in META_PATTERNS {
        if text.contains(pattern) {
            errors.push(ValidationError {
                kind: ValidationErrorKind::MetaTextDetected,
                detail: format!("Meta-text detected in output: '{}'", pattern),
            });
            return; // One meta-text error is enough
        }
    }
}

fn check_footnote_definitions(text: &str, errors: &mut Vec<ValidationError>) {
    for line in text.lines() {
        let trimmed = line.trim();
        // Lines that look like they're trying to be footnote defs but are malformed
        if trimmed.starts_with("[^") && trimmed.contains(']') && !SOURCE_DEF_REGEX.is_match(trimmed)
        {
            // Allow footnote references in running text (e.g. "fact [^1]")
            // Only flag lines that start with [^ — those should be definitions
            if trimmed.starts_with("[^") {
                errors.push(ValidationError {
                    kind: ValidationErrorKind::MalformedFootnote,
                    detail: format!("Malformed footnote definition: '{}'", truncate_str(trimmed, 80)),
                });
                return; // One is enough
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== validate_document tests ====================

    #[test]
    fn test_valid_document_passes() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme\n";
        let new_content = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme @t[2020..]\n";
        let errors = validate_document(original, new_content);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_document_header_lost() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme\n";
        let new_content = "# John Doe\n\n- VP at Acme\n";
        let errors = validate_document(original, new_content);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::HeaderLost));
    }

    #[test]
    fn test_document_title_changed() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme\n";
        let new_content =
            "<!-- factbase:abc123 -->\n# REWRITTEN SECTION\n\n- VP at Acme\n";
        let errors = validate_document(original, new_content);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::TitleCorrupted));
    }

    #[test]
    fn test_document_title_lost() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme\n";
        let new_content = "<!-- factbase:abc123 -->\n\n- VP at Acme\n";
        let errors = validate_document(original, new_content);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::TitleCorrupted));
    }

    #[test]
    fn test_document_fact_loss() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- Fact 1\n- Fact 2\n- Fact 3\n- Fact 4\n- Fact 5\n- Fact 6\n";
        let new_content = "<!-- factbase:abc123 -->\n# John Doe\n\n- Fact 1\n";
        let errors = validate_document(original, new_content);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss));
    }

    #[test]
    fn test_document_meta_text_in_output() {
        let original = "<!-- factbase:abc123 -->\n# John Doe\n\n- VP at Acme\n";
        let new_content =
            "<!-- factbase:abc123 -->\n# John Doe\n\nOUTPUT:\n- VP at Acme @t[2020..]\n";
        let errors = validate_document(original, new_content);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::MetaTextDetected));
    }

    #[test]
    fn test_document_no_original_header_skips_check() {
        // Documents without headers (rare) should not fail header check
        let original = "# Notes\n\n- Some fact\n";
        let new_content = "# Notes\n\n- Some fact @t[2024]\n";
        let errors = validate_document(original, new_content);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_document_no_original_title_skips_check() {
        let original = "<!-- factbase:abc123 -->\n\nSome content\n";
        let new_content = "<!-- factbase:abc123 -->\n\nUpdated content\n";
        let errors = validate_document(original, new_content);
        assert!(!errors.iter().any(|e| e.kind == ValidationErrorKind::TitleCorrupted));
    }

    #[test]
    fn test_removing_review_questions_not_counted_as_fact_loss() {
        // Simulates the bug: document with facts + many review queue items.
        // After applying answers, the review queue items are removed.
        // This should NOT trigger content loss.
        let original = "<!-- factbase:abc123 -->\n# Battle of Actium\n\n\
            - Octavian defeated Antony in 31 BCE\n\
            - Naval battle near Greece\n\
            - Resulted in end of Roman Republic\n\
            \n---\n\n## Review Queue\n<!-- factbase:review -->\n\
            - [x] @q[temporal] When exactly did the battle occur? > 2 September 31 BCE\n\
            - [x] @q[missing] What sources confirm this? > Plutarch, Cassius Dio\n\
            - [x] @q[temporal] When did Antony flee? > During the battle\n\
            - [x] @q[conflict] Was it 31 or 30 BCE? > 31 BCE is correct\n\
            - [x] @q[stale] Is this still accurate? > Yes\n\
            - [x] @q[ambiguous] Which Octavian? > Gaius Octavius, later Augustus\n";
        // After applying: review questions removed, facts preserved
        let new_content = "<!-- factbase:abc123 -->\n# Battle of Actium\n\n\
            - Octavian defeated Antony in 31 BCE\n\
            - Naval battle near Greece\n\
            - Resulted in end of Roman Republic\n";
        let errors = validate_document(original, new_content);
        assert!(
            !errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss),
            "Removing review questions should not trigger content loss: {:?}",
            errors
        );
    }

    #[test]
    fn test_real_fact_loss_still_detected_with_review_queue() {
        // Even with a review queue, actual fact loss should still be caught
        let original = "<!-- factbase:abc123 -->\n# Topic\n\n\
            - Fact 1\n- Fact 2\n- Fact 3\n- Fact 4\n- Fact 5\n- Fact 6\n\
            \n---\n\n## Review Queue\n<!-- factbase:review -->\n\
            - [x] @q[temporal] Question? > Answer\n";
        // Both facts AND review questions lost
        let new_content = "<!-- factbase:abc123 -->\n# Topic\n\n- Fact 1\n";
        let errors = validate_document(original, new_content);
        assert!(
            errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss),
            "Real fact loss should still be detected"
        );
    }

    #[test]
    fn test_count_fact_lines_excludes_review_queue() {
        let content = "# Doc\n\n\
            - Fact A\n\
            - Fact B\n\
            \n---\n\n## Review Queue\n<!-- factbase:review -->\n\
            - [x] @q[temporal] Q1? > A1\n\
            - [x] @q[conflict] Q2? > A2\n";
        assert_eq!(count_fact_lines(content), 2);
    }

    #[test]
    fn test_count_fact_lines_excludes_callout_review_queue() {
        let content = "# Doc\n\n\
            - Fact A\n\
            - Fact B\n\
            \n> [!info]- Review Queue\n> <!-- factbase:review -->\n\
            > - [x] @q[temporal] Q1? > A1\n\
            > - [x] @q[conflict] Q2? > A2\n";
        assert_eq!(count_fact_lines(content), 2);
    }

    #[test]
    fn test_validate_document_callout_review_no_false_content_loss() {
        let original = "<!-- factbase:abc123 -->\n# Topic\n\n\
            - Fact 1\n- Fact 2\n- Fact 3\n\
            \n> [!info]- Review Queue\n> <!-- factbase:review -->\n\
            > - [x] @q[temporal] Q? > A\n";
        // Remove answered review question but keep all facts
        let new_content = "<!-- factbase:abc123 -->\n# Topic\n\n\
            - Fact 1\n- Fact 2\n- Fact 3\n";
        let errors = validate_document(original, new_content);
        assert!(
            !errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss),
            "Removing callout review questions should not trigger content loss"
        );
    }
}
