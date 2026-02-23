//! Document CRUD MCP tools: create_document, update_document, delete_document, bulk_create_documents

use super::helpers::resolve_doc_path;
use super::{get_str_arg, get_str_arg_required};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::patterns::ID_REGEX;
use crate::processor::DocumentProcessor;
use crate::ProgressReporter;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tracing::instrument;

const MAX_TITLE_LENGTH: usize = 200;
const MAX_CONTENT_SIZE: usize = 1_048_576; // 1MB

/// Analyze content for temporal tag and source footnote coverage on fact lines.
/// Returns a warning string if coverage is below 50%, or None if adequate.
fn content_coverage_warning(content: &str) -> Option<String> {
    let mut facts = 0u32;
    let mut temporal = 0u32;
    let mut sourced = 0u32;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('-') {
            facts += 1;
            if trimmed.contains("@t[") {
                temporal += 1;
            }
            if trimmed.contains("[^") {
                sourced += 1;
            }
        }
    }
    if facts == 0 {
        return None;
    }
    let temporal_pct = temporal as f64 / facts as f64;
    let source_pct = sourced as f64 / facts as f64;
    if temporal_pct < 0.5 || source_pct < 0.5 {
        Some(format!(
            "⚠️ {temporal}/{facts} facts have temporal tags, {sourced}/{facts} have sources. \
             Call get_authoring_guide for format requirements."
        ))
    } else {
        None
    }
}

/// Extract the `# Title` from content, skipping any factbase header first.
/// Returns `None` if no title line is found.
fn extract_title_from_content(content: &str) -> Option<String> {
    let mut lines = content.lines().peekable();

    // Skip factbase header if present
    if let Some(first) = lines.peek() {
        if ID_REGEX.is_match(first) {
            lines.next();
        }
    }

    // Skip blank lines between header and title
    while let Some(line) = lines.peek() {
        if !line.trim().is_empty() {
            break;
        }
        lines.next();
    }

    // Extract title if present
    if let Some(line) = lines.peek() {
        if line.starts_with("# ") {
            return Some(crate::patterns::clean_title(&line[2..]));
        }
    }

    None
}

/// Strip the `<!-- factbase:ID -->` header and first `# Title` line from content,
/// returning only the body. Handles content with or without the header.
fn strip_factbase_header(content: &str) -> String {
    let mut lines = content.lines().peekable();

    // Skip factbase header if present
    if let Some(first) = lines.peek() {
        if ID_REGEX.is_match(first) {
            lines.next();
        }
    }

    // Skip blank lines between header and title
    while let Some(line) = lines.peek() {
        if !line.trim().is_empty() {
            break;
        }
        lines.next();
    }

    // Skip title line if present
    if let Some(line) = lines.peek() {
        if line.starts_with("# ") {
            lines.next();
        }
    }

    // Skip blank lines after title
    while let Some(line) = lines.peek() {
        if !line.trim().is_empty() {
            break;
        }
        lines.next();
    }

    lines.collect::<Vec<_>>().join("\n")
}

fn validate_title(title: &str) -> Result<(), FactbaseError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(FactbaseError::parse("Title cannot be empty"));
    }
    if trimmed.len() > MAX_TITLE_LENGTH {
        return Err(FactbaseError::parse(format!(
            "Title exceeds {MAX_TITLE_LENGTH} characters"
        )));
    }
    Ok(())
}

fn validate_content(content: &str) -> Result<(), FactbaseError> {
    if content.len() > MAX_CONTENT_SIZE {
        return Err(FactbaseError::parse(format!(
            "Content exceeds {MAX_CONTENT_SIZE} bytes"
        )));
    }
    Ok(())
}

/// Strip a leading `# Title` from content if the agent included it redundantly.
/// create_document already prepends `# {title}`, so if content starts with the
/// same heading, strip it to avoid duplication.
fn strip_leading_title<'a>(content: &'a str, title: &str) -> &'a str {
    let trimmed = content.trim_start();
    // Check for `# Title` possibly followed by newlines
    if let Some(rest) = trimmed.strip_prefix('#') {
        let rest = rest.trim_start_matches('#'); // handle ## or ### too
        let rest = rest.trim_start();
        // Compare case-insensitively and strip if it matches the title
        let first_line_end = rest.find('\n').unwrap_or(rest.len());
        let first_line = rest[..first_line_end].trim();
        if first_line.eq_ignore_ascii_case(title.trim()) {
            let after_title = &rest[first_line_end..];
            // Skip leading blank lines after the stripped title
            return after_title.trim_start_matches('\n').trim_start_matches('\r');
        }
    }
    content
}

/// Creates a new document in a repository.
///
/// Writes a markdown file with factbase header and title to the specified path.
/// The file must not already exist.
///
/// # Arguments (from JSON)
/// - `repo` (required): Repository ID
/// - `path` (required): Relative path within repository (e.g., "people/john.md")
/// - `title` (required): Document title (max 200 chars)
/// - `content` (optional): Document body content (max 1MB)
///
/// # Returns
/// JSON with `id`, `title`, `file_path`, and `message` fields.
///
/// # Errors
/// - `FactbaseError::NotFound` if repository doesn't exist
/// - `FactbaseError::Parse` if file already exists or validation fails
#[instrument(name = "mcp_create_document", skip(db, args))]
pub fn create_document(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg_required(args, "repo")?;
    let path = get_str_arg_required(args, "path")?;
    let title = get_str_arg_required(args, "title")?;
    let content = get_str_arg(args, "content").unwrap_or("");

    validate_title(&title)?;
    validate_content(content)?;

    let repo = db.require_repository(&repo_id)?;

    let processor = DocumentProcessor::new();
    let id = processor.generate_unique_id(db);

    // Strip duplicate title from content if the agent included it
    let content_trimmed = strip_leading_title(content, &title);

    // Build document content with header and title
    let doc_content = format!("<!-- factbase:{id} -->\n# {title}\n\n{content_trimmed}");

    // Construct full file path
    let file_path: PathBuf = repo.path.join(&path);

    // Ensure parent directory exists
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check if file already exists
    if file_path.exists() {
        return Err(FactbaseError::parse(format!(
            "File already exists: {}",
            file_path.display()
        )));
    }

    // Write the file
    fs::write(&file_path, &doc_content)?;

    let mut response = serde_json::json!({
        "id": id,
        "title": title,
        "file_path": file_path.to_string_lossy(),
        "message": "Document created. Run scan to index."
    });
    if let Some(warning) = content_coverage_warning(content) {
        response["warning"] = Value::String(warning);
    }

    Ok(response)
}

/// Updates an existing document's title and/or content.
///
/// Reads the document from database, modifies the file on disk with new values.
/// At least one of `title` or `content` must be provided.
///
/// # Arguments (from JSON)
/// - `id` (required): Document ID (6-char hex)
/// - `title` (optional): New title (max 200 chars)
/// - `content` (optional): New body content (max 1MB)
///
/// # Returns
/// JSON with `id`, `title`, `file_path`, and `message` fields.
///
/// # Errors
/// - `FactbaseError::NotFound` if document or file doesn't exist
/// - `FactbaseError::Parse` if neither title nor content provided
#[instrument(name = "mcp_update_document", skip(db, args))]
pub fn update_document(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let id = get_str_arg_required(args, "id")?;
    let new_title = get_str_arg(args, "title");
    let new_content = get_str_arg(args, "content");

    if new_title.is_none() && new_content.is_none() {
        return Err(FactbaseError::parse(
            "At least one of title or content must be provided",
        ));
    }

    if let Some(t) = new_title {
        validate_title(t)?;
    }
    if let Some(c) = new_content {
        validate_content(c)?;
    }

    let doc = db.require_document(&id)?;

    let file_path = resolve_doc_path(db, &doc)?;
    if !file_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    // When content includes a # Title line and no explicit title param was given,
    // extract the title from the content so it isn't silently reverted to the stale DB value.
    let extracted_title = if new_title.is_none() && new_content.is_some() {
        extract_title_from_content(new_content.unwrap())
    } else {
        None
    };
    let title = new_title
        .map(|t| t.to_string())
        .or(extracted_title)
        .unwrap_or_else(|| doc.title.clone());

    let content = new_content.unwrap_or(&doc.content);

    // Strip existing factbase header and title from content to avoid duplication
    let body = strip_factbase_header(if new_content.is_some() {
        content
    } else {
        &doc.content
    });

    let doc_content = format!("<!-- factbase:{id} -->\n# {title}\n\n{body}");
    fs::write(&file_path, &doc_content)?;

    // Sync content and title to database so subsequent tools (answer_questions,
    // apply_review_answers, get_entity) see the current data instead of stale pre-edit values.
    let new_hash = crate::processor::content_hash(&doc_content);
    db.update_document_content(&id, &doc_content, &new_hash)?;
    db.update_document_title(&id, &title)?;

    Ok(serde_json::json!({
        "id": id,
        "title": title,
        "file_path": file_path.to_string_lossy(),
        "message": "Document updated. Run scan to re-index."
    }))
}

/// Deletes a document by ID.
///
/// Removes the file from disk and marks the document as deleted in the database.
///
/// # Arguments (from JSON)
/// - `id` (required): Document ID (6-char hex)
///
/// # Returns
/// JSON with `id`, `title`, and `message` fields.
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist
#[instrument(name = "mcp_delete_document", skip(db, args))]
pub fn delete_document(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let id = get_str_arg_required(args, "id")?;

    let doc = db.require_document(&id)?;

    let file_path = resolve_doc_path(db, &doc)?;
    if file_path.exists() {
        fs::remove_file(&file_path)?;
    }

    db.mark_deleted(&id)?;

    Ok(serde_json::json!({
        "id": id,
        "title": doc.title,
        "message": "Document deleted."
    }))
}

/// Creates multiple documents atomically.
///
/// Validates all documents first, then creates them. If any validation fails,
/// no documents are created (all-or-nothing semantics).
///
/// # Arguments (from JSON)
/// - `repo` (required): Repository ID
/// - `documents` (required): Array of objects with `path`, `title`, and optional `content`
///
/// # Limits
/// - Maximum 100 documents per call
/// - Each title max 200 chars, content max 1MB
///
/// # Returns
/// JSON with `success`, `created` array, `errors` array, and `message`.
///
/// # Errors
/// - `FactbaseError::NotFound` if repository doesn't exist
/// - `FactbaseError::Parse` if documents array empty or exceeds limit
#[instrument(name = "mcp_bulk_create_documents", skip(db, args, progress))]
pub fn bulk_create_documents(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg_required(args, "repo")?;
    let documents = args
        .get("documents")
        .and_then(|v| v.as_array())
        .ok_or_else(|| FactbaseError::parse("documents array is required"))?;

    if documents.is_empty() {
        return Err(FactbaseError::parse("documents array cannot be empty"));
    }

    if documents.len() > 100 {
        return Err(FactbaseError::parse(
            "Maximum 100 documents per bulk operation",
        ));
    }

    let repo = db.require_repository(&repo_id)?;

    let processor = DocumentProcessor::new();
    let mut errors: Vec<Value> = Vec::with_capacity(documents.len() / 4); // Expect few errors

    // Validated document data
    struct ValidatedDoc<'a> {
        path: &'a str,
        title: &'a str,
        content: &'a str,
    }
    let mut validated_docs: Vec<ValidatedDoc> = Vec::with_capacity(documents.len());

    // Validate all documents first
    for (i, doc) in documents.iter().enumerate() {
        let path_opt = doc.get("path").and_then(|v| v.as_str());
        let title_opt = doc.get("title").and_then(|v| v.as_str());
        let content = doc.get("content").and_then(|v| v.as_str()).unwrap_or("");

        // Check required fields
        let (path, title) = match (path_opt, title_opt) {
            (None, _) => {
                errors.push(serde_json::json!({
                    "index": i,
                    "error": "path is required"
                }));
                continue;
            }
            (_, None) => {
                errors.push(serde_json::json!({
                    "index": i,
                    "error": "title is required"
                }));
                continue;
            }
            (Some(p), Some(t)) => (p, t),
        };

        if let Err(e) = validate_title(title) {
            errors.push(serde_json::json!({
                "index": i,
                "error": e.to_string()
            }));
            continue;
        }
        if let Err(e) = validate_content(content) {
            errors.push(serde_json::json!({
                "index": i,
                "error": e.to_string()
            }));
            continue;
        }

        let file_path: PathBuf = repo.path.join(path);
        if file_path.exists() {
            errors.push(serde_json::json!({
                "index": i,
                "error": format!("File already exists: {}", file_path.display())
            }));
            continue;
        }

        validated_docs.push(ValidatedDoc {
            path,
            title,
            content,
        });
    }

    // If any validation errors, return early (atomic: all or nothing)
    if !errors.is_empty() {
        return Ok(serde_json::json!({
            "success": false,
            "created": [],
            "errors": errors,
            "message": "Validation failed. No documents created."
        }));
    }

    // Create all documents using validated data
    let mut created: Vec<Value> = Vec::with_capacity(validated_docs.len());
    let total = validated_docs.len();
    for (i, validated) in validated_docs.iter().enumerate() {
        let id = processor.generate_unique_id(db);
        let content_trimmed = strip_leading_title(validated.content, validated.title);
        let doc_content = format!(
            "<!-- factbase:{} -->\n# {}\n\n{}",
            id, validated.title, content_trimmed
        );
        let file_path: PathBuf = repo.path.join(validated.path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&file_path, &doc_content)?;

        progress.report(i + 1, total, validated.title);

        let mut entry = serde_json::json!({
            "id": id,
            "title": validated.title,
            "file_path": file_path.to_string_lossy()
        });
        if let Some(warning) = content_coverage_warning(validated.content) {
            entry["warning"] = Value::String(warning);
        }
        created.push(entry);
    }

    Ok(serde_json::json!({
        "success": true,
        "created": created,
        "errors": [],
        "message": format!("{} documents created. Run scan to index.", created.len())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_warning_no_facts() {
        assert!(content_coverage_warning("Just a paragraph").is_none());
    }

    #[test]
    fn test_coverage_warning_all_covered() {
        let content = "- fact @t[2024] [^1]\n- another @t[2023] [^2]";
        assert!(content_coverage_warning(content).is_none());
    }

    #[test]
    fn test_coverage_warning_no_tags() {
        let content = "- fact one\n- fact two\n- fact three";
        let w = content_coverage_warning(content).unwrap();
        assert!(w.contains("0/3 facts have temporal tags"));
        assert!(w.contains("0/3 have sources"));
        assert!(w.contains("get_authoring_guide"));
    }

    #[test]
    fn test_coverage_warning_partial_below_threshold() {
        // 1/3 = 33% temporal, 0/3 sources → both below 50%
        let content = "- fact @t[2024]\n- bare fact\n- another bare";
        assert!(content_coverage_warning(content).is_some());
    }

    #[test]
    fn test_coverage_warning_exactly_half() {
        // 1/2 = 50% temporal, 1/2 = 50% source → not below 50%, no warning
        let content = "- fact @t[2024] [^1]\n- bare fact";
        assert!(content_coverage_warning(content).is_none());
    }

    #[test]
    fn test_coverage_warning_indented_facts() {
        let content = "  - indented fact\n    - nested fact";
        let w = content_coverage_warning(content).unwrap();
        assert!(w.contains("0/2 facts have temporal tags"));
    }

    #[test]
    fn test_validate_title_empty() {
        assert!(validate_title("").is_err());
        assert!(validate_title("   ").is_err());
    }

    #[test]
    fn test_validate_title_too_long() {
        let long_title = "a".repeat(201);
        assert!(validate_title(&long_title).is_err());
    }

    #[test]
    fn test_validate_title_valid() {
        assert!(validate_title("Valid Title").is_ok());
        assert!(validate_title("a".repeat(200).as_str()).is_ok());
    }

    #[test]
    fn test_validate_content_too_large() {
        let large_content = "a".repeat(MAX_CONTENT_SIZE + 1);
        assert!(validate_content(&large_content).is_err());
    }

    #[test]
    fn test_validate_content_valid() {
        assert!(validate_content("").is_ok());
        assert!(validate_content("Normal content").is_ok());
    }

    #[test]
    fn test_strip_factbase_header_with_header_and_title() {
        let content = "<!-- factbase:a1cb2b -->\n# Stacey Lee\n\n- fact 1\n- fact 2";
        assert_eq!(strip_factbase_header(content), "- fact 1\n- fact 2");
    }

    #[test]
    fn test_strip_factbase_header_without_header() {
        let content = "- fact 1\n- fact 2";
        assert_eq!(strip_factbase_header(content), "- fact 1\n- fact 2");
    }

    #[test]
    fn test_strip_factbase_header_title_only() {
        let content = "# Stacey Lee\n\n- fact 1";
        assert_eq!(strip_factbase_header(content), "- fact 1");
    }

    #[test]
    fn test_strip_factbase_header_preserves_html_comments() {
        let content = "<!-- factbase:a1cb2b -->\n# Title\n\n<!-- important note -->\n- fact 1";
        assert_eq!(
            strip_factbase_header(content),
            "<!-- important note -->\n- fact 1"
        );
    }

    #[test]
    fn test_strip_factbase_header_preserves_later_h1() {
        let content = "<!-- factbase:a1cb2b -->\n# Title\n\n## Section\n# Another H1";
        assert_eq!(
            strip_factbase_header(content),
            "## Section\n# Another H1"
        );
    }

    #[test]
    fn test_strip_factbase_header_non_factbase_comment() {
        let content = "<!-- not a factbase header -->\n# Title\n\n- fact 1";
        assert_eq!(
            strip_factbase_header(content),
            "<!-- not a factbase header -->\n# Title\n\n- fact 1"
        );
    }

    #[test]
    fn test_extract_title_from_content_with_header() {
        let content = "<!-- factbase:a1cb2b -->\n# My Title\n\n- fact";
        assert_eq!(
            extract_title_from_content(content),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn test_extract_title_from_content_without_header() {
        let content = "# My Title\n\n- fact";
        assert_eq!(
            extract_title_from_content(content),
            Some("My Title".to_string())
        );
    }

    #[test]
    fn test_extract_title_from_content_no_title() {
        let content = "- fact 1\n- fact 2";
        assert_eq!(extract_title_from_content(content), None);
    }

    #[test]
    fn test_extract_title_from_content_strips_footnote_refs() {
        let content = "<!-- factbase:a1cb2b -->\n# Joan Butters [^7]\n\n- fact";
        assert_eq!(
            extract_title_from_content(content),
            Some("Joan Butters".to_string())
        );
    }

    #[test]
    fn test_update_document_content_writes_to_disk() {
        use crate::database::tests::test_db;
        use crate::models::{Document, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file = repo_dir.path().join("test.md");
        fs::write(&file, "<!-- factbase:abc123 -->\n# Old Title\n\nOld body").unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Old Title".into(),
            content: "<!-- factbase:abc123 -->\n# Old Title\n\nOld body".into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({"id": "abc123", "content": "New body here"});
        let result = update_document(&db, &args).unwrap();
        assert_eq!(result["title"], "Old Title");

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("New body here"), "file should have new body: {on_disk}");
        assert!(on_disk.contains("# Old Title"), "title preserved when not in content");
    }

    #[test]
    fn test_update_document_extracts_title_from_content() {
        use crate::database::tests::test_db;
        use crate::models::{Document, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file = repo_dir.path().join("test.md");
        fs::write(&file, "<!-- factbase:abc123 -->\n# Old Title\n\nOld body").unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Old Title".into(),
            content: "<!-- factbase:abc123 -->\n# Old Title\n\nOld body".into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Pass content with a new title embedded — no separate title param
        let args = serde_json::json!({
            "id": "abc123",
            "content": "<!-- factbase:abc123 -->\n# Fixed Title\n\nCleaned body"
        });
        let result = update_document(&db, &args).unwrap();
        assert_eq!(result["title"], "Fixed Title");

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("# Fixed Title"), "title should be extracted from content: {on_disk}");
        assert!(on_disk.contains("Cleaned body"), "body should be updated: {on_disk}");
        assert!(!on_disk.contains("Old body"), "old body should be gone: {on_disk}");
    }

    #[test]
    fn test_update_document_explicit_title_overrides_content_title() {
        use crate::database::tests::test_db;
        use crate::models::{Document, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file = repo_dir.path().join("test.md");
        fs::write(&file, "<!-- factbase:abc123 -->\n# Old\n\nBody").unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Old".into(),
            content: "<!-- factbase:abc123 -->\n# Old\n\nBody".into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Explicit title param should win over title in content
        let args = serde_json::json!({
            "id": "abc123",
            "title": "Explicit Title",
            "content": "# Content Title\n\nNew body"
        });
        let result = update_document(&db, &args).unwrap();
        assert_eq!(result["title"], "Explicit Title");

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("# Explicit Title"), "explicit title wins: {on_disk}");
    }

    #[test]
    fn test_update_document_syncs_to_database() {
        use crate::database::tests::test_db;
        use crate::models::{Document, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let file = repo_dir.path().join("test.md");
        fs::write(&file, "<!-- factbase:abc123 -->\n# Title\n\nOld").unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Title".into(),
            content: "<!-- factbase:abc123 -->\n# Title\n\nOld".into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({"id": "abc123", "content": "New content"});
        update_document(&db, &args).unwrap();

        let updated = db.get_document("abc123").unwrap().unwrap();
        assert!(updated.content.contains("New content"), "DB should have new content");
    }

    #[test]
    fn test_create_document_warns_on_missing_tags() {
        use crate::database::tests::test_db;
        use crate::models::Repository;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({
            "repo": "r1",
            "path": "test.md",
            "title": "Test",
            "content": "- bare fact\n- another bare fact"
        });
        let result = create_document(&db, &args).unwrap();
        assert!(result.get("warning").is_some());
        assert!(result["warning"].as_str().unwrap().contains("0/2 facts have temporal tags"));
    }

    #[test]
    fn test_create_document_no_warning_when_covered() {
        use crate::database::tests::test_db;
        use crate::models::Repository;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({
            "repo": "r1",
            "path": "test.md",
            "title": "Test",
            "content": "- fact @t[2024] [^1]\n- another @t[2023] [^2]"
        });
        let result = create_document(&db, &args).unwrap();
        assert!(result.get("warning").is_none());
    }

    #[test]
    fn test_bulk_create_documents_warns_per_document() {
        use crate::database::tests::test_db;
        use crate::models::Repository;
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({
            "repo": "r1",
            "documents": [
                {"path": "a.md", "title": "A", "content": "- bare fact"},
                {"path": "b.md", "title": "B", "content": "- fact @t[2024] [^1]"}
            ]
        });
        let result = bulk_create_documents(&db, &args, &ProgressReporter::Silent).unwrap();
        let created = result["created"].as_array().unwrap();
        assert!(created[0].get("warning").is_some());
        assert!(created[1].get("warning").is_none());
    }

    #[test]
    fn test_strip_leading_title_exact_match() {
        let content = "# Amanita muscaria\n\n## Classification\n- Kingdom: Fungi";
        let result = strip_leading_title(content, "Amanita muscaria");
        assert_eq!(result, "## Classification\n- Kingdom: Fungi");
    }

    #[test]
    fn test_strip_leading_title_case_insensitive() {
        let content = "# amanita muscaria\n\nSome content";
        let result = strip_leading_title(content, "Amanita muscaria");
        assert_eq!(result, "Some content");
    }

    #[test]
    fn test_strip_leading_title_no_match() {
        let content = "# Different Title\n\nSome content";
        let result = strip_leading_title(content, "Amanita muscaria");
        assert_eq!(result, content);
    }

    #[test]
    fn test_strip_leading_title_no_heading() {
        let content = "Just some content without a heading";
        let result = strip_leading_title(content, "Amanita muscaria");
        assert_eq!(result, content);
    }

    #[test]
    fn test_strip_leading_title_with_leading_whitespace() {
        let content = "\n\n# Amanita muscaria\n\n## Habitat";
        let result = strip_leading_title(content, "Amanita muscaria");
        assert_eq!(result, "## Habitat");
    }

    #[test]
    fn test_strip_leading_title_empty_content() {
        let result = strip_leading_title("", "Title");
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_leading_title_with_factbase_header() {
        // If content includes the factbase header + title, it should NOT strip
        // because the first # line would be the factbase comment, not a heading
        let content = "<!-- factbase:abc123 -->\n# Amanita muscaria\n\nContent";
        let result = strip_leading_title(content, "Amanita muscaria");
        // Should not strip — the first non-whitespace is a comment, not a heading
        assert_eq!(result, content);
    }
}
