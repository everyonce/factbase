//! Filesystem helpers for organize operations.
//!
//! Wraps filesystem operations with descriptive error messages.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::FactbaseError;

/// Wrap an `io::Error` with a descriptive action and path context.
fn io_err(e: io::Error, action: &str, path: &Path) -> FactbaseError {
    FactbaseError::Io(io::Error::new(
        e.kind(),
        format!("Failed to {} {}: {}", action, path.display(), e),
    ))
}

/// Write content to a file with a descriptive error on failure.
/// Uses temp file + rename for atomic writes.
pub(crate) fn write_file(path: &Path, content: &str) -> Result<(), FactbaseError> {
    let temp_path = path.with_extension("md.tmp");
    fs::write(&temp_path, content).map_err(|e| io_err(e, "write", &temp_path))?;
    fs::rename(&temp_path, path).map_err(|e| io_err(e, "rename", path))?;
    Ok(())
}

/// Read a file to string with a descriptive error on failure.
pub(crate) fn read_file(path: &Path) -> Result<String, FactbaseError> {
    fs::read_to_string(path).map_err(|e| io_err(e, "read", path))
}

/// Remove a file with a descriptive error on failure.
pub(crate) fn remove_file(path: &Path) -> Result<(), FactbaseError> {
    fs::remove_file(path).map_err(|e| io_err(e, "remove", path))
}

/// Copy a file with a descriptive error on failure.
pub(crate) fn copy_file(from: &Path, to: &Path) -> Result<(), FactbaseError> {
    fs::copy(from, to).map_err(|e| {
        FactbaseError::Io(io::Error::new(
            e.kind(),
            format!(
                "Failed to copy {} to {}: {}",
                from.display(),
                to.display(),
                e
            ),
        ))
    })?;
    Ok(())
}

/// Create a directory and all parent directories with a descriptive error on failure.
pub(crate) fn create_dir(path: &Path) -> Result<(), FactbaseError> {
    fs::create_dir_all(path).map_err(|e| io_err(e, "create directory", path))
}

/// Remove a directory and all its contents with a descriptive error on failure.
pub(crate) fn remove_dir(path: &Path) -> Result<(), FactbaseError> {
    fs::remove_dir_all(path).map_err(|e| io_err(e, "remove directory", path))
}

/// Canonicalize a path, stripping the Windows `\\?\` prefix if present.
pub fn clean_canonicalize(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    #[cfg(target_os = "windows")]
    {
        let s = canonical.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    canonical
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_and_read_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.md");
        write_file(&path, "hello world").unwrap();
        let content = read_file(&path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_write_file_atomic_no_temp_left() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.md");
        write_file(&path, "content").unwrap();
        let temp_path = path.with_extension("md.tmp");
        assert!(!temp_path.exists(), "temp file should be cleaned up");
    }

    #[test]
    fn test_read_file_not_found() {
        let result = read_file(Path::new("/nonexistent/path.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_file_success() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("to_remove.md");
        fs::write(&path, "content").unwrap();
        remove_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_remove_file_not_found() {
        let result = remove_file(Path::new("/nonexistent/file.md"));
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_file_success() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src.md");
        let dst = tmp.path().join("dst.md");
        fs::write(&src, "original").unwrap();
        copy_file(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(&dst).unwrap(), "original");
    }

    #[test]
    fn test_create_dir_nested() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("a").join("b").join("c");
        create_dir(&path).unwrap();
        assert!(path.is_dir());
    }

    #[test]
    fn test_remove_dir_success() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("to_remove");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("file.txt"), "content").unwrap();
        remove_dir(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_clean_canonicalize_existing_path() {
        let tmp = TempDir::new().unwrap();
        let result = clean_canonicalize(tmp.path());
        assert!(result.is_absolute());
    }

    #[test]
    fn test_clean_canonicalize_nonexistent_path() {
        let result = clean_canonicalize(Path::new("/nonexistent/path"));
        assert_eq!(result, PathBuf::from("/nonexistent/path"));
    }
}
