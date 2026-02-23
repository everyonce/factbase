//! Audit logging for reorganization operations.
//!
//! All reorganization operations are logged to `.factbase/reorg-log/<timestamp>.yaml`
//! for review and debugging. The log format is human-readable YAML.

#![allow(dead_code)] // operational utility — not yet wired to CLI/MCP

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::error::FactbaseError;
use crate::organize::{
    FactLedger, MergeResult, MoveResult, RetypeResult, SplitResult, TrackedFact,
};

/// Directory name for audit logs within .factbase
const AUDIT_LOG_DIR: &str = "reorg-log";

/// Type of reorganization operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OperationType {
    Merge,
    Split,
    Move,
    Retype,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Merge => write!(f, "merge"),
            OperationType::Split => write!(f, "split"),
            OperationType::Move => write!(f, "move"),
            OperationType::Retype => write!(f, "retype"),
        }
    }
}

/// Summary of a fact for audit logging (excludes full content for readability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FactSummary {
    /// Fact ID
    pub id: String,
    /// Source document ID
    pub source_doc: String,
    /// Line number in source
    pub source_line: usize,
    /// First 80 chars of content (truncated)
    pub content_preview: String,
}

impl From<&TrackedFact> for FactSummary {
    fn from(fact: &TrackedFact) -> Self {
        let content_preview = if fact.content.len() > 80 {
            let mut end = 77;
            while end > 0 && !fact.content.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &fact.content[..end])
        } else {
            fact.content.clone()
        };
        Self {
            id: fact.id.clone(),
            source_doc: fact.source_doc.clone(),
            source_line: fact.source_line,
            content_preview,
        }
    }
}

/// Audit log entry for a reorganization operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuditEntry {
    /// Timestamp of the operation
    pub timestamp: DateTime<Local>,
    /// Type of operation
    pub operation: OperationType,
    /// Source document IDs involved
    pub source_docs: Vec<String>,
    /// Result document IDs (kept/created)
    pub result_docs: Vec<String>,
    /// Fact mappings (for merge/split)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_mappings: Option<Vec<FactMapping>>,
    /// Orphan facts (for merge/split)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub orphans: Vec<FactSummary>,
    /// Path to orphan file if created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orphan_path: Option<PathBuf>,
    /// Additional operation-specific details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<OperationDetails>,
}

/// Mapping of a fact to its destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FactMapping {
    /// Fact summary
    pub fact: FactSummary,
    /// Destination type
    pub destination: String,
    /// Target document ID (if assigned to document)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_doc: Option<String>,
    /// Reason for assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Operation-specific details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum OperationDetails {
    Merge(MergeDetails),
    Split(SplitDetails),
    Move(MoveDetails),
    Retype(RetypeDetails),
}

/// Details specific to merge operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MergeDetails {
    /// ID of the kept document
    pub kept_id: String,
    /// IDs of merged (deleted) documents
    pub merged_ids: Vec<String>,
    /// Number of links redirected
    pub links_redirected: usize,
    /// Number of duplicate facts
    pub duplicate_count: usize,
}

/// Details specific to split operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SplitDetails {
    /// ID of the source document (deleted)
    pub source_id: String,
    /// IDs of newly created documents
    pub new_doc_ids: Vec<String>,
}

/// Details specific to move operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MoveDetails {
    /// Original file path
    pub old_path: String,
    /// New file path
    pub new_path: String,
    /// Original type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<String>,
    /// New type
    pub new_type: String,
}

/// Details specific to retype operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RetypeDetails {
    /// Original type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_type: Option<String>,
    /// New type
    pub new_type: String,
    /// Whether persisted to file
    pub persisted_to_file: bool,
}

impl AuditEntry {
    /// Create an audit entry for a merge operation.
    pub(crate) fn from_merge(result: &MergeResult, ledger: &FactLedger) -> Self {
        let (fact_mappings, orphans) = extract_mappings_and_orphans(ledger);

        Self {
            timestamp: Local::now(),
            operation: OperationType::Merge,
            source_docs: std::iter::once(result.kept_id.clone())
                .chain(result.merged_ids.iter().cloned())
                .collect(),
            result_docs: vec![result.kept_id.clone()],
            fact_mappings: Some(fact_mappings),
            orphans,
            orphan_path: result.orphan_path.clone(),
            details: Some(OperationDetails::Merge(MergeDetails {
                kept_id: result.kept_id.clone(),
                merged_ids: result.merged_ids.clone(),
                links_redirected: result.links_redirected,
                duplicate_count: result.duplicate_count,
            })),
        }
    }

    /// Create an audit entry for a split operation.
    pub(crate) fn from_split(result: &SplitResult, ledger: &FactLedger) -> Self {
        let (fact_mappings, orphans) = extract_mappings_and_orphans(ledger);

        Self {
            timestamp: Local::now(),
            operation: OperationType::Split,
            source_docs: vec![result.source_id.clone()],
            result_docs: result.new_doc_ids.clone(),
            fact_mappings: Some(fact_mappings),
            orphans,
            orphan_path: result.orphan_path.clone(),
            details: Some(OperationDetails::Split(SplitDetails {
                source_id: result.source_id.clone(),
                new_doc_ids: result.new_doc_ids.clone(),
            })),
        }
    }

    /// Create an audit entry for a move operation.
    pub(crate) fn from_move(result: &MoveResult) -> Self {
        Self {
            timestamp: Local::now(),
            operation: OperationType::Move,
            source_docs: vec![result.doc_id.clone()],
            result_docs: vec![result.doc_id.clone()],
            fact_mappings: None,
            orphans: Vec::new(),
            orphan_path: None,
            details: Some(OperationDetails::Move(MoveDetails {
                old_path: result.old_path.clone(),
                new_path: result.new_path.clone(),
                old_type: result.old_type.clone(),
                new_type: result.new_type.clone(),
            })),
        }
    }

    /// Create an audit entry for a retype operation.
    pub(crate) fn from_retype(result: &RetypeResult) -> Self {
        Self {
            timestamp: Local::now(),
            operation: OperationType::Retype,
            source_docs: vec![result.doc_id.clone()],
            result_docs: vec![result.doc_id.clone()],
            fact_mappings: None,
            orphans: Vec::new(),
            orphan_path: None,
            details: Some(OperationDetails::Retype(RetypeDetails {
                old_type: result.old_type.clone(),
                new_type: result.new_type.clone(),
                persisted_to_file: result.persisted_to_file,
            })),
        }
    }
}

/// Extract fact mappings and orphans from a ledger.
fn extract_mappings_and_orphans(ledger: &FactLedger) -> (Vec<FactMapping>, Vec<FactSummary>) {
    use crate::organize::FactDestination;

    let mut mappings = Vec::new();
    let mut orphans = Vec::new();

    for fact in &ledger.source_facts {
        if let Some(assignment) = ledger.assignments.get(&fact.id) {
            let mapping = FactMapping {
                fact: FactSummary::from(fact),
                destination: assignment.destination.to_string(),
                target_doc: assignment.target_doc.clone(),
                reason: assignment.reason.clone(),
            };
            mappings.push(mapping);

            if assignment.destination == FactDestination::Orphan {
                orphans.push(FactSummary::from(fact));
            }
        }
    }

    (mappings, orphans)
}

/// Get the audit log directory path for a repository.
pub(crate) fn audit_log_dir(repo_path: &Path) -> PathBuf {
    repo_path.join(".factbase").join(AUDIT_LOG_DIR)
}

/// Generate a timestamped filename for an audit log entry.
fn generate_log_filename(timestamp: &DateTime<Local>, operation: OperationType) -> String {
    format!("{}-{}.yaml", timestamp.format("%Y%m%d-%H%M%S"), operation)
}

/// Write an audit entry to the log directory.
///
/// Creates the log directory if it doesn't exist.
///
/// # Arguments
/// * `entry` - The audit entry to write
/// * `repo_path` - Path to the repository root
///
/// # Returns
/// Path to the written log file.
pub(crate) fn write_audit_log(
    entry: &AuditEntry,
    repo_path: &Path,
) -> Result<PathBuf, FactbaseError> {
    let log_dir = audit_log_dir(repo_path);
    fs::create_dir_all(&log_dir)?;

    let filename = generate_log_filename(&entry.timestamp, entry.operation);
    let log_path = log_dir.join(&filename);

    let yaml = serde_yaml_ng::to_string(entry)
        .map_err(|e| FactbaseError::internal(format!("Failed to serialize audit entry: {e}")))?;

    fs::write(&log_path, yaml)?;

    Ok(log_path)
}

/// List all audit log files in a repository.
pub(crate) fn list_audit_logs(repo_path: &Path) -> Result<Vec<PathBuf>, FactbaseError> {
    let log_dir = audit_log_dir(repo_path);

    if !log_dir.exists() {
        return Ok(Vec::new());
    }

    let mut logs: Vec<PathBuf> = fs::read_dir(&log_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();

    // Sort by filename (which includes timestamp)
    logs.sort();

    Ok(logs)
}

/// Read an audit log entry from a file.
pub(crate) fn read_audit_log(path: &Path) -> Result<AuditEntry, FactbaseError> {
    let content = fs::read_to_string(path)?;
    let entry: AuditEntry = serde_yaml_ng::from_str(&content)
        .map_err(|e| FactbaseError::internal(format!("Failed to parse audit log: {e}")))?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organize::{FactDestination, TrackedFact};
    use tempfile::TempDir;

    #[test]
    fn test_operation_type_display() {
        assert_eq!(OperationType::Merge.to_string(), "merge");
        assert_eq!(OperationType::Split.to_string(), "split");
        assert_eq!(OperationType::Move.to_string(), "move");
        assert_eq!(OperationType::Retype.to_string(), "retype");
    }

    #[test]
    fn test_fact_summary_from_tracked_fact() {
        let fact = TrackedFact::new("doc1", 5, "Short content", None, vec![]);
        let summary = FactSummary::from(&fact);
        assert_eq!(summary.source_doc, "doc1");
        assert_eq!(summary.source_line, 5);
        assert_eq!(summary.content_preview, "Short content");
    }

    #[test]
    fn test_fact_summary_truncates_long_content() {
        let long_content = "A".repeat(100);
        let fact = TrackedFact::new("doc1", 1, &long_content, None, vec![]);
        let summary = FactSummary::from(&fact);
        assert_eq!(summary.content_preview.len(), 80);
        assert!(summary.content_preview.ends_with("..."));
    }

    #[test]
    fn test_audit_entry_from_merge() {
        let result = MergeResult {
            kept_id: "abc123".to_string(),
            merged_ids: vec!["def456".to_string()],
            fact_count: 5,
            duplicate_count: 1,
            orphan_count: 0,
            orphan_path: None,
            links_redirected: 2,
        };
        let ledger = FactLedger::new();

        let entry = AuditEntry::from_merge(&result, &ledger);
        assert_eq!(entry.operation, OperationType::Merge);
        assert_eq!(entry.source_docs, vec!["abc123", "def456"]);
        assert_eq!(entry.result_docs, vec!["abc123"]);
    }

    #[test]
    fn test_audit_entry_from_split() {
        let result = SplitResult {
            source_id: "abc123".to_string(),
            new_doc_ids: vec!["def456".to_string(), "ghi789".to_string()],
            fact_count: 10,
            orphan_count: 1,
            orphan_path: Some(PathBuf::from("_orphans.md")),
        };
        let ledger = FactLedger::new();

        let entry = AuditEntry::from_split(&result, &ledger);
        assert_eq!(entry.operation, OperationType::Split);
        assert_eq!(entry.source_docs, vec!["abc123"]);
        assert_eq!(entry.result_docs, vec!["def456", "ghi789"]);
        assert!(entry.orphan_path.is_some());
    }

    #[test]
    fn test_audit_entry_from_move() {
        let result = MoveResult {
            doc_id: "abc123".to_string(),
            old_path: "notes/doc.md".to_string(),
            new_path: "projects/doc.md".to_string(),
            old_type: Some("note".to_string()),
            new_type: "project".to_string(),
        };

        let entry = AuditEntry::from_move(&result);
        assert_eq!(entry.operation, OperationType::Move);
        assert_eq!(entry.source_docs, vec!["abc123"]);
        assert!(entry.fact_mappings.is_none());
    }

    #[test]
    fn test_audit_entry_from_retype() {
        let result = RetypeResult {
            doc_id: "abc123".to_string(),
            old_type: Some("note".to_string()),
            new_type: "person".to_string(),
            persisted_to_file: true,
        };

        let entry = AuditEntry::from_retype(&result);
        assert_eq!(entry.operation, OperationType::Retype);
        assert!(entry.orphans.is_empty());
    }

    #[test]
    fn test_generate_log_filename() {
        use chrono::TimeZone;
        let timestamp = Local.with_ymd_and_hms(2026, 2, 3, 14, 30, 45).unwrap();
        let filename = generate_log_filename(&timestamp, OperationType::Merge);
        assert_eq!(filename, "20260203-143045-merge.yaml");
    }

    #[test]
    fn test_write_and_read_audit_log() {
        let temp = TempDir::new().unwrap();
        let result = MoveResult {
            doc_id: "abc123".to_string(),
            old_path: "old/path.md".to_string(),
            new_path: "new/path.md".to_string(),
            old_type: Some("note".to_string()),
            new_type: "project".to_string(),
        };

        let entry = AuditEntry::from_move(&result);
        let log_path = write_audit_log(&entry, temp.path()).unwrap();

        assert!(log_path.exists());
        assert!(log_path.to_string_lossy().contains("move.yaml"));

        let read_entry = read_audit_log(&log_path).unwrap();
        assert_eq!(read_entry.operation, OperationType::Move);
        assert_eq!(read_entry.source_docs, vec!["abc123"]);
    }

    #[test]
    fn test_list_audit_logs_empty() {
        let temp = TempDir::new().unwrap();
        let logs = list_audit_logs(temp.path()).unwrap();
        assert!(logs.is_empty());
    }

    #[test]
    fn test_list_audit_logs_sorted() {
        let temp = TempDir::new().unwrap();
        let log_dir = audit_log_dir(temp.path());
        fs::create_dir_all(&log_dir).unwrap();

        // Create logs out of order
        fs::write(log_dir.join("20260203-140000-merge.yaml"), "test: 1").unwrap();
        fs::write(log_dir.join("20260201-100000-split.yaml"), "test: 2").unwrap();
        fs::write(log_dir.join("20260202-120000-move.yaml"), "test: 3").unwrap();

        let logs = list_audit_logs(temp.path()).unwrap();
        assert_eq!(logs.len(), 3);
        // Should be sorted by filename (timestamp)
        assert!(logs[0].to_string_lossy().contains("20260201"));
        assert!(logs[1].to_string_lossy().contains("20260202"));
        assert!(logs[2].to_string_lossy().contains("20260203"));
    }

    #[test]
    fn test_extract_mappings_and_orphans() {
        let mut ledger = FactLedger::new();
        let fact1 = TrackedFact::new("doc1", 1, "Fact one", None, vec![]);
        let fact2 = TrackedFact::new("doc1", 2, "Fact two", None, vec![]);
        let fact1_id = fact1.id.clone();
        let fact2_id = fact2.id.clone();

        ledger.add_fact(fact1);
        ledger.add_fact(fact2);
        ledger.assign(
            &fact1_id,
            FactDestination::Document,
            Some("target".to_string()),
            None,
        );
        ledger.assign(
            &fact2_id,
            FactDestination::Orphan,
            None,
            Some("No match".to_string()),
        );

        let (mappings, orphans) = extract_mappings_and_orphans(&ledger);
        assert_eq!(mappings.len(), 2);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].content_preview, "Fact two");
    }

    #[test]
    fn test_audit_log_dir() {
        let repo_path = Path::new("/home/user/repo");
        let log_dir = audit_log_dir(repo_path);
        assert_eq!(
            log_dir,
            PathBuf::from("/home/user/repo/.factbase/reorg-log")
        );
    }
}
