use glob::Pattern;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct Scanner {
    ignore_patterns: Vec<Pattern>,
}

impl Scanner {
    pub fn new(ignore_patterns: &[String]) -> Self {
        let patterns = ignore_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();
        Self {
            ignore_patterns: patterns,
        }
    }

    pub fn find_markdown_files(&self, root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().map(|e| e != "md").unwrap_or(true) {
                continue;
            }

            let relative = path.strip_prefix(root).unwrap_or(path);
            let rel_str = relative.to_string_lossy();
            if self.ignore_patterns.iter().any(|p| p.matches(&rel_str)) {
                continue;
            }

            files.push(path.to_path_buf());
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
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("doc.md"), "# Doc").unwrap();
        fs::write(temp.path().join("readme.txt"), "text").unwrap();
        fs::create_dir(temp.path().join("sub")).unwrap();
        fs::write(temp.path().join("sub/nested.md"), "# Nested").unwrap();

        let scanner = Scanner::new(&[]);
        let files = scanner.find_markdown_files(temp.path());
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_ignore_patterns() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("doc.md"), "# Doc").unwrap();
        fs::write(temp.path().join("doc.md.swp"), "swap").unwrap();

        let scanner = Scanner::new(&["*.swp".into()]);
        let files = scanner.find_markdown_files(temp.path());
        assert_eq!(files.len(), 1);
    }
}
