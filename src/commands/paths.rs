//! Path validation utilities for CLI commands

use std::path::Path;

/// Validate that a path exists and is a directory
pub fn validate_directory_path(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path.display());
    }
    Ok(())
}

/// Validate that a path exists (file or directory)
pub fn validate_file_path(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("Path does not exist: {}", path.display());
    }
    Ok(())
}

/// Case-insensitive file extension check.
pub fn ends_with_ext(path: &str, ext: &str) -> bool {
    path.len() >= ext.len() && path[path.len() - ext.len()..].eq_ignore_ascii_case(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_directory_path_nonexistent() {
        let path = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let result = validate_directory_path(path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Path does not exist"));
    }

    #[test]
    fn test_validate_directory_path_is_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = validate_directory_path(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Path is not a directory"));
    }

    #[test]
    fn test_validate_directory_path_valid() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = validate_directory_path(path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_path_nonexistent() {
        let path = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let result = validate_file_path(path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Path does not exist"));
    }

    #[test]
    fn test_validate_file_path_valid_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = validate_file_path(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_path_valid_directory() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = validate_file_path(path);
        assert!(result.is_ok());
    }
}
