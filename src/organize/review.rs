//! Orphan review integration for reorganization operations.
//!
//! Allows assigning orphaned facts to documents via the review system.
//! Orphan entries use `@r[orphan]` marker and can be answered with:
//! - A document ID (6-char hex) to assign the fact to that document
//! - "dismiss" to remove the orphan without assigning

use crate::error::FactbaseError;
use crate::organize::fs_helpers::{read_file, remove_file, write_file};
use crate::patterns::{DOC_ID_REGEX, ORPHAN_ENTRY_REGEX, SIMPLE_ORPHAN_REGEX};
use crate::Database;
use std::path::{Path, PathBuf};

/// A parsed orphan entry from `_orphans.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrphanEntry {
    /// The fact content (without `@r[orphan]` marker)
    pub content: String,
    /// Source document ID (if available)
    pub source_doc: Option<String>,
    /// Source line number (if available)
    pub source_line: Option<usize>,
    /// Whether the entry has been answered
    pub answered: bool,
    /// The answer (document ID or "dismiss")
    pub answer: Option<String>,
    /// Line number in _orphans.md
    pub line_number: usize,
}

/// Result of processing orphan answers.
#[derive(Debug, Clone)]
pub struct OrphanProcessResult {
    /// Number of orphans assigned to documents
    pub assigned_count: usize,
    /// Number of orphans dismissed
    pub dismissed_count: usize,
    /// Number of orphans remaining (unanswered)
    pub remaining_count: usize,
    /// Documents that were modified
    pub modified_docs: Vec<String>,
}

/// Parse orphan entries from `_orphans.md` content.
///
/// Supports two formats:
/// 1. Checkbox format: `- [x] content @r[orphan] <!-- from doc line N --> → answer`
/// 2. Simple format: `- content @r[orphan] <!-- from doc line N -->`
///
/// For simple format, entries are considered unanswered.
pub fn parse_orphan_entries(content: &str) -> Vec<OrphanEntry> {
    let mut entries = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line = line.trim();
        let line_number = line_idx + 1;

        // Try checkbox format first
        if let Some(caps) = ORPHAN_ENTRY_REGEX.captures(line) {
            let checkbox = &caps[1];
            let fact_content = caps[2].trim().to_string();
            let source_doc = caps.get(3).map(|m| m.as_str().to_string());
            let source_line = caps.get(4).and_then(|m| m.as_str().parse().ok());
            let answer = caps.get(5).map(|m| m.as_str().trim().to_string());

            let checkbox_checked = checkbox == "x" || checkbox == "X";
            let answered = checkbox_checked && answer.is_some();

            entries.push(OrphanEntry {
                content: fact_content,
                source_doc,
                source_line,
                answered,
                answer: if answered { answer } else { None },
                line_number,
            });
        }
        // Try simple format (original write_orphans output)
        else if let Some(caps) = SIMPLE_ORPHAN_REGEX.captures(line) {
            let fact_content = caps[1].trim().to_string();
            let source_doc = caps.get(2).map(|m| m.as_str().to_string());
            let source_line = caps.get(3).and_then(|m| m.as_str().parse().ok());

            entries.push(OrphanEntry {
                content: fact_content,
                source_doc,
                source_line,
                answered: false,
                answer: None,
                line_number,
            });
        }
    }

    entries
}

/// Validate an orphan answer.
///
/// Valid answers:
/// - "dismiss" or "ignore" - remove orphan without assigning
/// - 6-character hex document ID - assign to that document
///
/// Returns the normalized answer or an error.
pub fn validate_orphan_answer(answer: &str) -> Result<OrphanAnswer, FactbaseError> {
    let answer = answer.trim().to_lowercase();

    if answer == "dismiss" || answer == "ignore" {
        return Ok(OrphanAnswer::Dismiss);
    }

    if DOC_ID_REGEX.is_match(&answer) {
        return Ok(OrphanAnswer::AssignTo(answer));
    }

    Err(FactbaseError::parse(format!(
        "Invalid orphan answer '{}'. Expected document ID (6-char hex) or 'dismiss'",
        answer
    )))
}

/// Validated orphan answer.
#[derive(Debug, Clone, PartialEq)]
pub enum OrphanAnswer {
    /// Dismiss the orphan without assigning
    Dismiss,
    /// Assign to a specific document
    AssignTo(String),
}

/// Process answered orphan entries.
///
/// For each answered orphan:
/// - If "dismiss": remove from orphan file
/// - If document ID: append fact to that document and remove from orphan file
///
/// # Arguments
/// * `repo_path` - Repository root path
/// * `db` - Database for document lookup
///
/// # Returns
/// Result with counts of processed orphans
pub fn process_orphan_answers(
    repo_path: &Path,
    db: &Database,
) -> Result<OrphanProcessResult, FactbaseError> {
    let orphan_path = repo_path.join("_orphans.md");

    if !orphan_path.exists() {
        return Ok(OrphanProcessResult {
            assigned_count: 0,
            dismissed_count: 0,
            remaining_count: 0,
            modified_docs: vec![],
        });
    }

    let content = read_file(&orphan_path)?;

    let entries = parse_orphan_entries(&content);

    let mut assigned_count = 0;
    let mut dismissed_count = 0;
    let mut remaining_count = 0;
    let mut modified_docs = Vec::new();
    let mut lines_to_remove = Vec::new();

    for entry in &entries {
        if !entry.answered {
            remaining_count += 1;
            continue;
        }

        let answer = entry.answer.as_ref().expect("checked answered above");
        match validate_orphan_answer(answer) {
            Ok(OrphanAnswer::Dismiss) => {
                dismissed_count += 1;
                lines_to_remove.push(entry.line_number);
            }
            Ok(OrphanAnswer::AssignTo(doc_id)) => {
                // Verify document exists
                if let Some(doc) = db.get_document(&doc_id)? {
                    // Append fact to document
                    let doc_path = repo_path.join(&doc.file_path);
                    append_fact_to_document(&doc_path, &entry.content)?;

                    assigned_count += 1;
                    lines_to_remove.push(entry.line_number);
                    if !modified_docs.contains(&doc_id) {
                        modified_docs.push(doc_id);
                    }
                } else {
                    // Document not found - leave orphan in place
                    remaining_count += 1;
                }
            }
            Err(_) => {
                // Invalid answer - leave orphan in place
                remaining_count += 1;
            }
        }
    }

    // Remove processed lines from orphan file
    if !lines_to_remove.is_empty() {
        let new_content = remove_lines(&content, &lines_to_remove);
        write_orphan_file(&orphan_path, &new_content)?;
    }

    Ok(OrphanProcessResult {
        assigned_count,
        dismissed_count,
        remaining_count,
        modified_docs,
    })
}

/// Append a fact to a document file.
fn append_fact_to_document(doc_path: &Path, fact_content: &str) -> Result<(), FactbaseError> {
    let mut content = read_file(doc_path)?;

    // Ensure content ends with newline
    if !content.ends_with('\n') {
        content.push('\n');
    }

    // Append fact as list item (remove @r[orphan] marker if present)
    let clean_fact = fact_content.replace("@r[orphan]", "").trim().to_string();
    content.push_str(&format!("- {}\n", clean_fact));

    write_file(doc_path, &content)?;

    Ok(())
}

/// Remove specific lines from content.
fn remove_lines(content: &str, line_numbers: &[usize]) -> String {
    content
        .lines()
        .enumerate()
        .filter(|(idx, _)| !line_numbers.contains(&(idx + 1)))
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write orphan file, removing it if empty.
fn write_orphan_file(path: &Path, content: &str) -> Result<(), FactbaseError> {
    // Check if content is effectively empty (only whitespace and headers)
    let has_orphans = content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("Facts that could not")
            && !trimmed.starts_with("Review and assign")
    });

    if !has_orphans {
        // Remove empty orphan file
        remove_file(path)?;
    } else {
        write_file(path, content)?;
    }

    Ok(())
}

/// Get the path to the orphan file for a repository.
pub fn orphan_file_path(repo_path: &Path) -> PathBuf {
    repo_path.join("_orphans.md")
}

/// Load and parse orphan entries from a repository's `_orphans.md` file.
///
/// Returns an empty vec if the orphan file doesn't exist.
pub fn load_orphan_entries(repo_path: &Path) -> Result<Vec<OrphanEntry>, FactbaseError> {
    let orphan_path = orphan_file_path(repo_path);
    if !orphan_path.exists() {
        return Ok(vec![]);
    }
    let content = read_file(&orphan_path)?;
    Ok(parse_orphan_entries(&content))
}

/// Check if a repository has orphan entries.
pub fn has_orphans(repo_path: &Path) -> bool {
    load_orphan_entries(repo_path).is_ok_and(|e| !e.is_empty())
}

/// Count orphan entries in a repository.
pub fn count_orphans(repo_path: &Path) -> Result<(usize, usize), FactbaseError> {
    let entries = load_orphan_entries(repo_path)?;
    let total = entries.len();
    let answered = entries.iter().filter(|e| e.answered).count();
    Ok((total, answered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ============================================================================
    // Parsing Tests
    // ============================================================================

    #[test]
    fn test_parse_orphan_entries_empty() {
        let content = "# Orphaned Facts\n\nNo orphans here.\n";
        let entries = parse_orphan_entries(content);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_orphan_entries_simple_format() {
        let content = "- Some fact content @r[orphan] <!-- from abc123 line 5 -->\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "Some fact content");
        assert_eq!(entries[0].source_doc, Some("abc123".to_string()));
        assert_eq!(entries[0].source_line, Some(5));
        assert!(!entries[0].answered);
        assert!(entries[0].answer.is_none());
    }

    #[test]
    fn test_parse_orphan_entries_checkbox_unanswered() {
        let content = "- [ ] Some fact @r[orphan] <!-- from abc123 line 5 -->\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "Some fact");
        assert!(!entries[0].answered);
    }

    #[test]
    fn test_parse_orphan_entries_checkbox_answered_dismiss() {
        let content = "- [x] Some fact @r[orphan] <!-- from abc123 line 5 --> → dismiss\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].answered);
        assert_eq!(entries[0].answer, Some("dismiss".to_string()));
    }

    #[test]
    fn test_parse_orphan_entries_checkbox_answered_doc_id() {
        let content = "- [X] Some fact @r[orphan] <!-- from abc123 line 5 --> → def456\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].answered);
        assert_eq!(entries[0].answer, Some("def456".to_string()));
    }

    #[test]
    fn test_parse_orphan_entries_multiple() {
        let content = r#"# Orphaned Facts

## Merge abc123 (2026-02-02)

- [x] First fact @r[orphan] <!-- from abc123 line 1 --> → def456
- [ ] Second fact @r[orphan] <!-- from abc123 line 2 -->
- [x] Third fact @r[orphan] <!-- from abc123 line 3 --> → dismiss
"#;
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 3);

        assert!(entries[0].answered);
        assert_eq!(entries[0].answer, Some("def456".to_string()));

        assert!(!entries[1].answered);
        assert!(entries[1].answer.is_none());

        assert!(entries[2].answered);
        assert_eq!(entries[2].answer, Some("dismiss".to_string()));
    }

    #[test]
    fn test_parse_orphan_entries_preserves_temporal_tags() {
        let content = "- [x] Role at Company @t[2020..2022] @r[orphan] <!-- from abc123 line 5 --> → def456\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].content.contains("@t[2020..2022]"));
    }

    #[test]
    fn test_parse_orphan_entries_no_source_info() {
        let content = "- [x] Fact without source @r[orphan] → dismiss\n";
        let entries = parse_orphan_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].source_doc.is_none());
        assert!(entries[0].source_line.is_none());
    }

    // ============================================================================
    // Answer Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_orphan_answer_dismiss() {
        assert_eq!(
            validate_orphan_answer("dismiss").unwrap(),
            OrphanAnswer::Dismiss
        );
        assert_eq!(
            validate_orphan_answer("DISMISS").unwrap(),
            OrphanAnswer::Dismiss
        );
        assert_eq!(
            validate_orphan_answer("ignore").unwrap(),
            OrphanAnswer::Dismiss
        );
    }

    #[test]
    fn test_validate_orphan_answer_doc_id() {
        assert_eq!(
            validate_orphan_answer("abc123").unwrap(),
            OrphanAnswer::AssignTo("abc123".to_string())
        );
        assert_eq!(
            validate_orphan_answer("ABC123").unwrap(),
            OrphanAnswer::AssignTo("abc123".to_string())
        );
    }

    #[test]
    fn test_validate_orphan_answer_invalid() {
        assert!(validate_orphan_answer("invalid").is_err());
        assert!(validate_orphan_answer("abc12").is_err()); // Too short
        assert!(validate_orphan_answer("abc1234").is_err()); // Too long
        assert!(validate_orphan_answer("ghijkl").is_err()); // Not hex
    }

    // ============================================================================
    // Line Removal Tests
    // ============================================================================

    #[test]
    fn test_remove_lines_single() {
        let content = "line 1\nline 2\nline 3";
        let result = remove_lines(content, &[2]);
        assert_eq!(result, "line 1\nline 3");
    }

    #[test]
    fn test_remove_lines_multiple() {
        let content = "line 1\nline 2\nline 3\nline 4";
        let result = remove_lines(content, &[1, 3]);
        assert_eq!(result, "line 2\nline 4");
    }

    #[test]
    fn test_remove_lines_empty() {
        let content = "line 1\nline 2";
        let result = remove_lines(content, &[]);
        assert_eq!(result, "line 1\nline 2");
    }

    // ============================================================================
    // Append Fact Tests
    // ============================================================================

    #[test]
    fn test_append_fact_to_document() {
        let temp = TempDir::new().unwrap();
        let doc_path = temp.path().join("test.md");
        fs::write(&doc_path, "# Test\n\n- Existing fact\n").unwrap();

        append_fact_to_document(&doc_path, "New fact @t[2024]").unwrap();

        let content = fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("- Existing fact"));
        assert!(content.contains("- New fact @t[2024]"));
    }

    #[test]
    fn test_append_fact_removes_orphan_marker() {
        let temp = TempDir::new().unwrap();
        let doc_path = temp.path().join("test.md");
        fs::write(&doc_path, "# Test\n").unwrap();

        append_fact_to_document(&doc_path, "Fact @r[orphan] content").unwrap();

        let content = fs::read_to_string(&doc_path).unwrap();
        assert!(content.contains("- Fact  content")); // @r[orphan] removed
        assert!(!content.contains("@r[orphan]"));
    }

    // ============================================================================
    // Orphan File Management Tests
    // ============================================================================

    #[test]
    fn test_orphan_file_path() {
        let repo_path = Path::new("/repo");
        assert_eq!(
            orphan_file_path(repo_path),
            PathBuf::from("/repo/_orphans.md")
        );
    }

    #[test]
    fn test_has_orphans_no_file() {
        let temp = TempDir::new().unwrap();
        assert!(!has_orphans(temp.path()));
    }

    #[test]
    fn test_has_orphans_empty_file() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");
        fs::write(&orphan_path, "# Orphaned Facts\n\nNo orphans.\n").unwrap();
        assert!(!has_orphans(temp.path()));
    }

    #[test]
    fn test_has_orphans_with_entries() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");
        fs::write(
            &orphan_path,
            "# Orphaned Facts\n\n- Fact @r[orphan] <!-- from abc123 line 1 -->\n",
        )
        .unwrap();
        assert!(has_orphans(temp.path()));
    }

    #[test]
    fn test_count_orphans() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");
        fs::write(
            &orphan_path,
            r#"# Orphaned Facts

- [x] Answered @r[orphan] → dismiss
- [ ] Unanswered @r[orphan]
- Unanswered simple @r[orphan]
"#,
        )
        .unwrap();

        let (total, answered) = count_orphans(temp.path()).unwrap();
        assert_eq!(total, 3);
        assert_eq!(answered, 1);
    }

    #[test]
    fn test_write_orphan_file_removes_empty() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");
        fs::write(&orphan_path, "# Orphaned Facts\n\nReview and assign.\n").unwrap();

        write_orphan_file(&orphan_path, "# Orphaned Facts\n\nReview and assign.\n").unwrap();

        assert!(!orphan_path.exists());
    }

    #[test]
    fn test_write_orphan_file_keeps_with_content() {
        let temp = TempDir::new().unwrap();
        let orphan_path = temp.path().join("_orphans.md");

        let content = "# Orphaned Facts\n\n- Fact @r[orphan]\n";
        write_orphan_file(&orphan_path, content).unwrap();

        assert!(orphan_path.exists());
        assert_eq!(fs::read_to_string(&orphan_path).unwrap(), content);
    }
}
