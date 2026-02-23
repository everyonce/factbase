//! Document validation for import operations.

use factbase::{parse_source_definitions, parse_source_references, validate_temporal_tags};
use std::collections::HashSet;

/// Validation error for an imported document
#[derive(Debug)]
pub struct ImportValidationError {
    pub filename: String,
    pub errors: Vec<String>,
}

/// Validate a document's content before import.
/// Returns None if valid, Some(errors) if invalid.
pub fn validate_import_document(content: &str, filename: &str) -> Option<ImportValidationError> {
    // Pre-allocate for typical case of ~4 validation errors per document
    let mut errors = Vec::with_capacity(4);

    // Check 1: Valid factbase ID header format (if present)
    let first_line = content.lines().next().unwrap_or("");
    if first_line.starts_with("<!-- factbase:") {
        // Extract ID and validate format (6 hex chars)
        if let Some(start) = first_line.find("factbase:") {
            let after_prefix = &first_line[start + 9..];
            if let Some(end) = after_prefix.find(" -->") {
                let id = &after_prefix[..end];
                if id.len() != 6 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
                    errors.push(format!(
                        "Invalid factbase ID format '{id}' (expected 6 hex characters)"
                    ));
                }
            } else {
                errors.push("Malformed factbase header (missing closing -->)".to_string());
            }
        }
    }

    // Check 2: Valid temporal tag syntax (if present)
    let temporal_errors = validate_temporal_tags(content);
    for err in temporal_errors {
        errors.push(format!(
            "Line {}: Invalid temporal tag '{}' - {}",
            err.line_number, err.raw_text, err.message
        ));
    }

    // Check 3: Valid source footnote format (orphan refs/defs)
    let refs = parse_source_references(content);
    let defs = parse_source_definitions(content);

    let ref_numbers: HashSet<u32> = refs.iter().map(|r| r.number).collect();
    let def_numbers: HashSet<u32> = defs.iter().map(|d| d.number).collect();

    // Find orphan references (refs without definitions)
    for r in &refs {
        if !def_numbers.contains(&r.number) {
            errors.push(format!(
                "Line {}: Orphan reference [^{}] (no definition found)",
                r.line_number, r.number
            ));
        }
    }

    // Find orphan definitions (defs without references)
    for d in &defs {
        if !ref_numbers.contains(&d.number) {
            errors.push(format!(
                "Line {}: Orphan definition [^{}] (never referenced)",
                d.line_number, d.number
            ));
        }
    }

    if errors.is_empty() {
        None
    } else {
        Some(ImportValidationError {
            filename: filename.to_string(),
            errors,
        })
    }
}

/// Extract factbase ID from document content.
pub fn extract_factbase_id(content: &str) -> Option<String> {
    let first_line = content.lines().next()?;
    if first_line.starts_with("<!-- factbase:") {
        let start = first_line.find("factbase:")? + 9;
        let after_prefix = &first_line[start..];
        let end = after_prefix.find(" -->")?;
        let id = &after_prefix[..end];
        if id.len() == 6 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(id.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_import_document_valid() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test Document\n\nSome content here.";
        assert!(validate_import_document(content, "test.md").is_none());
    }

    #[test]
    fn test_validate_import_document_invalid_id_format() {
        let content = "<!-- factbase:invalid -->\n# Test Document";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err.errors[0].contains("Invalid factbase ID format"));
    }

    #[test]
    fn test_validate_import_document_invalid_id_too_short() {
        let content = "<!-- factbase:abc -->\n# Test Document";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err.errors[0].contains("Invalid factbase ID format"));
    }

    #[test]
    fn test_validate_import_document_malformed_header() {
        let content = "<!-- factbase:a1b2c3\n# Test Document";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err.errors[0].contains("Malformed factbase header"));
    }

    #[test]
    fn test_validate_import_document_invalid_temporal_tag() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test\n\n- Fact @t[2024-13]";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err
            .errors
            .iter()
            .any(|e| e.contains("Invalid temporal tag")));
    }

    #[test]
    fn test_validate_import_document_orphan_reference() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test\n\n- Fact [^1]";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err.errors.iter().any(|e| e.contains("Orphan reference")));
    }

    #[test]
    fn test_validate_import_document_orphan_definition() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test\n\n- Fact\n\n[^1]: Source";
        let result = validate_import_document(content, "test.md");
        assert!(result.is_some());
        let err = result.unwrap();
        assert!(err.errors.iter().any(|e| e.contains("Orphan definition")));
    }

    #[test]
    fn test_validate_import_document_valid_with_sources() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test\n\n- Fact [^1]\n\n[^1]: Source";
        assert!(validate_import_document(content, "test.md").is_none());
    }

    #[test]
    fn test_validate_import_document_no_header() {
        // Documents without headers are valid (header will be added on scan)
        let content = "# Test Document\n\nSome content.";
        assert!(validate_import_document(content, "test.md").is_none());
    }

    #[test]
    fn test_extract_factbase_id_valid() {
        let content = "<!-- factbase:a1b2c3 -->\n# Test";
        assert_eq!(extract_factbase_id(content), Some("a1b2c3".to_string()));
    }

    #[test]
    fn test_extract_factbase_id_invalid() {
        let content = "<!-- factbase:invalid -->\n# Test";
        assert_eq!(extract_factbase_id(content), None);
    }

    #[test]
    fn test_extract_factbase_id_no_header() {
        let content = "# Test Document";
        assert_eq!(extract_factbase_id(content), None);
    }
}
