//! Output validation for apply_review_answers.
//!
//! Validates LLM-rewritten sections and full documents before writing to disk,
//! preventing document corruption from malformed LLM output.

use crate::patterns::{FACT_LINE_REGEX, ID_REGEX, SOURCE_DEF_REGEX};

/// Errors detected during output validation.
#[derive(Debug, Clone)]
pub struct ValidationError {
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

/// Validate an LLM-rewritten section against the original.
///
/// Returns a list of validation errors (empty = valid).
pub fn validate_rewrite(original: &str, rewritten: &str) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // 1. Check for dramatic content loss (>50% line reduction)
    let orig_lines = content_line_count(original);
    let new_lines = content_line_count(rewritten);
    if orig_lines > 2 && new_lines < orig_lines / 2 {
        errors.push(ValidationError {
            kind: ValidationErrorKind::ContentLoss,
            detail: format!(
                "Content lines dropped from {} to {} (>50% loss)",
                orig_lines, new_lines
            ),
        });
    }

    // 2. Check for meta-text / JSON / classification labels
    check_meta_text(rewritten, &mut errors);

    // 3. Check that any footnote definitions in the output are well-formed
    check_footnote_definitions(rewritten, &mut errors);

    errors
}

/// Validate a full document before writing to disk.
///
/// Compares new content against original to catch corruption.
/// Returns a list of validation errors (empty = valid).
pub fn validate_document(original: &str, new_content: &str) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // 1. Factbase ID header must be preserved
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

    // 2. Title must be preserved (first # heading)
    let orig_title = extract_title(original);
    let new_title = extract_title(new_content);
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

    // 3. Fact count should not decrease dramatically
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

    // 4. Check for meta-text in the output
    check_meta_text(new_content, &mut errors);

    // 5. Check footnote definitions are well-formed
    check_footnote_definitions(new_content, &mut errors);

    errors
}

fn extract_header_id(content: &str) -> Option<String> {
    content
        .lines()
        .next()
        .and_then(|line| ID_REGEX.captures(line))
        .map(|cap| cap[1].to_string())
}

fn extract_title(content: &str) -> Option<String> {
    content
        .lines()
        .find(|l| l.starts_with("# ") && !l.starts_with("## "))
        .map(|l| crate::patterns::clean_title(&l[2..]))
}

fn content_line_count(text: &str) -> usize {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("<!--")
        })
        .count()
}

fn count_fact_lines(content: &str) -> usize {
    content
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
                    detail: format!("Malformed footnote definition: '{}'", truncate(trimmed, 80)),
                });
                return; // One is enough
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== validate_rewrite tests ====================

    #[test]
    fn test_valid_rewrite_passes() {
        let original = "## Career\n- VP at Acme @t[2020..2023]\n- CTO at BigCo @t[2023..]\n";
        let rewritten =
            "## Career\n- VP at Acme @t[2020..2023-06]\n- CTO at BigCo @t[2023..]\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_rewrite_content_loss_detected() {
        let original =
            "## Career\n- Fact 1\n- Fact 2\n- Fact 3\n- Fact 4\n- Fact 5\n- Fact 6\n";
        let rewritten = "## Career\n- Fact 1\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss));
    }

    #[test]
    fn test_rewrite_meta_text_detected() {
        let original = "## Career\n- VP at Acme\n";
        let rewritten = "## Career\n- VP at Acme\n```json\n{\"instruction\": \"update\"}\n```\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::MetaTextDetected));
    }

    #[test]
    fn test_rewrite_classification_label_detected() {
        let original = "## Career\n- VP at Acme\n";
        let rewritten = "CLASSIFICATION: correction\n## Career\n- VP at Acme @t[2020..2023]\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(errors.iter().any(|e| e.kind == ValidationErrorKind::MetaTextDetected));
    }

    #[test]
    fn test_rewrite_malformed_footnote_detected() {
        let original = "## Career\n- VP at Acme\n";
        let rewritten = "## Career\n- VP at Acme\n[^1] This is not a proper definition\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(
            errors.iter().any(|e| e.kind == ValidationErrorKind::MalformedFootnote),
            "Expected malformed footnote error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_rewrite_wellformed_footnote_passes() {
        let original = "## Career\n- VP at Acme [^1]\n";
        let rewritten = "## Career\n- VP at Acme [^1]\n[^1]: LinkedIn profile\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(
            !errors.iter().any(|e| e.kind == ValidationErrorKind::MalformedFootnote),
            "Well-formed footnote should pass: {:?}",
            errors
        );
    }

    #[test]
    fn test_small_section_no_false_positive_content_loss() {
        let original = "## Notes\n- One fact\n";
        let rewritten = "## Notes\n- Updated fact\n";
        let errors = validate_rewrite(original, rewritten);
        assert!(
            !errors.iter().any(|e| e.kind == ValidationErrorKind::ContentLoss),
            "Small sections should not trigger content loss: {:?}",
            errors
        );
    }

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
}
