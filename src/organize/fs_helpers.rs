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
