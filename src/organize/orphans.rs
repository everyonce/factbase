//! Orphan fact management for reorganization operations.
//!
//! Orphans are facts that cannot be assigned to a destination document during
//! reorganization (merge, split). They are collected in `_orphans.md` for human review.

use crate::error::FactbaseError;
use crate::organize::fs_helpers::{read_file, write_file};
use crate::organize::TrackedFact;
use chrono::Utc;
use std::fmt;
use std::path::{Path, PathBuf};

/// Operation type for orphan grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanOperation {
    Merge,
    Split,
}

impl fmt::Display for OrphanOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrphanOperation::Merge => write!(f, "Merge"),
            OrphanOperation::Split => write!(f, "Split"),
        }
    }
}

/// Write orphaned facts to `_orphans.md` holding document.
///
/// Creates the file if it doesn't exist, or appends to it if it does.
/// Facts are grouped by operation with `@r[orphan]` review markers.
///
/// # Arguments
/// * `orphans` - Facts that could not be assigned during reorganization
/// * `repo_path` - Repository root path
/// * `operation` - Type of operation (Merge or Split)
/// * `operation_id` - ID of the source document(s) for attribution
///
/// # Returns
/// Path to the orphan document
pub fn write_orphans(
    orphans: &[&TrackedFact],
    repo_path: &Path,
    operation: OrphanOperation,
    operation_id: &str,
) -> Result<PathBuf, FactbaseError> {
    if orphans.is_empty() {
        return Err(FactbaseError::internal("No orphans to write"));
    }

    let orphan_path = repo_path.join("_orphans.md");
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

    let mut content = String::new();

    // If file exists, append to it
    if orphan_path.exists() {
        content = read_file(&orphan_path)?;
        content.push_str("\n\n");
    } else {
        // Create new file with header
        content.push_str("# Orphaned Facts\n\n");
        content.push_str("Facts that could not be assigned during reorganization.\n");
        content.push_str("Review and assign to appropriate documents.\n\n");
    }

    // Add section for this operation
    content.push_str(&format!(
        "## {} {} ({})\n\n",
        operation, operation_id, timestamp
    ));

    for fact in orphans {
        content.push_str(&format!(
            "- {} @r[orphan] <!-- from {} line {} -->\n",
            fact.content, fact.source_doc, fact.source_line
        ));
    }

    write_file(&orphan_path, &content)?;

    Ok(orphan_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_fact(id: &str, content: &str, source_doc: &str, line: usize) -> TrackedFact {
        TrackedFact {
            id: id.to_string(),
            source_doc: source_doc.to_string(),
            source_line: line,
            content: content.to_string(),
            temporal: None,
            sources: vec![],
        }
    }

    #[test]
    fn test_orphan_operation_display() {
        assert_eq!(format!("{}", OrphanOperation::Merge), "Merge");
        assert_eq!(format!("{}", OrphanOperation::Split), "Split");
    }

    #[test]
    fn test_write_orphans_empty_returns_error() {
        let temp = TempDir::new().unwrap();
        let result = write_orphans(&[], temp.path(), OrphanOperation::Merge, "abc123");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No orphans"));
    }

    #[test]
    fn test_write_orphans_creates_new_file() {
        let temp = TempDir::new().unwrap();
        let fact = make_fact("f1", "Orphaned fact content", "doc123", 5);
        let facts: Vec<&TrackedFact> = vec![&fact];

        let path = write_orphans(&facts, temp.path(), OrphanOperation::Merge, "abc123").unwrap();

        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Orphaned Facts"));
        assert!(content.contains("## Merge abc123"));
        assert!(content.contains("Orphaned fact content @r[orphan]"));
        assert!(content.contains("<!-- from doc123 line 5 -->"));
    }

    #[test]
    fn test_write_orphans_appends_to_existing() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");

        // Create initial file
        fs::write(&orphan_path, "# Orphaned Facts\n\nExisting content.\n").unwrap();

        let fact = make_fact("f1", "New orphan", "doc456", 10);
        let facts: Vec<&TrackedFact> = vec![&fact];

        let path = write_orphans(&facts, temp.path(), OrphanOperation::Split, "xyz789").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Existing content."));
        assert!(content.contains("## Split xyz789"));
        assert!(content.contains("New orphan @r[orphan]"));
    }

    #[test]
    fn test_write_orphans_multiple_facts() {
        let temp = TempDir::new().unwrap();
        let fact1 = make_fact("f1", "First orphan", "doc1", 1);
        let fact2 = make_fact("f2", "Second orphan", "doc1", 5);
        let fact3 = make_fact("f3", "Third orphan", "doc2", 3);
        let facts: Vec<&TrackedFact> = vec![&fact1, &fact2, &fact3];

        let path = write_orphans(&facts, temp.path(), OrphanOperation::Merge, "multi").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("First orphan @r[orphan]"));
        assert!(content.contains("Second orphan @r[orphan]"));
        assert!(content.contains("Third orphan @r[orphan]"));
        assert!(content.contains("<!-- from doc1 line 1 -->"));
        assert!(content.contains("<!-- from doc1 line 5 -->"));
        assert!(content.contains("<!-- from doc2 line 3 -->"));
    }

    #[test]
    fn test_write_orphans_preserves_temporal_in_content() {
        let temp = TempDir::new().unwrap();
        let fact = make_fact("f1", "Role at Company @t[2020..2022]", "person1", 3);
        let facts: Vec<&TrackedFact> = vec![&fact];

        let path = write_orphans(&facts, temp.path(), OrphanOperation::Merge, "test").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Role at Company @t[2020..2022] @r[orphan]"));
    }
}
