//! Verification of reorganization operations.
//!
//! Verifies that fact counts match expectations after merge/split operations
//! to ensure no data was silently lost.
//!
//! Orphan counts are NOT verified because the orphan file (`_orphans.md`) is
//! append-only and shared across operations. Orphan writing is deterministic
//! (driven by the plan's ledger), so the document fact count is the only
//! meaningful verification target.

use std::path::Path;

use crate::database::Database;
use crate::error::FactbaseError;
use crate::organize::fs_helpers::read_file;
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
    /// Detailed mismatch description if verification failed
    pub mismatch_details: Option<String>,
}

impl VerificationResult {
    fn success(expected_facts: usize, actual_facts: usize) -> Self {
        Self {
            passed: true,
            expected_facts,
            actual_facts,
            mismatch_details: None,
        }
    }

    fn failure(expected_facts: usize, actual_facts: usize, details: String) -> Self {
        Self {
            passed: false,
            expected_facts,
            actual_facts,
            mismatch_details: Some(details),
        }
    }
}

/// Build a detailed failure message showing expected vs actual facts.
fn build_failure_details(
    operation: &str,
    expected: usize,
    actual: usize,
    actual_facts: &[crate::organize::TrackedFact],
    content_preview: &str,
) -> String {
    let mut details = format!(
        "{operation} verification failed: expected >= {expected} document facts, got {actual}.\n\n"
    );

    // Show first N lines of the destination content for debugging
    let preview_lines: Vec<&str> = content_preview.lines().take(30).collect();
    details.push_str("--- destination content (first 30 lines) ---\n");
    for line in &preview_lines {
        details.push_str(line);
        details.push('\n');
    }
    if content_preview.lines().count() > 30 {
        details.push_str("... (truncated)\n");
    }

    details.push_str("\n--- extracted facts ---\n");
    for (i, fact) in actual_facts.iter().enumerate() {
        let preview = if fact.content.len() > 80 {
            format!("{}...", &fact.content[..77])
        } else {
            fact.content.clone()
        };
        details.push_str(&format!("  {}: L{} {}\n", i + 1, fact.source_line, preview));
    }

    details
}

/// Verify a merge operation completed correctly.
///
/// Checks that the kept document contains at least as many facts as the plan
/// assigned to it. Orphan counts are not verified (see module docs).
pub fn verify_merge(
    plan: &MergePlan,
    result: &MergeResult,
    db: &Database,
    repo_path: &Path,
) -> Result<VerificationResult, FactbaseError> {
    let expected_doc_facts = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();

    // Read from filesystem since execute_merge writes the merged content
    // to the file but doesn't update the DB
    let kept_doc = db.require_document(&result.kept_id)?;
    let kept_path = repo_path.join(&kept_doc.file_path);
    let kept_content = read_file(&kept_path)?;
    let actual_facts = extract_facts(&kept_content, &result.kept_id);
    let actual_doc_facts = actual_facts.len();

    if actual_doc_facts >= expected_doc_facts {
        Ok(VerificationResult::success(
            expected_doc_facts,
            actual_doc_facts,
        ))
    } else {
        let details = build_failure_details(
            "Merge",
            expected_doc_facts,
            actual_doc_facts,
            &actual_facts,
            &kept_content,
        );
        Ok(VerificationResult::failure(
            expected_doc_facts,
            actual_doc_facts,
            details,
        ))
    }
}

/// Verify a split operation completed correctly.
///
/// Checks that the total facts across all new documents is at least as many
/// as the plan assigned. Orphan counts are not verified (see module docs).
pub fn verify_split(
    plan: &SplitPlan,
    result: &SplitResult,
    db: &Database,
    _repo_path: &Path,
) -> Result<VerificationResult, FactbaseError> {
    let expected_doc_facts = plan
        .ledger
        .assignments
        .values()
        .filter(|a| a.destination == FactDestination::Document)
        .count();

    let mut actual_doc_facts = 0;
    let mut all_facts = Vec::new();
    let mut all_content = String::new();
    for doc_id in &result.new_doc_ids {
        let doc = db.require_document(doc_id)?;
        let facts = extract_facts(&doc.content, doc_id);
        actual_doc_facts += facts.len();
        all_facts.extend(facts);
        all_content.push_str(&format!("--- {} ---\n{}\n\n", doc_id, doc.content));
    }

    if actual_doc_facts >= expected_doc_facts {
        Ok(VerificationResult::success(
            expected_doc_facts,
            actual_doc_facts,
        ))
    } else {
        let details = build_failure_details(
            "Split",
            expected_doc_facts,
            actual_doc_facts,
            &all_facts,
            &all_content,
        );
        Ok(VerificationResult::failure(
            expected_doc_facts,
            actual_doc_facts,
            details,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organize::{FactLedger, TrackedFact};

    #[test]
    fn test_verification_result_success() {
        let result = VerificationResult::success(5, 5);
        assert!(result.passed);
        assert_eq!(result.expected_facts, 5);
        assert_eq!(result.actual_facts, 5);
        assert!(result.mismatch_details.is_none());
    }

    #[test]
    fn test_verification_result_failure() {
        let result = VerificationResult::failure(5, 3, "test failure".to_string());
        assert!(!result.passed);
        assert_eq!(result.expected_facts, 5);
        assert_eq!(result.actual_facts, 3);
        assert_eq!(result.mismatch_details.as_deref(), Some("test failure"));
    }

    #[test]
    fn test_verification_passes_when_actual_exceeds_expected() {
        // LLM may restructure content producing more fact lines than expected
        let result = VerificationResult::success(5, 8);
        assert!(result.passed);
        assert_eq!(result.actual_facts, 8);
    }

    #[test]
    fn test_merge_plan_expected_counts() {
        let mut ledger = FactLedger::new();

        for i in 1..=5 {
            let fact = TrackedFact::new("doc1", i, &format!("fact {}", i), None, vec![]);
            ledger.add_fact(fact);
        }

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

    #[test]
    fn test_build_failure_details_includes_content() {
        let facts = vec![
            TrackedFact::new("doc1", 3, "**Role:** Engineer", None, vec![]),
            TrackedFact::new("doc1", 5, "- Hired: 2020", None, vec![]),
        ];
        let content =
            "---\nfactbase_id: abc123\n---\n# Test Doc\n\n**Role:** Engineer\n\n- Hired: 2020\n";

        let details = build_failure_details("Merge", 5, 2, &facts, content);

        assert!(details.contains("expected >= 5 document facts, got 2"));
        assert!(details.contains("destination content"));
        assert!(details.contains("**Role:** Engineer"));
        assert!(details.contains("extracted facts"));
        assert!(details.contains("L3"));
        assert!(details.contains("L5"));
    }

    #[test]
    fn test_build_failure_details_truncates_long_content() {
        let facts = vec![];
        // Create content with 40 lines
        let content: String = (1..=40).map(|i| format!("Line {i}\n")).collect();

        let details = build_failure_details("Split", 10, 0, &facts, &content);

        assert!(details.contains("(truncated)"));
        assert!(details.contains("Line 1"));
        assert!(details.contains("Line 30"));
        assert!(!details.contains("Line 40"));
    }

    #[test]
    fn test_build_failure_details_truncates_long_facts() {
        let long_content = "x".repeat(100);
        let facts = vec![TrackedFact::new("doc1", 1, &long_content, None, vec![])];

        let details = build_failure_details("Merge", 5, 1, &facts, "");

        // Should truncate to ~80 chars with "..."
        assert!(details.contains("..."));
    }
}
