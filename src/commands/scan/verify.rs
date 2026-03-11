//! Document verification and fix logic.
//!
//! Contains `cmd_scan_verify` and fix helper functions.

use anyhow::bail;
use factbase::database::Database;
use factbase::models::Repository;
use factbase::output::format_json;
use factbase::processor::content_hash;
use serde::Serialize;
use std::fmt;
use std::fs;
use std::path::Path;

/// Type of verification issue found in a document.
#[derive(Serialize, Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) enum IssueType {
    #[serde(rename = "missing_file")]
    MissingFile,
    #[serde(rename = "read_error")]
    ReadError,
    #[serde(rename = "modified")]
    Modified,
    #[serde(rename = "missing_header")]
    MissingHeader,
    #[serde(rename = "id_mismatch")]
    IdMismatch,
}

impl IssueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingFile => "missing_file",
            Self::ReadError => "read_error",
            Self::Modified => "modified",
            Self::MissingHeader => "missing_header",
            Self::IdMismatch => "id_mismatch",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::MissingFile | Self::IdMismatch | Self::ReadError => "✗",
            Self::Modified | Self::MissingHeader => "⚠",
        }
    }
}

impl fmt::Display for IssueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single verification issue found in a document.
#[derive(Serialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct VerifyIssue {
    pub doc_id: String,
    pub title: String,
    pub file_path: String,
    pub repo_id: String,
    pub issue_type: IssueType,
    pub message: String,
    pub fixable: bool,
}

/// Result of attempting to fix an issue.
#[derive(Serialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct FixResult {
    pub doc_id: String,
    pub issue_type: IssueType,
    pub success: bool,
    pub message: String,
}

/// Overall verification result.
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct VerifyResult {
    pub total_documents: usize,
    pub verified_ok: usize,
    pub issues: Vec<VerifyIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed: Vec<FixResult>,
}

/// Verify document integrity without re-indexing
pub(super) fn cmd_scan_verify(
    db: &Database,
    repos: &[Repository],
    json_output: bool,
    quiet: bool,
    fix: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let mut total_documents = 0;
    let mut verified_ok = 0;
    let mut issues: Vec<VerifyIssue> = Vec::with_capacity(16); // Typical verify finds few issues

    for repo in repos {
        let docs = db.get_documents_for_repo(&repo.id)?;
        let repo_path = Path::new(&repo.path);

        for (_, doc) in docs {
            if doc.is_deleted {
                continue;
            }
            total_documents += 1;

            // Extract owned values from doc once - avoids repeated clones
            let doc_id = doc.id;
            let title = doc.title;
            let file_path = doc.file_path;
            let repo_id = repo.id.clone(); // repo is borrowed, must clone once per doc
            let db_hash = doc.file_hash;

            // Build full file path
            let full_path = repo_path.join(&file_path);

            // Check 1: File exists
            if !full_path.exists() {
                issues.push(VerifyIssue {
                    doc_id,
                    title,
                    file_path,
                    repo_id,
                    issue_type: IssueType::MissingFile,
                    message: format!("File not found: {}", full_path.display()),
                    fixable: false, // Cannot fix - file doesn't exist
                });
                continue;
            }

            // Read file content
            let content = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(e) => {
                    issues.push(VerifyIssue {
                        doc_id,
                        title,
                        file_path,
                        repo_id,
                        issue_type: IssueType::ReadError,
                        message: format!("Cannot read file: {e}"),
                        fixable: false,
                    });
                    continue;
                }
            };

            // Check 2: Content hash matches
            let current_hash = content_hash(&content);
            let hash_mismatch = current_hash != db_hash;

            // Check 3: ID header present and matches
            let expected_header = format!("<!-- factbase:{doc_id} -->");
            let has_expected_header = content.contains(&expected_header);
            let has_any_header = content.contains("<!-- factbase:");

            // Determine issues - a doc can have both hash mismatch AND header issue
            match (hash_mismatch, has_expected_header, has_any_header) {
                // No issues
                (false, true, _) => {
                    verified_ok += 1;
                }
                // Only hash mismatch (header is fine)
                (true, true, _) => {
                    issues.push(VerifyIssue {
                        doc_id,
                        title,
                        file_path,
                        repo_id,
                        issue_type: IssueType::Modified,
                        message: "File content has changed since last scan".to_string(),
                        fixable: true,
                    });
                }
                // Missing header (no factbase header at all)
                (hash_mismatch, false, false) => {
                    if hash_mismatch {
                        // Both issues - need to clone for second issue
                        issues.push(VerifyIssue {
                            doc_id: doc_id.clone(),
                            title: title.clone(),
                            file_path: file_path.clone(),
                            repo_id: repo_id.clone(),
                            issue_type: IssueType::Modified,
                            message: "File content has changed since last scan".to_string(),
                            fixable: true,
                        });
                    }
                    issues.push(VerifyIssue {
                        doc_id,
                        title,
                        file_path,
                        repo_id,
                        issue_type: IssueType::MissingHeader,
                        message: "File is missing factbase ID header".to_string(),
                        fixable: true,
                    });
                }
                // ID mismatch (has different factbase header)
                (hash_mismatch, false, true) => {
                    // Pre-create message using reference, then move doc_id into struct
                    let id_mismatch_msg =
                        format!("File has different factbase ID (expected {})", &doc_id);

                    if hash_mismatch {
                        // Both issues - need to clone for first issue
                        issues.push(VerifyIssue {
                            doc_id: doc_id.clone(),
                            title: title.clone(),
                            file_path: file_path.clone(),
                            repo_id: repo_id.clone(),
                            issue_type: IssueType::Modified,
                            message: "File content has changed since last scan".to_string(),
                            fixable: true,
                        });
                    }
                    issues.push(VerifyIssue {
                        doc_id, // Move instead of clone
                        title,
                        file_path,
                        repo_id,
                        issue_type: IssueType::IdMismatch,
                        message: id_mismatch_msg,
                        fixable: false,
                    });
                }
            }
        }
    }

    // Handle --fix mode
    let mut fixed: Vec<FixResult> = Vec::with_capacity(issues.len()); // At most one fix per issue
    if fix && !issues.is_empty() {
        // Partition issues into fixable and non-fixable
        let (fixable_issues, unfixable_issues): (Vec<_>, Vec<_>) =
            issues.into_iter().partition(|i| i.fixable);

        if fixable_issues.is_empty() {
            if !quiet {
                println!("No fixable issues found.");
            }
            issues = unfixable_issues;
        } else {
            // Prompt for confirmation unless --yes
            let proceed = if yes {
                true
            } else if !quiet {
                println!("\nFound {} fixable issue(s):", fixable_issues.len());
                for issue in &fixable_issues {
                    println!(
                        "  - [{}] {} [{}]",
                        issue.issue_type, issue.title, issue.doc_id
                    );
                }
                print!("\nProceed with fixes? [y/N] ");
                use std::io::{self, Write};
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                input.trim().eq_ignore_ascii_case("y")
            } else {
                false
            };

            if proceed {
                // Consume fixable_issues - move strings into FixResult
                for issue in fixable_issues {
                    let repo = repos.iter().find(|r| r.id == issue.repo_id);
                    let repo_path = repo.map(|r| Path::new(&r.path));

                    let result = match issue.issue_type {
                        IssueType::MissingHeader => {
                            fix_missing_header(repo_path, &issue.file_path, &issue.doc_id)
                        }
                        IssueType::Modified => {
                            fix_modified_content(db, &issue.doc_id, repo_path, &issue.file_path)
                        }
                        _ => bail!("Unknown fixable issue type"),
                    };

                    // Move doc_id and issue_type from issue into FixResult
                    let (success, message) = match result {
                        Ok(msg) => {
                            if !quiet {
                                println!(
                                    "✓ Fixed [{}] {} [{}]: {}",
                                    issue.issue_type, issue.title, issue.doc_id, msg
                                );
                            }
                            (true, msg)
                        }
                        Err(e) => {
                            if !quiet {
                                println!(
                                    "✗ Failed [{}] {} [{}]: {}",
                                    issue.issue_type, issue.title, issue.doc_id, e
                                );
                            }
                            (false, e.to_string())
                        }
                    };

                    fixed.push(FixResult {
                        doc_id: issue.doc_id,
                        issue_type: issue.issue_type,
                        success,
                        message,
                    });
                }
                issues = unfixable_issues;
            } else {
                // User declined - recombine issues
                issues = unfixable_issues;
                issues.extend(fixable_issues);
            }
        }
    }

    // Calculate counts before moving vectors
    let issues_len = issues.len();
    let successful_fixes = fixed.iter().filter(|f| f.success).count();

    let result = VerifyResult {
        total_documents,
        verified_ok,
        issues,
        fixed,
    };

    if json_output {
        println!("{}", format_json(&result)?);
    } else if !quiet && !fix {
        // Only show summary if not in fix mode (fix mode shows its own output)
        println!("Verification Results");
        println!("====================");
        println!("Total documents: {total_documents}");
        println!("Verified OK: {verified_ok}");
        println!("Issues found: {issues_len}");

        if !result.issues.is_empty() {
            println!();
            for issue in &result.issues {
                let icon = issue.issue_type.icon();
                let fixable_marker = if issue.fixable { " [fixable]" } else { "" };
                println!(
                    "{} [{}] {} [{}]: {}{}",
                    icon,
                    issue.issue_type,
                    issue.title,
                    issue.doc_id,
                    issue.message,
                    fixable_marker
                );
            }
        }
    } else if !quiet && fix {
        // Show fix summary
        println!("\nFix Summary");
        println!("===========");
        let failed = result.fixed.iter().filter(|f| !f.success).count();
        println!("Fixed: {successful_fixes}");
        println!("Failed: {failed}");
        println!("Remaining issues: {issues_len}");
    }

    // Exit with error if any unfixed issues remain (useful for CI)
    if issues_len > 0 {
        anyhow::bail!("{issues_len} unfixed issue(s) remain")
    }

    Ok(())
}

/// Fix missing_header issue by injecting factbase ID header into file
fn fix_missing_header(
    repo_path: Option<&Path>,
    file_path: &str,
    doc_id: &str,
) -> anyhow::Result<String> {
    let full_path = repo_path
        .ok_or_else(super::super::repo_path_not_found_error)?
        .join(file_path);

    let content = fs::read_to_string(&full_path)?;
    let header = format!("<!-- factbase:{doc_id} -->\n");
    let new_content = format!("{header}{content}");

    // Write to temp file first, then rename (atomic)
    let temp_path = full_path.with_extension("tmp");
    fs::write(&temp_path, &new_content)?;
    fs::rename(&temp_path, &full_path)?;

    Ok("Injected factbase ID header".to_string())
}

/// Fix modified issue by updating database hash to match current file content
fn fix_modified_content(
    db: &Database,
    doc_id: &str,
    repo_path: Option<&Path>,
    file_path: &str,
) -> anyhow::Result<String> {
    let full_path = repo_path
        .ok_or_else(super::super::repo_path_not_found_error)?
        .join(file_path);

    let content = fs::read_to_string(&full_path)?;
    let new_hash = content_hash(&content);

    // Update the document's hash in the database
    db.update_document_hash(doc_id, &new_hash)?;

    Ok("Updated database hash to match file content".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_issue_serialization() {
        let issue = VerifyIssue {
            doc_id: "abc123".to_string(),
            title: "Test Doc".to_string(),
            file_path: "docs/test.md".to_string(),
            repo_id: "main".to_string(),
            issue_type: IssueType::MissingFile,
            message: "File not found".to_string(),
            fixable: false,
        };

        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains(r#""doc_id":"abc123""#));
        assert!(json.contains(r#""title":"Test Doc""#));
        assert!(json.contains(r#""file_path":"docs/test.md""#));
        assert!(json.contains(r#""repo_id":"main""#));
        assert!(json.contains(r#""issue_type":"missing_file""#));
        assert!(json.contains(r#""message":"File not found""#));
        assert!(json.contains(r#""fixable":false"#));
    }

    #[test]
    fn test_verify_issue_fixable_true() {
        let issue = VerifyIssue {
            doc_id: "def456".to_string(),
            title: "Fixable Doc".to_string(),
            file_path: "notes/fix.md".to_string(),
            repo_id: "notes".to_string(),
            issue_type: IssueType::MissingHeader,
            message: "File is missing factbase ID header".to_string(),
            fixable: true,
        };

        let json = serde_json::to_string(&issue).unwrap();
        assert!(json.contains(r#""fixable":true"#));
        assert!(json.contains(r#""issue_type":"missing_header""#));
    }

    #[test]
    fn test_fix_result_success() {
        let result = FixResult {
            doc_id: "abc123".to_string(),
            issue_type: IssueType::MissingHeader,
            success: true,
            message: "Injected factbase ID header".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""doc_id":"abc123""#));
        assert!(json.contains(r#""issue_type":"missing_header""#));
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""message":"Injected factbase ID header""#));
    }

    #[test]
    fn test_fix_result_failure() {
        let result = FixResult {
            doc_id: "xyz789".to_string(),
            issue_type: IssueType::Modified,
            success: false,
            message: "Permission denied".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""success":false"#));
        assert!(json.contains(r#""message":"Permission denied""#));
    }

    #[test]
    fn test_verify_result_default() {
        let result = VerifyResult::default();

        assert_eq!(result.total_documents, 0);
        assert_eq!(result.verified_ok, 0);
        assert!(result.issues.is_empty());
        assert!(result.fixed.is_empty());
    }

    #[test]
    fn test_verify_result_serialization_no_fixed() {
        let result = VerifyResult {
            total_documents: 10,
            verified_ok: 8,
            issues: vec![VerifyIssue {
                doc_id: "abc123".to_string(),
                title: "Test".to_string(),
                file_path: "test.md".to_string(),
                repo_id: "main".to_string(),
                issue_type: IssueType::Modified,
                message: "Changed".to_string(),
                fixable: true,
            }],
            fixed: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""total_documents":10"#));
        assert!(json.contains(r#""verified_ok":8"#));
        assert!(json.contains(r#""issues":"#));
        // fixed should be omitted when empty (skip_serializing_if)
        assert!(!json.contains(r#""fixed""#));
    }

    #[test]
    fn test_verify_result_serialization_with_fixed() {
        let result = VerifyResult {
            total_documents: 5,
            verified_ok: 4,
            issues: vec![],
            fixed: vec![FixResult {
                doc_id: "abc123".to_string(),
                issue_type: IssueType::MissingHeader,
                success: true,
                message: "Fixed".to_string(),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""total_documents":5"#));
        assert!(json.contains(r#""verified_ok":4"#));
        assert!(json.contains(r#""fixed""#));
        assert!(json.contains(r#""success":true"#));
    }

    #[test]
    fn test_verify_result_multiple_issues() {
        let result = VerifyResult {
            total_documents: 20,
            verified_ok: 15,
            issues: vec![
                VerifyIssue {
                    doc_id: "a1".to_string(),
                    title: "Doc A".to_string(),
                    file_path: "a.md".to_string(),
                    repo_id: "main".to_string(),
                    issue_type: IssueType::MissingFile,
                    message: "Not found".to_string(),
                    fixable: false,
                },
                VerifyIssue {
                    doc_id: "b2".to_string(),
                    title: "Doc B".to_string(),
                    file_path: "b.md".to_string(),
                    repo_id: "main".to_string(),
                    issue_type: IssueType::Modified,
                    message: "Changed".to_string(),
                    fixable: true,
                },
            ],
            fixed: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        // Verify both issues are present
        assert!(json.contains(r#""doc_id":"a1""#));
        assert!(json.contains(r#""doc_id":"b2""#));
        assert!(json.contains(r#""issue_type":"missing_file""#));
        assert!(json.contains(r#""issue_type":"modified""#));
    }
}
