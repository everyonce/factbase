//! Link checking for lint.
//!
//! Checks documents for orphan and broken links.

use super::LinkCheckResult;
use factbase::{Document, Link, MANUAL_LINK_REGEX};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};

/// Check a document for orphan and broken links.
///
/// # Arguments
/// * `doc` - The document to check
/// * `doc_ids` - Set of all valid document IDs (for broken link detection)
/// * `links_from` - Pre-fetched outgoing links for this document
/// * `links_to` - Pre-fetched incoming links for this document
/// * `fix` - Whether to fix broken links
/// * `dry_run` - Whether to show what would be fixed without making changes
/// * `is_table_format` - Whether to print table-formatted output
pub fn check_document_links(
    doc: &Document,
    doc_ids: &HashSet<&str>,
    links_from: &[Link],
    links_to: &[Link],
    fix: bool,
    dry_run: bool,
    is_table_format: bool,
) -> anyhow::Result<LinkCheckResult> {
    let mut result = LinkCheckResult {
        warnings: 0,
        errors: 0,
        fixed: 0,
        broken_links: Vec::new(),
    };

    // Check for orphan documents
    if links_from.is_empty() && links_to.is_empty() {
        if is_table_format {
            println!(
                "  WARN: Orphan document (no links): {} [{}]",
                doc.title, doc.id
            );
        }
        result.warnings += 1;
    }

    // Check for broken links
    for cap in MANUAL_LINK_REGEX.captures_iter(&doc.content) {
        let link_id = &cap[1];
        if !doc_ids.contains(link_id) {
            result.broken_links.push(link_id.to_string());
            if !fix && !dry_run && is_table_format {
                println!(
                    "  ERROR: Broken link [[{}]] in {} [{}]",
                    link_id, doc.title, doc.id
                );
            }
            result.errors += 1;
        }
    }

    // Fix broken links if requested
    if fix && is_table_format && !result.broken_links.is_empty() {
        if dry_run {
            println!(
                "  Would fix {} broken link(s) in {} [{}]:",
                result.broken_links.len(),
                doc.title,
                doc.id
            );
            for link_id in &result.broken_links {
                println!("    - Remove [[{link_id}]]");
            }
        } else {
            print!(
                "  Fix {} broken link(s) in {} [{}]? [y/N] ",
                result.broken_links.len(),
                doc.title,
                doc.id
            );
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() == "y" {
                // Read fresh content from file to avoid clone
                let mut content = fs::read_to_string(&doc.file_path)?;
                for link_id in &result.broken_links {
                    let pattern = format!("[[{link_id}]]");
                    content = content.replace(&pattern, "");
                }
                fs::write(&doc.file_path, &content)?;
                println!(
                    "    Fixed: removed {} broken link(s)",
                    result.broken_links.len()
                );
                result.fixed = result.broken_links.len();
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::lint::execute::test_helpers::make_test_doc_with_id;
    use chrono::Utc;

    #[test]
    fn test_check_document_links_orphan() {
        let doc = make_test_doc_with_id("doc001", "# Test\n\nNo links here.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // No links = orphan
        let result = check_document_links(&doc, &doc_ids, &[], &[], false, false, false).unwrap();
        assert_eq!(result.warnings, 1); // orphan warning
        assert_eq!(result.errors, 0);
        assert!(result.broken_links.is_empty());
    }

    #[test]
    fn test_check_document_links_not_orphan_with_outgoing() {
        let doc1 = make_test_doc_with_id("doc001", "# Test\n\nSome content.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // Has outgoing link
        let outgoing = vec![Link {
            source_id: "doc001".to_string(),
            target_id: "doc002".to_string(),
            context: Some("test link".to_string()),
            created_at: Utc::now(),
        }];

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false).unwrap();
        assert_eq!(result.warnings, 0); // not orphan
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn test_check_document_links_not_orphan_with_incoming() {
        let doc2 = make_test_doc_with_id("doc002", "# Test 2\n\nOther content.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // Has incoming link
        let incoming = vec![Link {
            source_id: "doc001".to_string(),
            target_id: "doc002".to_string(),
            context: Some("test link".to_string()),
            created_at: Utc::now(),
        }];

        let result =
            check_document_links(&doc2, &doc_ids, &[], &incoming, false, false, false).unwrap();
        assert_eq!(result.warnings, 0); // not orphan (has incoming)
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn test_check_document_links_broken_link() {
        // Use valid 6-char hex ID that doesn't exist in doc_ids
        let doc1 = make_test_doc_with_id("doc001", "# Test\n\nSee [[aaaaaa]] for details.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // Has outgoing link so it's not orphan
        let outgoing = vec![Link {
            source_id: "doc001".to_string(),
            target_id: "doc002".to_string(),
            context: Some("test link".to_string()),
            created_at: Utc::now(),
        }];

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false).unwrap();
        assert_eq!(result.warnings, 0);
        assert_eq!(result.errors, 1); // broken link [[aaaaaa]]
        assert_eq!(result.broken_links, vec!["aaaaaa"]);
    }

    #[test]
    fn test_check_document_links_valid_manual_link() {
        let doc1 = make_test_doc_with_id("doc001", "# Test\n\nSee [[doc002]] for details.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // Has outgoing link so it's not orphan
        let outgoing = vec![Link {
            source_id: "doc001".to_string(),
            target_id: "doc002".to_string(),
            context: Some("test link".to_string()),
            created_at: Utc::now(),
        }];

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false).unwrap();
        assert_eq!(result.warnings, 0);
        assert_eq!(result.errors, 0); // valid link
        assert!(result.broken_links.is_empty());
    }
}
