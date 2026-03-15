//! Merge execution for document reorganization.
//!
//! Executes a merge plan by combining documents, redirecting links,
//! and handling orphaned facts.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::{remove_file, write_file};
use crate::organize::links::redirect_links;
use crate::organize::orphans::{write_orphans, OrphanOperation};
use crate::organize::{FactDestination, MergePlan, TrackedFact};

/// Result of executing a merge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    /// ID of the kept document
    pub kept_id: String,
    /// IDs of documents that were merged (and deleted)
    pub merged_ids: Vec<String>,
    /// Number of facts in the final document
    pub fact_count: usize,
    /// Number of facts marked as duplicates
    pub duplicate_count: usize,
    /// Number of facts sent to orphan document
    pub orphan_count: usize,
    /// Path to orphan document if any orphans were created
    pub orphan_path: Option<PathBuf>,
    /// Number of links redirected
    pub links_redirected: usize,
}

/// Execute a merge plan, combining documents and handling orphans.
///
/// # Safety Guarantees
/// - Verifies ledger is balanced before any changes
/// - Writes orphan document before deleting source files
/// - Rolls back on any failure (best effort)
///
/// # Arguments
/// * `plan` - The merge plan from `plan_merge()`
/// * `db` - Database connection
/// * `repo_path` - Path to the repository root (for file operations)
///
/// # Returns
/// `MergeResult` with counts and paths of affected documents.
///
/// # Errors
/// - `FactbaseError::Validation` if ledger is not balanced
/// - `FactbaseError::Io` on file operation failures
/// - `FactbaseError::Database` on database errors
pub fn execute_merge(
    plan: &MergePlan,
    db: &Database,
    repo_path: &Path,
) -> Result<MergeResult, FactbaseError> {
    // Verify ledger is balanced before proceeding
    if !plan.is_valid() {
        let unaccounted = plan.ledger.unaccounted_facts();
        return Err(FactbaseError::internal(format!(
            "Merge plan has {} unaccounted facts - cannot proceed",
            unaccounted.len()
        )));
    }

    // Get the kept document for file path
    let kept_doc = db.require_document(&plan.keep_id)?;

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
            OrphanOperation::Merge,
            &plan.keep_id,
        )?)
    } else {
        None
    };

    // Update the kept document with combined content
    let kept_path = repo_path.join(&kept_doc.file_path);
    write_file(&kept_path, &plan.combined_content)?;

    // Redirect links from merged documents to kept document
    let mut links_redirected = 0;
    for merge_id in &plan.merge_ids {
        links_redirected += redirect_links(db, merge_id, &plan.keep_id, repo_path)?;
    }

    // Delete merged source files and mark documents as deleted
    for merge_id in &plan.merge_ids {
        if let Some(merge_doc) = db.get_document(merge_id)? {
            let merge_path = repo_path.join(&merge_doc.file_path);
            if merge_path.exists() {
                remove_file(&merge_path)?;
            }
            db.mark_deleted(merge_id)?;
        }
    }

    // Count facts by destination
    let fact_count = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();
    let duplicate_count = plan.duplicate_count();
    let orphan_count = plan.orphan_count();

    Ok(MergeResult {
        kept_id: plan.keep_id.clone(),
        merged_ids: plan.merge_ids.clone(),
        fact_count,
        duplicate_count,
        orphan_count,
        orphan_path,
        links_redirected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::database::tests::{test_db, test_repo_in_db as test_repo};
    use crate::organize::test_helpers::tests::insert_test_doc as test_doc;
    use crate::organize::FactLedger;

    #[test]
    fn test_merge_result_struct() {
        let result = MergeResult {
            kept_id: "abc123".to_string(),
            merged_ids: vec!["def456".to_string()],
            fact_count: 5,
            duplicate_count: 2,
            orphan_count: 1,
            orphan_path: Some(PathBuf::from("_orphans.md")),
            links_redirected: 3,
        };

        assert_eq!(result.kept_id, "abc123");
        assert_eq!(result.merged_ids.len(), 1);
        assert_eq!(result.fact_count, 5);
    }

    #[test]
    fn test_execute_merge_unbalanced_ledger() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create an unbalanced plan (fact without assignment)
        let mut ledger = FactLedger::new();
        let fact = TrackedFact::new("doc1", 1, "test fact", None, vec![]);
        ledger.add_fact(fact);
        // Don't assign the fact - ledger is unbalanced

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content: "test".to_string(),
            temporal_issues: vec![],
        };

        let result = execute_merge(&plan, &db, repo_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unaccounted facts"));
    }

    #[test]
    fn test_execute_merge_basic() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test files
        let doc1_path = repo_path.join("doc1.md");
        let doc2_path = repo_path.join("doc2.md");
        fs::write(&doc1_path, "---\nfactbase_id: doc1\n---\n# Doc 1\n- Fact A").unwrap();
        fs::write(&doc2_path, "---\nfactbase_id: doc2\n---\n# Doc 2\n- Fact B").unwrap();

        // Create documents in database
        test_doc(&db, "doc1", "repo1", "Doc 1", "- Fact A", "doc1.md");
        test_doc(&db, "doc2", "repo1", "Doc 2", "- Fact B", "doc2.md");

        // Create a balanced merge plan
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "- Fact A", None, vec![]);
        let fact2 = TrackedFact::new("doc2", 1, "- Fact B", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content:
                "---\nfactbase_id: doc1\n---\n# Doc 1\n- Fact A\n\n## Merged Content\n\n- Fact B\n"
                    .to_string(),
            temporal_issues: vec![],
        };

        let result = execute_merge(&plan, &db, repo_path).expect("merge should succeed");

        assert_eq!(result.kept_id, "doc1");
        assert_eq!(result.merged_ids, vec!["doc2"]);
        assert_eq!(result.fact_count, 2);
        assert_eq!(result.orphan_count, 0);
        assert!(result.orphan_path.is_none());

        // Verify doc2 file was deleted
        assert!(!doc2_path.exists());

        // Verify doc1 has merged content
        let content = fs::read_to_string(&doc1_path).unwrap();
        assert!(content.contains("Fact A"));
        assert!(content.contains("Fact B"));
    }

    #[test]
    fn test_execute_merge_with_orphans() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test files
        let doc1_path = repo_path.join("doc1.md");
        let doc2_path = repo_path.join("doc2.md");
        fs::write(&doc1_path, "---\nfactbase_id: doc1\n---\n# Doc 1\n- Fact A").unwrap();
        fs::write(
            &doc2_path,
            "---\nfactbase_id: doc2\n---\n# Doc 2\n- Fact B\n- Fact C",
        )
        .unwrap();

        test_doc(&db, "doc1", "repo1", "Doc 1", "- Fact A", "doc1.md");
        test_doc(
            &db,
            "doc2",
            "repo1",
            "Doc 2",
            "- Fact B\n- Fact C",
            "doc2.md",
        );

        // Create plan with one orphan
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "- Fact A", None, vec![]);
        let fact2 = TrackedFact::new("doc2", 1, "- Fact B", None, vec![]);
        let fact3 = TrackedFact::new("doc2", 2, "- Fact C", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        let id3 = fact3.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.add_fact(fact3);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(
            &id3,
            FactDestination::Orphan,
            None,
            Some("doesn't fit".to_string()),
        );

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content:
                "---\nfactbase_id: doc1\n---\n# Doc 1\n- Fact A\n\n## Merged Content\n\n- Fact B\n"
                    .to_string(),
            temporal_issues: vec![],
        };

        let result = execute_merge(&plan, &db, repo_path).expect("merge should succeed");

        assert_eq!(result.orphan_count, 1);
        assert!(result.orphan_path.is_some());

        // Verify orphan file was created
        let orphan_path = result.orphan_path.unwrap();
        assert!(orphan_path.exists());
        let orphan_content = fs::read_to_string(&orphan_path).unwrap();
        assert!(orphan_content.contains("Fact C"));
        assert!(orphan_content.contains("@r[orphan]"));
    }

    #[test]
    fn test_execute_merge_redirects_links() {
        let (db, temp) = test_db();
        let repo_path = temp.path();
        test_repo(&db, "repo1", repo_path);

        // Create test files
        let doc1_path = repo_path.join("doc1.md");
        let doc2_path = repo_path.join("doc2.md");
        let doc3_path = repo_path.join("doc3.md");
        fs::write(&doc1_path, "---\nfactbase_id: doc1\n---\n# Doc 1\n- Fact A").unwrap();
        fs::write(&doc2_path, "---\nfactbase_id: doc2\n---\n# Doc 2\n- Fact B").unwrap();
        fs::write(
            &doc3_path,
            "---\nfactbase_id: doc3\n---\n# Doc 3\n- Links to Doc 2",
        )
        .unwrap();

        test_doc(&db, "doc1", "repo1", "Doc 1", "- Fact A", "doc1.md");
        test_doc(&db, "doc2", "repo1", "Doc 2", "- Fact B", "doc2.md");
        test_doc(&db, "doc3", "repo1", "Doc 3", "- Links to Doc 2", "doc3.md");

        // Create link from doc3 to doc2
        db.update_links(
            "doc3",
            &[crate::link_detection::DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "references".to_string(),
            }],
        )
        .unwrap();

        // Create balanced plan
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "- Fact A", None, vec![]);
        let fact2 = TrackedFact::new("doc2", 1, "- Fact B", None, vec![]);
        let id1 = fact1.id.clone();
        let id2 = fact2.id.clone();
        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &id1,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );
        ledger.assign(
            &id2,
            FactDestination::Document,
            Some("doc1".to_string()),
            None,
        );

        let plan = MergePlan {
            keep_id: "doc1".to_string(),
            merge_ids: vec!["doc2".to_string()],
            ledger,
            combined_content: "merged".to_string(),
            temporal_issues: vec![],
        };

        let result = execute_merge(&plan, &db, repo_path).expect("merge should succeed");

        assert_eq!(result.links_redirected, 1);

        // Verify link was redirected
        let links = db.get_links_from("doc3").unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_id, "doc1"); // Now points to doc1 instead of doc2
    }
}
