//! Statistics display helpers.
//!
//! Contains `cmd_scan_stats_only` and `cmd_scan_check` functions.

use factbase::{format_bytes, format_json, Config, Database, Repository, Scanner};
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

/// Statistics for a single repository.
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct RepoStats {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub type_distribution: HashMap<String, usize>,
}

/// Aggregate scan statistics across repositories.
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct ScanStats {
    pub repositories: Vec<RepoStats>,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub total_size_human: String,
}

/// Result of index integrity check.
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct CheckResult {
    pub valid: bool,
    pub total_documents: usize,
    pub documents_with_embeddings: usize,
    pub documents_without_embeddings: Vec<DocInfo>,
    pub orphaned_embeddings: usize,
    pub embedding_dimension: Option<usize>,
    pub expected_dimension: usize,
    pub dimension_mismatch: bool,
}

/// Document info for check results.
#[derive(Serialize, Default)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(super) struct DocInfo {
    pub id: String,
    pub title: String,
    pub repo_id: String,
}

/// Quick statistics without Ollama or database modifications
pub(super) fn cmd_scan_stats_only(
    repos: &[Repository],
    scanner: &Scanner,
    json_output: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut all_stats = ScanStats {
        repositories: Vec::with_capacity(repos.len()),
        total_files: 0,
        total_size_bytes: 0,
        total_size_human: String::new(),
    };

    for repo in repos {
        let files: Vec<PathBuf> = scanner.find_markdown_files(&repo.path);
        let mut type_distribution: HashMap<String, usize> = HashMap::new();
        let mut total_size: u64 = 0;

        for file in &files {
            // Get file size
            if let Ok(metadata) = fs::metadata(file) {
                total_size += metadata.len();
            }

            // Derive type from parent folder
            if let Some(parent) = file.parent() {
                let type_name = parent
                    .file_name()
                    .and_then(|n: &OsStr| n.to_str())
                    .unwrap_or("unknown")
                    .to_lowercase();
                *type_distribution.entry(type_name).or_insert(0) += 1;
            }
        }

        all_stats.total_files += files.len();
        all_stats.total_size_bytes += total_size;

        all_stats.repositories.push(RepoStats {
            id: repo.id.clone(),
            name: repo.name.clone(),
            path: repo.path.display().to_string(),
            file_count: files.len(),
            total_size_bytes: total_size,
            type_distribution,
        });
    }

    all_stats.total_size_human = format_bytes(all_stats.total_size_bytes);

    if json_output {
        println!("{}", format_json(&all_stats)?);
    } else if !quiet {
        println!("Scan Statistics");
        println!("===============\n");

        for repo_stats in &all_stats.repositories {
            println!("Repository: {} ({})", repo_stats.name, repo_stats.id);
            println!("  Path: {}", repo_stats.path);
            println!("  Files: {}", repo_stats.file_count);
            println!("  Size: {}", format_bytes(repo_stats.total_size_bytes));

            if !repo_stats.type_distribution.is_empty() {
                let mut types: Vec<_> = repo_stats.type_distribution.iter().collect();
                types.sort_by(|a, b| b.1.cmp(a.1));
                let type_str: Vec<_> = types
                    .iter()
                    .take(5)
                    .map(|(t, c)| format!("{}: {}", t, c))
                    .collect();
                println!("  Types: {}", type_str.join(", "));
                if types.len() > 5 {
                    println!("         ... and {} more types", types.len() - 5);
                }
            }
            println!();
        }

        if all_stats.repositories.len() > 1 {
            println!(
                "Total: {} files, {}",
                all_stats.total_files, all_stats.total_size_human
            );
        }
    }

    Ok(())
}

/// Validate index integrity for CI
pub(super) fn cmd_scan_check(
    db: &Database,
    config: &Config,
    repos: &[Repository],
    json_output: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let expected_dim = config.embedding.dimension;
    let mut total_docs = 0;
    let mut docs_with_emb = 0;
    let mut docs_without_emb: Vec<DocInfo> = Vec::with_capacity(repos.len() * 4); // Estimate few docs without embeddings
    let mut orphaned_count = 0;

    for repo in repos {
        let status = db.check_embedding_status(&repo.id)?;
        total_docs += status.with_embeddings.len() + status.without_embeddings.len();
        docs_with_emb += status.with_embeddings.len();
        for (id, title) in status.without_embeddings {
            docs_without_emb.push(DocInfo {
                id,
                title,
                repo_id: repo.id.clone(),
            });
        }
        orphaned_count += status.orphaned.len();
    }

    let actual_dim = db.get_embedding_dimension()?;
    let dimension_mismatch = actual_dim.is_some_and(|d| d != expected_dim);

    let valid = docs_without_emb.is_empty() && orphaned_count == 0 && !dimension_mismatch;

    let result = CheckResult {
        valid,
        total_documents: total_docs,
        documents_with_embeddings: docs_with_emb,
        documents_without_embeddings: docs_without_emb,
        orphaned_embeddings: orphaned_count,
        embedding_dimension: actual_dim,
        expected_dimension: expected_dim,
        dimension_mismatch,
    };

    if json_output {
        println!("{}", format_json(&result)?);
    } else if !quiet {
        if result.valid {
            println!("✓ {} documents indexed", result.total_documents);
            if let Some(dim) = result.embedding_dimension {
                println!("✓ All embeddings valid ({} dimensions)", dim);
            }
            println!("✓ No orphaned entries");
        } else {
            if !result.documents_without_embeddings.is_empty() {
                println!(
                    "✗ {} document(s) missing embeddings:",
                    result.documents_without_embeddings.len()
                );
                for doc in &result.documents_without_embeddings {
                    println!("  - {} [{}]", doc.title, doc.id);
                }
            }
            if result.dimension_mismatch {
                println!(
                    "✗ Embedding dimension mismatch: found {}, expected {}",
                    result.embedding_dimension.unwrap_or(0),
                    result.expected_dimension
                );
            }
            if result.orphaned_embeddings > 0 {
                println!("✗ {} orphaned embedding(s)", result.orphaned_embeddings);
            }
        }
    }

    if result.valid {
        Ok(())
    } else {
        anyhow::bail!("Database verification failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_stats_default() {
        let stats = RepoStats::default();
        assert_eq!(stats.id, "");
        assert_eq!(stats.name, "");
        assert_eq!(stats.path, "");
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.total_size_bytes, 0);
        assert!(stats.type_distribution.is_empty());
    }

    #[test]
    fn test_repo_stats_serialization() {
        let mut type_dist = HashMap::new();
        type_dist.insert("person".to_string(), 5);
        type_dist.insert("project".to_string(), 3);

        let stats = RepoStats {
            id: "main".to_string(),
            name: "My Repo".to_string(),
            path: "/home/user/notes".to_string(),
            file_count: 8,
            total_size_bytes: 12345,
            type_distribution: type_dist,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"id\":\"main\""));
        assert!(json.contains("\"name\":\"My Repo\""));
        assert!(json.contains("\"file_count\":8"));
        assert!(json.contains("\"total_size_bytes\":12345"));
        assert!(json.contains("\"person\":5"));
    }

    #[test]
    fn test_scan_stats_default() {
        let stats = ScanStats::default();
        assert!(stats.repositories.is_empty());
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_size_bytes, 0);
        assert_eq!(stats.total_size_human, "");
    }

    #[test]
    fn test_scan_stats_serialization() {
        let stats = ScanStats {
            repositories: vec![RepoStats {
                id: "test".to_string(),
                name: "Test".to_string(),
                path: "/test".to_string(),
                file_count: 10,
                total_size_bytes: 5000,
                type_distribution: HashMap::new(),
            }],
            total_files: 10,
            total_size_bytes: 5000,
            total_size_human: "4.9 KB".to_string(),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"repositories\":["));
        assert!(json.contains("\"total_files\":10"));
        assert!(json.contains("\"total_size_human\":\"4.9 KB\""));
    }

    #[test]
    fn test_check_result_default() {
        let result = CheckResult::default();
        assert!(!result.valid);
        assert_eq!(result.total_documents, 0);
        assert_eq!(result.documents_with_embeddings, 0);
        assert!(result.documents_without_embeddings.is_empty());
        assert_eq!(result.orphaned_embeddings, 0);
        assert!(result.embedding_dimension.is_none());
        assert_eq!(result.expected_dimension, 0);
        assert!(!result.dimension_mismatch);
    }

    #[test]
    fn test_check_result_valid_serialization() {
        let result = CheckResult {
            valid: true,
            total_documents: 50,
            documents_with_embeddings: 50,
            documents_without_embeddings: vec![],
            orphaned_embeddings: 0,
            embedding_dimension: Some(1024),
            expected_dimension: 1024,
            dimension_mismatch: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"valid\":true"));
        assert!(json.contains("\"total_documents\":50"));
        assert!(json.contains("\"embedding_dimension\":1024"));
        assert!(json.contains("\"dimension_mismatch\":false"));
    }

    #[test]
    fn test_check_result_with_missing_embeddings() {
        let result = CheckResult {
            valid: false,
            total_documents: 10,
            documents_with_embeddings: 8,
            documents_without_embeddings: vec![DocInfo {
                id: "abc123".to_string(),
                title: "Missing Doc".to_string(),
                repo_id: "main".to_string(),
            }],
            orphaned_embeddings: 0,
            embedding_dimension: Some(1024),
            expected_dimension: 1024,
            dimension_mismatch: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"valid\":false"));
        assert!(json.contains("\"documents_without_embeddings\":["));
        assert!(json.contains("\"id\":\"abc123\""));
        assert!(json.contains("\"title\":\"Missing Doc\""));
    }

    #[test]
    fn test_doc_info_serialization() {
        let info = DocInfo {
            id: "def456".to_string(),
            title: "Test Document".to_string(),
            repo_id: "notes".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":\"def456\""));
        assert!(json.contains("\"title\":\"Test Document\""));
        assert!(json.contains("\"repo_id\":\"notes\""));
    }
}
