//! Test fixture utilities for Phase 5 E2E tests.

#![allow(dead_code)] // Functions will be used in Phase 5 tests

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Path to the test fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Returns path to a specific fixture file or directory.
pub fn get_fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Copies the test-repo fixture to a temp directory.
/// Returns the temp dir (keep alive to preserve files).
pub fn copy_fixture_repo(name: &str) -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let src = get_fixture_path(name);

    if src.exists() {
        copy_dir_recursive(&src, temp.path()).expect("Failed to copy fixture");
    }

    temp
}

/// Creates an empty temp repository with .factbase directory.
pub fn create_temp_repo(name: &str) -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let factbase_dir = temp.path().join(".factbase");
    std::fs::create_dir_all(&factbase_dir).expect("Failed to create .factbase dir");

    // Create minimal perspective.yaml in repo root
    let perspective = format!("type: test\norganization: Test\nfocus: {}\n", name);
    std::fs::write(temp.path().join("perspective.yaml"), perspective)
        .expect("Failed to write perspective.yaml");

    temp
}

/// Creates a temp repo with specified markdown files.
/// `files` is a slice of (relative_path, content) tuples.
pub fn create_temp_repo_with_files(files: &[(&str, &str)]) -> TempDir {
    let temp = create_temp_repo("test");

    for (path, content) in files {
        let file_path = temp.path().join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent dir");
        }
        std::fs::write(&file_path, content).expect("Failed to write file");
    }

    temp
}

/// Recursively copies a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            // Skip .factbase directory (will be regenerated)
            if entry.file_name() == ".factbase" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_dir_exists() {
        // May not exist yet, but path should be valid
        let dir = fixtures_dir();
        assert!(dir.ends_with("tests/fixtures"));
    }

    #[test]
    fn test_create_temp_repo() {
        let temp = create_temp_repo("test-repo");
        assert!(temp.path().join(".factbase").exists());
        assert!(temp.path().join("perspective.yaml").exists());
    }

    #[test]
    fn test_create_temp_repo_with_files() {
        let files = &[
            ("people/alice.md", "# Alice\nA person."),
            ("projects/alpha.md", "# Alpha\nA project."),
        ];
        let temp = create_temp_repo_with_files(files);

        assert!(temp.path().join("people/alice.md").exists());
        assert!(temp.path().join("projects/alpha.md").exists());
    }
}
