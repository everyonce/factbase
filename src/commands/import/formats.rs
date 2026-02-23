//! Import format handlers for different file types.

use super::args::ImportArgs;
use super::validate::{extract_factbase_id, validate_import_document, ImportValidationError};
use factbase::{ProgressReporter, Repository};
use std::collections::HashSet;
use std::fs;

/// Sanitize a title for use as a filename.
/// Replaces `/`, `\`, and `:` with `_` to avoid path issues.
pub fn sanitize_filename(title: &str) -> String {
    title.replace(['/', '\\', ':'], "_")
}

/// Build final document content, injecting factbase ID header if needed.
/// Returns the content unchanged if it already has a factbase header,
/// or prepends the ID header if an ID is provided.
pub fn build_final_content(content: &str, id: &str) -> String {
    if content.contains("<!-- factbase:") {
        content.to_string()
    } else if !id.is_empty() {
        format!("<!-- factbase:{id} -->\n{content}")
    } else {
        content.to_string()
    }
}

/// Import from a compressed tar.zst archive.
#[cfg(feature = "compression")]
pub fn import_tar_zst(
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    use std::io::Read;

    let file = fs::File::open(&args.input)?;
    let decoder = zstd::Decoder::new(file)?;
    let mut archive = tar::Archive::new(decoder);

    let pattern = args
        .include
        .as_ref()
        .map(|p| glob::Pattern::new(p))
        .transpose()?;

    let mut imported = 0;
    let mut skipped = 0;
    // Pre-allocate for typical case of ~8 validation errors
    let mut validation_errors: Vec<ImportValidationError> = Vec::with_capacity(8);
    let mut seen_ids: HashSet<String> = HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();

        if path.to_string_lossy() == "_metadata.json" {
            continue;
        }

        if let Some(ref pat) = pattern {
            if !pat.matches_path(&path) {
                continue;
            }
        }

        let dest_path = repo.path.join(&path);
        let filename = path.display().to_string();
        progress.log(&format!("Importing {filename}"));

        if dest_path.exists() && !args.overwrite {
            println!("Skipped (exists): {filename}");
            skipped += 1;
            continue;
        }

        // Read content for validation
        if args.validate {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;

            // Check for duplicate IDs
            if let Some(id) = extract_factbase_id(&content) {
                if seen_ids.contains(&id) {
                    validation_errors.push(ImportValidationError {
                        filename: filename.clone(),
                        errors: vec![format!("Duplicate factbase ID '{}' in import set", id)],
                    });
                } else {
                    seen_ids.insert(id);
                }
            }

            if let Some(err) = validate_import_document(&content, &filename) {
                validation_errors.push(err);
                continue;
            }

            // Skip actual import in dry-run mode
            if args.dry_run {
                println!("Would import: {filename}");
                imported += 1;
                continue;
            }

            // Write content manually since we already read it
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest_path, content)?;
            println!("Imported: {filename}");
            imported += 1;
        } else {
            // No validation - use original unpack logic
            if args.dry_run {
                println!("Would import: {filename}");
                imported += 1;
                continue;
            }

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&dest_path)?;
            println!("Imported: {filename}");
            imported += 1;
        }
    }

    // Report validation errors
    if !validation_errors.is_empty() {
        println!("\nValidation errors:");
        for err in &validation_errors {
            println!("  {}:", err.filename);
            for e in &err.errors {
                println!("    - {e}");
            }
        }
        anyhow::bail!(
            "Validation failed: {} document(s) have errors",
            validation_errors.len()
        );
    }

    let action = if args.dry_run {
        "Would import"
    } else {
        "Imported"
    };
    println!("\n{action} {imported} files from archive, skipped {skipped} (run scan to index)");
    Ok(())
}

/// Import from a compressed JSON file.
#[cfg(feature = "compression")]
pub fn import_json_zst(
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let compressed = fs::read(&args.input)?;
    let decompressed = zstd::decode_all(compressed.as_slice())?;
    let json_str = String::from_utf8(decompressed)?;
    import_json_content(&json_str, args, repo, progress)
}

/// Import from an uncompressed JSON file.
pub fn import_json(
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let json_str = fs::read_to_string(&args.input)?;
    import_json_content(&json_str, args, repo, progress)
}

/// Import documents from JSON content.
pub fn import_json_content(
    json_str: &str,
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let docs: Vec<serde_json::Value> = serde_json::from_str(json_str)?;

    let pattern = args
        .include
        .as_ref()
        .map(|p| glob::Pattern::new(p))
        .transpose()?;

    let total = docs.len();
    let mut imported = 0;
    let mut skipped = 0;
    // Pre-allocate for typical case of ~8 validation errors
    let mut validation_errors: Vec<ImportValidationError> = Vec::with_capacity(8);
    let mut seen_ids: HashSet<String> = HashSet::new();

    for (i, doc) in docs.iter().enumerate() {
        let id = doc["id"].as_str().unwrap_or("");
        let title = doc["title"].as_str().unwrap_or("Untitled");
        progress.report(i + 1, total, title);
        let content = doc["content"].as_str().unwrap_or("");

        let filename = format!("{}.md", sanitize_filename(title));

        if let Some(ref pat) = pattern {
            if !pat.matches(&filename) {
                continue;
            }
        }

        let dest_path = repo.path.join(&filename);

        if dest_path.exists() && !args.overwrite {
            println!("Skipped (exists): {filename}");
            skipped += 1;
            continue;
        }

        let final_content = build_final_content(content, id);

        // Validate if requested
        if args.validate {
            // Check for duplicate IDs
            if let Some(doc_id) = extract_factbase_id(&final_content) {
                if seen_ids.contains(&doc_id) {
                    validation_errors.push(ImportValidationError {
                        filename: filename.clone(),
                        errors: vec![format!("Duplicate factbase ID '{}' in import set", doc_id)],
                    });
                } else {
                    seen_ids.insert(doc_id);
                }
            }

            if let Some(err) = validate_import_document(&final_content, &filename) {
                validation_errors.push(err);
                continue;
            }
        }

        // Skip actual import in dry-run mode
        if args.dry_run {
            println!("Would import: {filename}");
            imported += 1;
            continue;
        }

        fs::write(&dest_path, final_content)?;
        println!("Imported: {filename}");
        imported += 1;
    }

    // Report validation errors
    if !validation_errors.is_empty() {
        println!("\nValidation errors:");
        for err in &validation_errors {
            println!("  {}:", err.filename);
            for e in &err.errors {
                println!("    - {e}");
            }
        }
        anyhow::bail!(
            "Validation failed: {} document(s) have errors",
            validation_errors.len()
        );
    }

    let action = if args.dry_run {
        "Would import"
    } else {
        "Imported"
    };
    println!("\n{action} {imported} documents from JSON, skipped {skipped} (run scan to index)");
    Ok(())
}

/// Import from a compressed markdown file.
#[cfg(feature = "compression")]
pub fn import_md_zst(
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let compressed = fs::read(&args.input)?;
    let decompressed = zstd::decode_all(compressed.as_slice())?;
    let content = String::from_utf8(decompressed)?;

    let filename = args
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .map_or("imported", |s| s.strip_suffix(".md").unwrap_or(s));
    progress.report(1, 1, filename);
    let dest_path = repo.path.join(format!("{filename}.md"));

    if dest_path.exists() && !args.overwrite {
        println!("Skipped (exists): {filename}.md");
        return Ok(());
    }

    fs::write(&dest_path, content)?;
    println!("Imported: {filename}.md (run scan to index)");
    Ok(())
}

/// Import from a directory of markdown files.
pub fn import_directory(
    args: &ImportArgs,
    repo: &Repository,
    progress: &ProgressReporter,
) -> anyhow::Result<()> {
    let pattern = args
        .include
        .as_ref()
        .map(|p| glob::Pattern::new(p))
        .transpose()?;

    let mut imported = 0;
    let mut skipped = 0;
    // Pre-allocate for typical case of ~8 validation errors
    let mut validation_errors: Vec<ImportValidationError> = Vec::with_capacity(8);
    let mut seen_ids: HashSet<String> = HashSet::new();

    let mut md_files = Vec::new();
    let mut stack = vec![args.input.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md")
                && entry.file_name() != "_metadata.json"
            {
                md_files.push(path);
            }
        }
    }
    md_files.sort();

    let total = md_files.len();
    for (i, path) in md_files.iter().enumerate() {
        progress.report(i + 1, total, &path.display().to_string());
        let rel_path = path.strip_prefix(&args.input)?;

        if let Some(ref pat) = pattern {
            if !pat.matches_path(rel_path) {
                continue;
            }
        }

        let dest_path = repo.path.join(rel_path);
        let filename = rel_path.display().to_string();

        if dest_path.exists() && !args.overwrite {
            println!("Skipped (exists): {filename}");
            skipped += 1;
            continue;
        }

        // Read content for validation
        let content = fs::read_to_string(path)?;

        // Validate if requested
        if args.validate {
            // Check for duplicate IDs
            if let Some(id) = extract_factbase_id(&content) {
                if seen_ids.contains(&id) {
                    validation_errors.push(ImportValidationError {
                        filename: filename.clone(),
                        errors: vec![format!("Duplicate factbase ID '{}' in import set", id)],
                    });
                } else {
                    seen_ids.insert(id);
                }
            }

            if let Some(err) = validate_import_document(&content, &filename) {
                validation_errors.push(err);
                continue;
            }
        }

        // Skip actual import in dry-run mode
        if args.dry_run {
            println!("Would import: {filename}");
            imported += 1;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &dest_path)?;
        println!("Imported: {filename}");
        imported += 1;
    }

    // Report validation errors
    if !validation_errors.is_empty() {
        println!("\nValidation errors:");
        for err in &validation_errors {
            println!("  {}:", err.filename);
            for e in &err.errors {
                println!("    - {e}");
            }
        }
        anyhow::bail!(
            "Validation failed: {} document(s) have errors",
            validation_errors.len()
        );
    }

    let action = if args.dry_run {
        "Would import"
    } else {
        "Imported"
    };
    println!("\n{action} {imported} files, skipped {skipped} (run scan to index)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== sanitize_filename tests ====================

    #[test]
    fn test_sanitize_filename_no_special_chars() {
        assert_eq!(sanitize_filename("Simple Title"), "Simple Title");
    }

    #[test]
    fn test_sanitize_filename_with_slashes() {
        assert_eq!(sanitize_filename("path/to/file"), "path_to_file");
        assert_eq!(sanitize_filename("path\\to\\file"), "path_to_file");
    }

    #[test]
    fn test_sanitize_filename_with_colons() {
        assert_eq!(sanitize_filename("Title: Subtitle"), "Title_ Subtitle");
        assert_eq!(sanitize_filename("C:\\path"), "C__path");
    }

    #[test]
    fn test_sanitize_filename_mixed_special_chars() {
        assert_eq!(sanitize_filename("A/B\\C:D"), "A_B_C_D");
    }

    // ==================== build_final_content tests ====================

    #[test]
    fn test_build_final_content_already_has_header() {
        let content = "<!-- factbase:abc123 -->\n# Title\nContent";
        assert_eq!(build_final_content(content, "xyz789"), content);
    }

    #[test]
    fn test_build_final_content_injects_id() {
        let content = "# Title\nContent";
        let result = build_final_content(content, "abc123");
        assert_eq!(result, "<!-- factbase:abc123 -->\n# Title\nContent");
    }

    #[test]
    fn test_build_final_content_empty_id() {
        let content = "# Title\nContent";
        assert_eq!(build_final_content(content, ""), content);
    }

    #[test]
    fn test_build_final_content_preserves_existing_header_different_id() {
        // If content already has a factbase header, don't modify it even if ID differs
        let content = "<!-- factbase:original -->\n# Title";
        assert_eq!(build_final_content(content, "different"), content);
    }
}
