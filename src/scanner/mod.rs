//! Scanner module - file discovery and scan orchestration
//!
//! This module provides:
//! - `Scanner` - finds markdown files respecting ignore patterns
//! - `ScanOptions` - configuration for scan behavior
//! - `full_scan` - performs a complete scan of a repository
//! - `scan_all_repositories` - scans all registered repositories

mod options;
pub(crate) mod orchestration;
mod progress;

use glob::Pattern;
use std::path::{Path, PathBuf};

// Re-export public items
pub use options::ScanOptions;
pub use orchestration::facts::{run_fact_embedding_phase, FactEmbeddingInput, FactEmbeddingOutput};
pub use orchestration::{full_scan, scan_all_repositories, ScanContext};

/// Scanner for finding markdown files in a directory
pub struct Scanner {
    ignore_patterns: Vec<Pattern>,
}

impl Scanner {
    /// Create a new Scanner with the given glob ignore patterns.
    pub fn new(ignore_patterns: &[String]) -> Self {
        let patterns = ignore_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();
        Self {
            ignore_patterns: patterns,
        }
    }

    /// Find all `.md` files under root, respecting ignore patterns.
    pub fn find_markdown_files(&self, root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name();
                let name_bytes = name.as_encoded_bytes();
                if name_bytes.starts_with(b".") {
                    continue; // skip dot files/directories (hidden/tooling)
                }
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "md") {
                    continue;
                }
                let relative = path.strip_prefix(root).unwrap_or(&path);
                let rel_str = relative.to_string_lossy();
                if self.ignore_patterns.iter().any(|p| p.matches(&rel_str)) {
                    continue;
                }
                files.push(path);
            }
        }
        files.sort();
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_markdown_files() {
        let temp = TempDir::new().expect("TempDir should be created");
        fs::write(temp.path().join("doc.md"), "# Doc").expect("write doc.md should succeed");
        fs::write(temp.path().join("readme.txt"), "text").expect("write readme.txt should succeed");
        fs::create_dir(temp.path().join("sub")).expect("create sub dir should succeed");
        fs::write(temp.path().join("sub/nested.md"), "# Nested")
            .expect("write nested.md should succeed");

        let scanner = Scanner::new(&[]);
        let files = scanner.find_markdown_files(temp.path());
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_ignore_patterns() {
        let temp = TempDir::new().expect("TempDir should be created");
        fs::write(temp.path().join("doc.md"), "# Doc").expect("write doc.md should succeed");
        fs::write(temp.path().join("doc.md.swp"), "swap").expect("write swap file should succeed");

        let scanner = Scanner::new(&["*.swp".into()]);
        let files = scanner.find_markdown_files(temp.path());
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_skips_dot_directories() {
        let temp = TempDir::new().expect("TempDir should be created");
        fs::write(temp.path().join("doc.md"), "# Doc").unwrap();

        // Create dot directories with markdown files
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".git/config.md"), "# Git").unwrap();

        fs::create_dir(temp.path().join(".automate")).unwrap();
        fs::write(temp.path().join(".automate/task.md"), "# Task").unwrap();

        // Nested dot directory
        fs::create_dir_all(temp.path().join(".git/refs/heads")).unwrap();
        fs::write(temp.path().join(".git/refs/heads/main.md"), "# Main").unwrap();

        // Normal subdirectory should still work
        fs::create_dir(temp.path().join("notes")).unwrap();
        fs::write(temp.path().join("notes/note.md"), "# Note").unwrap();

        let scanner = Scanner::new(&[]);
        let files = scanner.find_markdown_files(temp.path());
        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            files.len(),
            2,
            "Expected only doc.md and note.md, got: {:?}",
            names
        );
        assert!(names.contains(&"doc.md"));
        assert!(names.contains(&"note.md"));
    }

    #[test]
    fn test_skips_dot_files() {
        let temp = TempDir::new().expect("TempDir should be created");
        fs::write(temp.path().join("doc.md"), "# Doc").unwrap();
        fs::write(temp.path().join(".hidden.md"), "# Hidden").unwrap();

        let scanner = Scanner::new(&[]);
        let files = scanner.find_markdown_files(temp.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("doc.md"));
    }
}
