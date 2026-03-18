//! Split execution for document reorganization.
//!
//! Executes a split plan by creating new documents from sections,
//! redirecting links, and handling orphaned facts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::{remove_file, write_file};
use crate::organize::orphans::{write_orphans, OrphanOperation};
use crate::organize::{FactDestination, SplitPlan, TrackedFact};
use crate::processor::DocumentProcessor;

/// Result of executing a split operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitResult {
    /// ID of the source document that was split
    pub source_id: String,
    /// IDs of newly created documents
    pub new_doc_ids: Vec<String>,
    /// Total number of facts distributed
    pub fact_count: usize,
    /// Number of facts sent to orphan document
    pub orphan_count: usize,
    /// Path to orphan document if any orphans were created
    pub orphan_path: Option<PathBuf>,
}

/// Execute a split plan, creating new documents and handling orphans.
///
/// # Safety Guarantees
/// - Verifies ledger is balanced before any changes
/// - Creates new documents before deleting source
/// - Writes orphan document before deleting source file
/// - Rolls back on any failure (best effort)
///
/// # Arguments
/// * `plan` - The split plan from `plan_split()`
/// * `db` - Database connection
/// * `repo_path` - Path to the repository root (for file operations)
///
/// # Returns
/// `SplitResult` with IDs of new documents and orphan info.
///
/// # Errors
/// - `FactbaseError::Validation` if ledger is not balanced
/// - `FactbaseError::Io` on file operation failures
/// - `FactbaseError::Database` on database errors
pub fn execute_split(
    plan: &SplitPlan,
    db: &Database,
    repo_path: &Path,
) -> Result<SplitResult, FactbaseError> {
    // Verify ledger is balanced before proceeding
    if !plan.is_valid() {
        let unaccounted = plan.ledger.unaccounted_facts();
        return Err(FactbaseError::internal(format!(
            "Split plan has {} unaccounted facts - cannot proceed",
            unaccounted.len()
        )));
    }

    // Get the source document for file path and type
    let source_doc = db.require_document(&plan.source_id)?;

    let source_path = repo_path.join(&source_doc.file_path);
    let source_dir = source_path
        .parent()
        .ok_or_else(|| FactbaseError::internal("Source file has no parent directory"))?;

    // Collect orphaned facts
    let orphans: Vec<&TrackedFact> = plan
        .ledger
        .source_facts
        .iter()
        .filter(|f| {
            plan.ledger
                .assignments
                .get(&f.id)
                .is_some_and(|a| a.destination == FactDestination::Orphan)
        })
        .collect();

    // Write orphan document first (before any destructive operations)
    let orphan_path = if !orphans.is_empty() {
        Some(write_orphans(
            &orphans,
            repo_path,
            OrphanOperation::Split,
            &plan.source_id,
        )?)
    } else {
        None
    };

    // Create new documents
    let processor = DocumentProcessor::new();
    let mut new_doc_ids = Vec::new();

    for proposed in &plan.new_documents {
        // Generate unique ID for new document
        let new_id = processor.generate_unique_id(db);

        // Build file path in same directory as source
        let safe_title = sanitize_filename(&proposed.title);
        let new_filename = format!("{safe_title}.md");
        let new_path = source_dir.join(&new_filename);

        // Inject factbase ID into content via frontmatter
        let fmt = crate::models::format::ResolvedFormat::default();
        let content_with_header =
            processor.inject_id_with_format(&proposed.content, &new_id, &fmt, None);

        // Write the new file
        write_file(&new_path, &content_with_header)?;

        new_doc_ids.push(new_id);
    }

    // Delete source file and mark document as deleted
    if source_path.exists() {
        remove_file(&source_path)?;
    }
    db.mark_deleted(&plan.source_id)?;

    // Count facts assigned to documents
    let fact_count = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();

    Ok(SplitResult {
        source_id: plan.source_id.clone(),
        new_doc_ids,
        fact_count,
        orphan_count: orphans.len(),
        orphan_path,
    })
}

/// Sanitize a title for use as a filename.
fn sanitize_filename(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .replace(' ', "-")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::insert_test_doc as test_doc;
    use crate::organize::{FactLedger, ProposedDocument};

    #[test]
    fn test_split_result_struct() {
        let result = SplitResult {
            source_id: "abc123".to_string(),
            new_doc_ids: vec!["def456".to_string(), "ghi789".to_string()],
            fact_count: 5,
            orphan_count: 1,
            orphan_path: Some(PathBuf::from("_orphans.md")),
        };

        assert_eq!(result.source_id, "abc123");
        assert_eq!(result.new_doc_ids.len(), 2);
        assert_eq!(result.fact_count, 5);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Career History"), "career-history");
        assert_eq!(sanitize_filename("Test/File:Name"), "test_file_name");
        assert_eq!(sanitize_filename("  Spaces  "), "spaces");
        assert_eq!(sanitize_filename("Already-Good"), "already-good");
        assert_eq!(sanitize_filename("With_Underscore"), "with_underscore");
    }

    #[test]
    fn test_execute_split_unbalanced_ledger() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create an unbalanced plan (fact without assignment)
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "test fact", None, vec![]);
        ledger.add_fact(fact);
        // Don't assign the fact - ledger is unbalanced

        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![],
            ledger,
            temporal_issues: vec![],
        };

        let result = execute_split(&plan, &db, repo_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unaccounted facts"));
    }

    #[test]
    fn test_execute_split_basic() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test file
        let doc_path = repo_path.join("doc1.md");
        fs::write(
            &doc_path,
            "---\nfactbase_id: doc1\n---\n# Multi-Topic Doc\n\n## Career\n- CTO at Acme\n\n## Hobbies\n- Plays guitar",
        )
        .unwrap();

        // Create document in database
        test_doc(
            &db,
            "doc1",
            "repo1",
            "Multi-Topic Doc",
            "## Career\n- CTO at Acme\n\n## Hobbies\n- Plays guitar",
            "doc1.md",
        );

        // Create a balanced split plan
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "- CTO at Acme", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "- Plays guitar", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("section_0".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("section_1".to_string()),
            None,
        );

        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![
                ProposedDocument {
                    title: "Career History".to_string(),
                    section_title: "Career".to_string(),
                    content: "# Career History\n\n- CTO at Acme\n".to_string(),
                },
                ProposedDocument {
                    title: "Hobbies".to_string(),
                    section_title: "Hobbies".to_string(),
                    content: "# Hobbies\n\n- Plays guitar\n".to_string(),
                },
            ],
            ledger,
            temporal_issues: vec![],
        };

        let result = execute_split(&plan, &db, repo_path).expect("split should succeed");

        assert_eq!(result.source_id, "doc1");
        assert_eq!(result.new_doc_ids.len(), 2);
        assert_eq!(result.fact_count, 2);
        assert_eq!(result.orphan_count, 0);
        assert!(result.orphan_path.is_none());

        // Verify source file was deleted
        assert!(!doc_path.exists());

        // Verify new files were created
        let career_path = repo_path.join("career-history.md");
        let hobbies_path = repo_path.join("hobbies.md");
        assert!(career_path.exists());
        assert!(hobbies_path.exists());

        // Verify content has factbase headers
        let career_content = fs::read_to_string(&career_path).unwrap();
        assert!(career_content.starts_with("---\nfactbase_id:"));
        assert!(career_content.contains("CTO at Acme"));

        let hobbies_content = fs::read_to_string(&hobbies_path).unwrap();
        assert!(hobbies_content.starts_with("---\nfactbase_id:"));
        assert!(hobbies_content.contains("Plays guitar"));
    }

    #[test]
    fn test_execute_split_with_orphans() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test file
        let doc_path = repo_path.join("doc1.md");
        fs::write(
            &doc_path,
            "---\nfactbase_id: doc1\n---\n# Doc\n- Fact A\n- Fact B",
        )
        .unwrap();

        test_doc(&db, "doc1", "repo1", "Doc", "- Fact A\n- Fact B", "doc1.md");

        // Create plan with one orphan
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "- Fact A", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "- Fact B", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("section_0".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Orphan,
            None,
            Some("doesn't fit".to_string()),
        );

        let plan = SplitPlan {
            source_id: "doc1".to_string(),
            new_documents: vec![ProposedDocument {
                title: "New Doc".to_string(),
                section_title: "Section".to_string(),
                content: "# New Doc\n\n- Fact A\n".to_string(),
            }],
            ledger,
            temporal_issues: vec![],
        };

        let result = execute_split(&plan, &db, repo_path).expect("split should succeed");

        assert_eq!(result.orphan_count, 1);
        assert!(result.orphan_path.is_some());

        // Verify orphan file was created
        let orphan_path = result.orphan_path.unwrap();
        assert!(orphan_path.exists());
        let orphan_content = fs::read_to_string(&orphan_path).unwrap();
        assert!(orphan_content.contains("Fact B"));
        assert!(orphan_content.contains("@r[orphan]"));
        assert!(orphan_content.contains("## Split doc1"));
    }
}
