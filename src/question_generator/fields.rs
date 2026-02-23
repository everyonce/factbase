//! Field detection and required field question generation.
//!
//! Detects fields present in documents and generates `@q[missing]` questions
//! for required fields that are not found.

use std::collections::{HashMap, HashSet};

use crate::models::{QuestionType, ReviewQuestion};
use crate::patterns::{FIELD_VALUE_REGEX, SECTION_HEADING_REGEX};

/// Detect fields present in a document.
///
/// Fields are detected via:
/// 1. `## Field Name` section headings (normalized to lowercase with underscores)
/// 2. `- field_name: value` patterns in list items (normalized to lowercase with underscores)
///
/// Returns a set of normalized field names found in the document.
pub fn detect_document_fields(content: &str) -> HashSet<String> {
    let mut fields = HashSet::new();

    for line in content.lines() {
        // Check for section headings: ## Field Name
        if let Some(caps) = SECTION_HEADING_REGEX.captures(line) {
            let field_name = normalize_field_name(&caps[1]);
            if !field_name.is_empty() {
                fields.insert(field_name);
            }
        }

        // Check for field-value patterns: - field_name: value
        if let Some(caps) = FIELD_VALUE_REGEX.captures(line) {
            let field_name = normalize_field_name(&caps[1]);
            if !field_name.is_empty() {
                fields.insert(field_name);
            }
        }
    }

    fields
}

/// Normalize a field name to lowercase with underscores.
/// "Current Role" -> "current_role"
/// "current_role" -> "current_role"
fn normalize_field_name(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Generate missing required field questions for a document.
///
/// Compares detected fields against required fields for the document type.
/// Generates `@q[missing]` questions for each required field not found.
///
/// # Arguments
/// * `content` - Document content
/// * `doc_type` - Document type (e.g., "person", "project")
/// * `required_fields` - Map of doc_type -> list of required field names
///
/// Returns a list of `ReviewQuestion` with `question_type = Missing`.
pub fn generate_required_field_questions(
    content: &str,
    doc_type: Option<&str>,
    required_fields: &HashMap<String, Vec<String>>,
) -> Vec<ReviewQuestion> {
    let mut questions = Vec::new();

    // Get required fields for this document type
    let doc_type = match doc_type {
        Some(t) => t.to_lowercase(),
        None => return questions, // No type, no required fields
    };

    let required = match required_fields.get(&doc_type) {
        Some(fields) => fields,
        None => return questions, // No required fields for this type
    };

    // Detect fields present in document
    let present_fields = detect_document_fields(content);

    // Generate questions for missing required fields
    for field in required {
        let normalized = normalize_field_name(field);
        if !present_fields.contains(&normalized) {
            questions.push(ReviewQuestion::new(
                QuestionType::Missing,
                None,
                format!(
                    "Required field \"{}\" is missing for {} document - please add",
                    field, doc_type
                ),
            ));
        }
    }

    questions
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_detect_document_fields_section_headings() {
        let content = "# Person\n\n## Current Role\n\nSome content\n\n## Location\n\nMore content";
        let fields = detect_document_fields(content);
        assert!(fields.contains("current_role"));
        assert!(fields.contains("location"));
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn test_detect_document_fields_field_value_patterns() {
        let content = "# Person\n\n- current_role: Engineer\n- location: NYC";
        let fields = detect_document_fields(content);
        assert!(fields.contains("current_role"));
        assert!(fields.contains("location"));
    }

    #[test]
    fn test_detect_document_fields_mixed_formats() {
        let content = "# Person\n\n## Current Role\n\n- location: NYC\n- Company: Acme";
        let fields = detect_document_fields(content);
        assert!(fields.contains("current_role"));
        assert!(fields.contains("location"));
        assert!(fields.contains("company"));
    }

    #[test]
    fn test_detect_document_fields_normalizes_names() {
        let content = "# Person\n\n## Current Role\n\n- Current-Role: duplicate\n- LOCATION: NYC";
        let fields = detect_document_fields(content);
        // Both "Current Role" and "Current-Role" normalize to "current_role"
        assert!(fields.contains("current_role"));
        assert!(fields.contains("location"));
    }

    #[test]
    fn test_detect_document_fields_empty_content() {
        let content = "";
        let fields = detect_document_fields(content);
        assert!(fields.is_empty());
    }

    #[test]
    fn test_detect_document_fields_no_fields() {
        let content = "# Person\n\nJust some text without fields.";
        let fields = detect_document_fields(content);
        assert!(fields.is_empty());
    }

    #[test]
    fn test_normalize_field_name() {
        assert_eq!(normalize_field_name("Current Role"), "current_role");
        assert_eq!(normalize_field_name("current_role"), "current_role");
        assert_eq!(normalize_field_name("Current-Role"), "current_role");
        assert_eq!(normalize_field_name("  Location  "), "location");
        assert_eq!(normalize_field_name("COMPANY"), "company");
    }

    #[test]
    fn test_generate_required_field_questions_no_type() {
        let content = "# Person\n\n- Works at Acme";
        let required: HashMap<String, Vec<String>> = HashMap::new();
        let questions = generate_required_field_questions(content, None, &required);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_required_field_questions_no_required_for_type() {
        let content = "# Person\n\n- Works at Acme";
        let required: HashMap<String, Vec<String>> = HashMap::new();
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_required_field_questions_all_present() {
        let content = "# Person\n\n## Current Role\n\nEngineer\n\n## Location\n\nNYC";
        let mut required = HashMap::new();
        required.insert(
            "person".to_string(),
            vec!["current_role".to_string(), "location".to_string()],
        );
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_required_field_questions_missing_one() {
        let content = "# Person\n\n## Current Role\n\nEngineer";
        let mut required = HashMap::new();
        required.insert(
            "person".to_string(),
            vec!["current_role".to_string(), "location".to_string()],
        );
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question_type, QuestionType::Missing);
        assert!(questions[0].description.contains("location"));
        assert!(questions[0].description.contains("person"));
    }

    #[test]
    fn test_generate_required_field_questions_missing_all() {
        let content = "# Person\n\nJust some text.";
        let mut required = HashMap::new();
        required.insert(
            "person".to_string(),
            vec!["current_role".to_string(), "location".to_string()],
        );
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert_eq!(questions.len(), 2);
    }

    #[test]
    fn test_generate_required_field_questions_case_insensitive_type() {
        let content = "# Person\n\nJust some text.";
        let mut required = HashMap::new();
        required.insert("person".to_string(), vec!["location".to_string()]);
        // Type is "Person" but required_fields has "person"
        let questions = generate_required_field_questions(content, Some("Person"), &required);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_generate_required_field_questions_field_value_format() {
        let content = "# Person\n\n- current_role: Engineer\n- location: NYC";
        let mut required = HashMap::new();
        required.insert(
            "person".to_string(),
            vec!["current_role".to_string(), "location".to_string()],
        );
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert!(questions.is_empty());
    }

    #[test]
    fn test_generate_required_field_questions_line_ref_is_none() {
        let content = "# Person\n\nJust some text.";
        let mut required = HashMap::new();
        required.insert("person".to_string(), vec!["location".to_string()]);
        let questions = generate_required_field_questions(content, Some("person"), &required);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].line_ref.is_none()); // Missing fields apply to whole doc
    }
}
