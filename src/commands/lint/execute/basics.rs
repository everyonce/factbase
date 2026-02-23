//! Basic document property checking for lint.
//!
//! Checks for stub documents, unknown types, and stale documents.

use chrono::{Duration, Utc};
use factbase::Document;

/// Check basic document properties (stub, type, stale).
pub fn check_document_basics(
    doc: &Document,
    min_length: usize,
    max_age_days: Option<i64>,
    allowed_types: Option<&Vec<String>>,
    is_table_format: bool,
) -> usize {
    let mut warnings = 0;

    // Check for stub documents
    let content_len = doc.content.len();
    if content_len < min_length {
        if is_table_format {
            println!(
                "  WARN: Stub document ({} chars): {} [{}]",
                content_len, doc.title, doc.id
            );
        }
        warnings += 1;
    }

    // Check for unknown types
    if let Some(allowed) = allowed_types {
        let doc_type = doc.doc_type.as_deref().unwrap_or("");
        if !allowed.iter().any(|t| t.to_lowercase() == doc_type) {
            if is_table_format {
                println!(
                    "  WARN: Unknown type '{}': {} [{}]",
                    doc_type, doc.title, doc.id
                );
            }
            warnings += 1;
        }
    }

    // Check for stale documents
    if let Some(max_age) = max_age_days {
        let cutoff = Utc::now() - Duration::days(max_age);
        let doc_date = doc.file_modified_at.unwrap_or(doc.indexed_at);
        if doc_date < cutoff {
            let age_days = (Utc::now() - doc_date).num_days();
            if is_table_format {
                println!(
                    "  WARN: Stale document ({} days old): {} [{}]",
                    age_days, doc.title, doc.id
                );
            }
            warnings += 1;
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::lint::execute::test_helpers::make_test_doc;

    #[test]
    fn test_check_document_basics_stub() {
        let doc = make_test_doc("Short");

        // Should warn about stub (content < 100 chars)
        let warnings = check_document_basics(&doc, 100, None, None, false);
        assert_eq!(warnings, 1);

        // Should not warn if min_length is lower
        let warnings = check_document_basics(&doc, 5, None, None, false);
        assert_eq!(warnings, 0);
    }

    #[test]
    fn test_check_document_basics_unknown_type() {
        let doc = Document {
            content: "x".repeat(200),
            doc_type: Some("unknown".to_string()),
            ..make_test_doc("")
        };

        let allowed = vec!["person".to_string(), "project".to_string()];
        let warnings = check_document_basics(&doc, 100, None, Some(&allowed), false);
        assert_eq!(warnings, 1);

        // Should not warn if type is allowed
        let doc2 = Document {
            doc_type: Some("person".to_string()),
            ..doc
        };
        let warnings = check_document_basics(&doc2, 100, None, Some(&allowed), false);
        assert_eq!(warnings, 0);
    }
}
