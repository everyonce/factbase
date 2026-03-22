//! Entity folder detection utilities.
//!
//! An entity folder is a directory that contains a file whose stem matches
//! the folder name (e.g., `characters/diluc/` contains `diluc.md`).
//! Files inside an entity folder (other than the entity doc itself) are
//! companion files — sub-documents intentionally typed by the entity name.

use std::path::Path;

/// Returns true if `dir_path` is an entity folder.
///
/// A folder is an entity folder if it contains a file whose stem matches
/// the folder name (e.g., `diluc/diluc.md` makes `diluc/` an entity folder).
/// Files inside an entity folder are companion files for that entity.
///
/// # Arguments
/// * `dir_path` - The directory to check (absolute or relative to `repo_root`)
/// * `repo_root` - Repository root, used to resolve relative paths
pub fn is_entity_folder(dir_path: &Path, repo_root: &Path) -> bool {
    let full_path = if dir_path.is_absolute() {
        dir_path.to_path_buf()
    } else {
        repo_root.join(dir_path)
    };

    let folder_name = match full_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return false,
    };

    full_path.join(format!("{folder_name}.md")).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_entity_folder_true() {
        let tmp = TempDir::new().unwrap();
        let diluc_dir = tmp.path().join("characters").join("diluc");
        std::fs::create_dir_all(&diluc_dir).unwrap();
        std::fs::write(diluc_dir.join("diluc.md"), "# Diluc").unwrap();

        assert!(is_entity_folder(&diluc_dir, tmp.path()));
    }

    #[test]
    fn test_is_entity_folder_false_no_matching_file() {
        // `characters/` contains a `diluc/` subdirectory but no `characters.md`
        let tmp = TempDir::new().unwrap();
        let chars_dir = tmp.path().join("characters");
        std::fs::create_dir_all(chars_dir.join("diluc")).unwrap();

        assert!(!is_entity_folder(&chars_dir, tmp.path()));
    }

    #[test]
    fn test_is_entity_folder_false_different_stem() {
        // Folder contains files but none match the folder name
        let tmp = TempDir::new().unwrap();
        let diluc_dir = tmp.path().join("diluc");
        std::fs::create_dir_all(&diluc_dir).unwrap();
        std::fs::write(diluc_dir.join("lore.md"), "# Lore").unwrap();

        assert!(!is_entity_folder(&diluc_dir, tmp.path()));
    }

    #[test]
    fn test_is_entity_folder_relative_path() {
        let tmp = TempDir::new().unwrap();
        let diluc_dir = tmp.path().join("diluc");
        std::fs::create_dir_all(&diluc_dir).unwrap();
        std::fs::write(diluc_dir.join("diluc.md"), "# Diluc").unwrap();

        assert!(is_entity_folder(Path::new("diluc"), tmp.path()));
    }
}
