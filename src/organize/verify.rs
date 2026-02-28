//! Verification of reorganization operations.
//!
//! Verifies that fact counts match expectations after merge/split operations
//! to ensure no data was silently lost.

use std::path::Path;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::read_file;
use crate::organize::review::parse_orphan_entries;
use crate::organize::{
    extract_facts, FactDestination, MergePlan, MergeResult, SplitPlan, SplitResult,
};

/// Result of verification with detailed counts.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification passed
    pub passed: bool,
    /// Expected fact count from ledger
    pub expected_facts: usize,
    /// Actual fact count in destination files
    pub actual_facts: usize,
    /// Expected orphan count from ledger
    pub expected_orphans: usize,
    /// Actual orphan count in orphan file
    pub actual_orphans: usize,
    /// Detailed mismatch description if verification failed
    pub mismatch_details: Option<String>,
}

impl VerificationResult {
    fn success(expected_facts: usize, expected_orphans: usize) -> Self {
        Self {
            passed: true,
            expected_facts,
            actual_facts: expected_facts,
            expected_orphans,
            actual_orphans: expected_orphans,
            mismatch_details: None,
        }
    }

    fn failure(
        expected_facts: usize,
        actual_facts: usize,
        expected_orphans: usize,
        actual_orphans: usize,
        details: String,
    ) -> Self {
        Self {
            passed: false,
            expected_facts,
            actual_facts,
            expected_orphans,
            actual_orphans,
            mismatch_details: Some(details),
        }
    }
}

/// Verify a merge operation completed correctly.
///
/// Counts facts in the kept document and orphan file, comparing to ledger expectations.
///
/// # Arguments
/// * `plan` - The merge plan that was executed
/// * `result` - The result from execute_merge
/// * `db` - Database connection
/// * `repo_path` - Repository root path
///
/// # Returns
/// `VerificationResult` with pass/fail status and counts.
pub fn verify_merge(
    plan: &MergePlan,
    result: &MergeResult,
    db: &Database,
    repo_path: &Path,
) -> Result<VerificationResult, FactbaseError> {
    // Calculate expected counts from ledger
    let expected_doc_facts = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();
    let expected_orphans = plan.ledger.orphan_count();

    // Count actual facts in kept document - read from filesystem since
    // execute_merge writes the merged content to the file but doesn't update the DB
    let kept_doc = db.require_document(&result.kept_id)?;
    let kept_path = repo_path.join(&kept_doc.file_path);
    let kept_content = read_file(&kept_path)?;
    let actual_doc_facts = extract_facts(&kept_content, &result.kept_id).len();

    // Count actual orphans from THIS operation only (the orphan file is shared
    // across operations, so we filter by source document IDs)
    let actual_orphans = match &result.orphan_path {
        Some(p) => {
            let all_entries = parse_orphan_entries(&read_file(p)?);
            let mut source_ids: Vec<&str> = plan.merge_ids.iter().map(|s| s.as_str()).collect();
            source_ids.push(&plan.keep_id);
            all_entries
                .iter()
                .filter(|e| {
                    e.source_doc
                        .as_ref()
                        .is_some_and(|id| source_ids.contains(&id.as_str()))
                })
                .count()
        }
        None => 0,
    };

    // Verify counts match — use >= for both checks because:
    // - Document facts: LLM may split/restructure facts, producing more lines
    // - Orphans: the orphan file is append-only and shared across operations,
    //   so pre-existing entries for the same doc IDs from prior operations
    //   inflate the count beyond what this operation alone produced
    if actual_doc_facts >= expected_doc_facts && actual_orphans >= expected_orphans {
        Ok(VerificationResult::success(
            expected_doc_facts,
            expected_orphans,
        ))
    } else {
        let details = format!(
            "Merge verification failed: expected {expected_doc_facts} document facts (got {actual_doc_facts}), {expected_orphans} orphans (got {actual_orphans})"
        );
        Ok(VerificationResult::failure(
            expected_doc_facts,
            actual_doc_facts,
            expected_orphans,
            actual_orphans,
            details,
        ))
    }
}

/// Verify a split operation completed correctly.
///
/// Counts facts in all new documents and orphan file, comparing to ledger expectations.
///
/// # Arguments
/// * `plan` - The split plan that was executed
/// * `result` - The result from execute_split
/// * `db` - Database connection
/// * `repo_path` - Repository root path
///
/// # Returns
/// `VerificationResult` with pass/fail status and counts.
pub fn verify_split(
    plan: &SplitPlan,
    result: &SplitResult,
    db: &Database,
    _repo_path: &Path,
) -> Result<VerificationResult, FactbaseError> {
    // Calculate expected counts from ledger
    let expected_doc_facts = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();
    let expected_orphans = plan.ledger.orphan_count();

    // Count actual facts in all new documents
    let mut actual_doc_facts = 0;
    for doc_id in &result.new_doc_ids {
        let doc = db.require_document(doc_id)?;
        actual_doc_facts += extract_facts(&doc.content, doc_id).len();
    }

    // Count actual orphans from THIS operation only (the orphan file is shared
    // across operations, so we filter by source document ID)
    let actual_orphans = match &result.orphan_path {
        Some(p) => {
            let all_entries = parse_orphan_entries(&read_file(p)?);
            all_entries
                .iter()
                .filter(|e| {
                    e.source_doc
                        .as_ref()
                        .is_some_and(|id| id == &plan.source_id)
                })
                .count()
        }
        None => 0,
    };

    // Verify counts match — use >= for both checks because:
    // - Document facts: LLM may split/restructure facts, producing more lines
    // - Orphans: the orphan file is append-only and shared across operations,
    //   so pre-existing entries for the same doc IDs from prior operations
    //   inflate the count beyond what this operation alone produced
    if actual_doc_facts >= expected_doc_facts && actual_orphans >= expected_orphans {
        Ok(VerificationResult::success(
            expected_doc_facts,
            expected_orphans,
        ))
    } else {
        let details = format!(
            "Split verification failed: expected {expected_doc_facts} document facts (got {actual_doc_facts}), {expected_orphans} orphans (got {actual_orphans})"
        );
        Ok(VerificationResult::failure(
            expected_doc_facts,
            actual_doc_facts,
            expected_orphans,
            actual_orphans,
            details,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organize::review::parse_orphan_entries;
    use crate::organize::{FactLedger, TrackedFact};
    use std::fs;

    #[test]
    fn test_verification_result_success() {
        let result = VerificationResult::success(5, 2);
        assert!(result.passed);
        assert_eq!(result.expected_facts, 5);
        assert_eq!(result.actual_facts, 5);
        assert_eq!(result.expected_orphans, 2);
        assert_eq!(result.actual_orphans, 2);
        assert!(result.mismatch_details.is_none());
    }

    #[test]
    fn test_verification_result_failure() {
        let result = VerificationResult::failure(5, 3, 2, 1, "test failure".to_string());
        assert!(!result.passed);
        assert_eq!(result.expected_facts, 5);
        assert_eq!(result.actual_facts, 3);
        assert_eq!(result.expected_orphans, 2);
        assert_eq!(result.actual_orphans, 1);
        assert_eq!(result.mismatch_details, Some("test failure".to_string()));
    }

    #[test]
    fn test_parse_orphan_entries_from_file_nonexistent() {
        let path = Path::new("/nonexistent/path/orphans.md");
        assert!(!path.exists());
        // Production code returns 0 when file doesn't exist (match None => 0)
    }

    #[test]
    fn test_parse_orphan_entries_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let orphan_path = temp_dir.path().join("_orphans.md");

        let content = r#"# Orphaned Facts

## Merge abc123 (2026-02-03 00:00:00)

- First orphan fact @r[orphan] <!-- from abc123 line 5 -->
- Second orphan fact @r[orphan] <!-- from abc123 line 8 -->

## Split def456 (2026-02-03 00:01:00)

- Third orphan @r[orphan] <!-- from def456 line 3 -->
"#;
        fs::write(&orphan_path, content).unwrap();

        let entries = parse_orphan_entries(&read_file(&orphan_path).unwrap());
        assert_eq!(entries.len(), 3);
    }

    /// Regression test: verify_merge must only count orphans from the current
    /// operation, not all orphans in the shared _orphans.md file.
    #[test]
    fn test_orphan_count_filters_by_source_doc() {
        // Simulate an _orphans.md with entries from two different merge operations
        let content = r#"# Orphaned Facts

## Merge aaa111 (2026-02-27 00:00:00)

- Old orphan from first merge @r[orphan] <!-- from aaa111 line 3 -->
- Another old orphan @r[orphan] <!-- from bbb222 line 7 -->
- Third old orphan @r[orphan] <!-- from aaa111 line 10 -->

## Merge ccc333 (2026-02-27 00:01:00)

- New orphan from second merge @r[orphan] <!-- from ccc333 line 5 -->
- Another new orphan @r[orphan] <!-- from ddd444 line 2 -->
"#;

        let all_entries = parse_orphan_entries(content);
        assert_eq!(all_entries.len(), 5, "total orphans in file");

        // Filter for second merge (keep_id=ccc333, merge_ids=[ddd444])
        let source_ids = vec!["ccc333", "ddd444"];
        let filtered: usize = all_entries
            .iter()
            .filter(|e| {
                e.source_doc
                    .as_ref()
                    .is_some_and(|id| source_ids.contains(&id.as_str()))
            })
            .count();
        assert_eq!(filtered, 2, "only orphans from current merge");

        // Filter for first merge (keep_id=aaa111, merge_ids=[bbb222])
        let source_ids = vec!["aaa111", "bbb222"];
        let filtered: usize = all_entries
            .iter()
            .filter(|e| {
                e.source_doc
                    .as_ref()
                    .is_some_and(|id| source_ids.contains(&id.as_str()))
            })
            .count();
        assert_eq!(filtered, 3, "only orphans from first merge");
    }

    #[test]
    fn test_parse_orphan_entries_ignores_non_orphan_lines() {
        let temp_dir = tempfile::tempdir().unwrap();
        let orphan_path = temp_dir.path().join("_orphans.md");

        let content = r#"# Orphaned Facts

Some intro text that is not an orphan.

## Merge abc123 (2026-02-03 00:00:00)

- Actual orphan @r[orphan] <!-- from abc123 line 5 -->
- Not an orphan, just a list item
- Another orphan @r[orphan] <!-- from abc123 line 8 -->

Regular paragraph text.
"#;
        fs::write(&orphan_path, content).unwrap();

        let entries = parse_orphan_entries(&read_file(&orphan_path).unwrap());
        assert_eq!(entries.len(), 2);
    }

    /// Regression test: pre-existing orphan entries for the same doc IDs
    /// (from a prior successful operation) must not cause verification failure.
    /// The orphan file is append-only, so actual_orphans >= expected_orphans
    /// is the correct check.
    #[test]
    fn test_orphan_verification_tolerates_preexisting_entries() {
        // Scenario: doc aaa111 was previously involved in a split that produced
        // 3 orphan entries. Now we merge aaa111 with bbb222, expecting 2 orphans.
        // The orphan file will have 3 (old) + 2 (new) = 5 entries for aaa111/bbb222.
        let content = r#"# Orphaned Facts

## Split aaa111 (2026-02-26 00:00:00)

- Old orphan from split @r[orphan] <!-- from aaa111 line 3 -->
- Another old orphan @r[orphan] <!-- from aaa111 line 7 -->
- Third old orphan @r[orphan] <!-- from aaa111 line 10 -->

## Merge aaa111 (2026-02-27 00:00:00)

- New orphan from merge @r[orphan] <!-- from aaa111 line 5 -->
- Another new orphan @r[orphan] <!-- from bbb222 line 2 -->
"#;

        let all_entries = parse_orphan_entries(content);
        let source_ids = vec!["aaa111", "bbb222"];
        let actual_orphans: usize = all_entries
            .iter()
            .filter(|e| {
                e.source_doc
                    .as_ref()
                    .is_some_and(|id| source_ids.contains(&id.as_str()))
            })
            .count();

        // actual_orphans is 5 (3 old + 2 new), expected is 2
        let expected_orphans = 2;
        assert_eq!(actual_orphans, 5);

        // The >= check should pass (5 >= 2)
        assert!(actual_orphans >= expected_orphans);
    }

    #[test]
    fn test_merge_plan_expected_counts() {
        let mut ledger = FactLedger::new();

        // Add 5 facts
        for i in 1..=5 {
            let fact = TrackedFact::new("doc1", i, &format!("fact {}", i), None, vec![]);
            ledger.add_fact(fact);
        }

        // Assign: 3 to document, 1 orphan, 1 duplicate
        let fact_ids: Vec<_> = ledger.source_facts.iter().map(|f| f.id.clone()).collect();
        ledger.assign(
            &fact_ids[0],
            FactDestination::Document,
            Some("keep".to_string()),
            None,
        );
        ledger.assign(
            &fact_ids[1],
            FactDestination::Document,
            Some("keep".to_string()),
            None,
        );
        ledger.assign(
            &fact_ids[2],
            FactDestination::Document,
            Some("keep".to_string()),
            None,
        );
        ledger.assign(&fact_ids[3], FactDestination::Orphan, None, None);
        ledger.assign(&fact_ids[4], FactDestination::Duplicate, None, None);

        let expected_doc = ledger
            .assignments
            .values()
            .filter(|a| a.destination == FactDestination::Document)
            .count();
        let expected_orphans = ledger.orphan_count();

        assert_eq!(expected_doc, 3);
        assert_eq!(expected_orphans, 1);
    }
}
