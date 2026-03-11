//! Error helper functions for CLI commands

use std::path::Path;

/// Create a "Database not found" error with helpful suggestion
pub fn db_not_found_error(path: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "Database not found at {}\nRun 'factbase init' to create database",
        path.display()
    )
}

/// Create a "Repository path not found" error with helpful suggestion
pub fn repo_path_not_found_error() -> anyhow::Error {
    anyhow::anyhow!(
        "Repository path not found\nCheck that the repository path exists and is accessible"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_not_found_error_contains_path() {
        let path = std::path::Path::new("/some/path/factbase.db");
        let err = db_not_found_error(path);
        let msg = err.to_string();
        assert!(msg.contains("/some/path/factbase.db"));
        assert!(msg.contains("Database not found"));
    }

    #[test]
    fn test_db_not_found_error_contains_suggestion() {
        let path = std::path::Path::new("/test/db.sqlite");
        let err = db_not_found_error(path);
        let msg = err.to_string();
        assert!(msg.contains("factbase init"));
    }

    #[test]
    fn test_repo_path_not_found_error_message() {
        let err = repo_path_not_found_error();
        let msg = err.to_string();
        assert!(msg.contains("Repository path not found"));
    }

    #[test]
    fn test_repo_path_not_found_error_contains_suggestion() {
        let err = repo_path_not_found_error();
        let msg = err.to_string();
        assert!(msg.contains("Check that the repository path exists"));
    }
}
