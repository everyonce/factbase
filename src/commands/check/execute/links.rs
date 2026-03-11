//! Link checking for lint.
//!
//! Checks documents for orphan and broken links.

use super::LinkCheckResult;
use factbase::models::{Document, Link};
use factbase::patterns::MANUAL_LINK_REGEX;
use std::collections::{HashMap, HashSet};
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
/// * `doc_id_to_stem` - Mapping from document ID to filename stem (for readable name suggestions)
pub fn check_document_links(
    doc: &Document,
    doc_ids: &HashSet<&str>,
    links_from: &[Link],
    links_to: &[Link],
    fix: bool,
    dry_run: bool,
    is_table_format: bool,
    doc_id_to_stem: &HashMap<&str, &str>,
) -> anyhow::Result<LinkCheckResult> {
    let mut result = LinkCheckResult {
        warnings: 0,
        errors: 0,
        fixed: 0,
        broken_links: Vec::new(),
        hex_id_link_warnings: 0,
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

    // Check for broken links and hex-ID link style
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
        } else if let Some(stem) = doc_id_to_stem.get(link_id) {
            // Valid link using hex ID — suggest readable name
            if is_table_format {
                println!(
                    "  WARN: Prefer [[{}]] over [[{}]] in {} [{}]",
                    stem, link_id, doc.title, doc.id
                );
            }
            result.hex_id_link_warnings += 1;
            result.warnings += 1;
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
    use chrono::Utc;
    use factbase::models::Document;

    fn make_test_doc_with_id(id: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            title: format!("Test {}", id),
            content: content.to_string(),
            doc_type: Some("note".to_string()),
            file_path: format!("/test_{}.md", id),
            file_hash: "hash".to_string(),
            repo_id: "test-repo".to_string(),
            indexed_at: Utc::now(),
            file_modified_at: Some(Utc::now()),
            is_deleted: false,
        }
    }

    fn empty_stem_map<'a>() -> HashMap<&'a str, &'a str> {
        HashMap::new()
    }

    #[test]
    fn test_check_document_links_orphan() {
        let doc = make_test_doc_with_id("doc001", "# Test\n\nNo links here.");
        let doc_ids: HashSet<&str> = ["doc001", "doc002"].iter().copied().collect();

        // No links = orphan
        let result = check_document_links(&doc, &doc_ids, &[], &[], false, false, false, &empty_stem_map()).unwrap();
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
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &empty_stem_map()).unwrap();
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
            check_document_links(&doc2, &doc_ids, &[], &incoming, false, false, false, &empty_stem_map()).unwrap();
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
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &empty_stem_map()).unwrap();
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
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &empty_stem_map()).unwrap();
        assert_eq!(result.warnings, 0);
        assert_eq!(result.errors, 0); // valid link
        assert!(result.broken_links.is_empty());
    }

    #[test]
    fn test_hex_id_link_warns_with_readable_alternative() {
        // doc002 has hex ID "doc002" and stem "alice-chen"
        let doc1 = make_test_doc_with_id("aaa111", "# Test\n\nSee [[bbb222]] for details.");
        let doc_ids: HashSet<&str> = ["aaa111", "bbb222"].iter().copied().collect();
        let outgoing = vec![Link {
            source_id: "aaa111".to_string(),
            target_id: "bbb222".to_string(),
            context: Some("link".to_string()),
            created_at: Utc::now(),
        }];
        let stem_map: HashMap<&str, &str> = [("bbb222", "alice-chen")].into_iter().collect();

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &stem_map).unwrap();
        assert_eq!(result.hex_id_link_warnings, 1);
        assert_eq!(result.warnings, 1); // hex-ID style warning
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn test_hex_id_link_no_warn_without_stem() {
        // If no stem mapping exists, no warning
        let doc1 = make_test_doc_with_id("aaa111", "# Test\n\nSee [[bbb222]] for details.");
        let doc_ids: HashSet<&str> = ["aaa111", "bbb222"].iter().copied().collect();
        let outgoing = vec![Link {
            source_id: "aaa111".to_string(),
            target_id: "bbb222".to_string(),
            context: Some("link".to_string()),
            created_at: Utc::now(),
        }];

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &empty_stem_map()).unwrap();
        assert_eq!(result.hex_id_link_warnings, 0);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_hex_id_link_multiple_in_one_doc() {
        let doc1 = make_test_doc_with_id(
            "aaa111",
            "# Test\n\nSee [[bbb222]] and [[ccc333]] for details.",
        );
        let doc_ids: HashSet<&str> = ["aaa111", "bbb222", "ccc333"].iter().copied().collect();
        let outgoing = vec![Link {
            source_id: "aaa111".to_string(),
            target_id: "bbb222".to_string(),
            context: None,
            created_at: Utc::now(),
        }];
        let stem_map: HashMap<&str, &str> = [
            ("bbb222", "alice-chen"),
            ("ccc333", "project-atlas"),
        ]
        .into_iter()
        .collect();

        let result =
            check_document_links(&doc1, &doc_ids, &outgoing, &[], false, false, false, &stem_map).unwrap();
        assert_eq!(result.hex_id_link_warnings, 2);
        assert_eq!(result.warnings, 2);
    }

}
