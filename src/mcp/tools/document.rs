//! Document CRUD MCP tools: create_document, update_document, delete_document, bulk_create_documents

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

    // Build document content with header and title
    let doc_content = format!("<!-- factbase:{id} -->\n# {title}\n\n{content}");

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

    Ok(serde_json::json!({
        "id": id,
        "title": title,
        "file_path": file_path.to_string_lossy(),
        "message": "Document created. Run scan to index."
    }))
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

    let file_path = PathBuf::from(&doc.file_path);
    if !file_path.exists() {
        return Err(FactbaseError::not_found(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    let title = new_title.unwrap_or(&doc.title);
    let content = new_content.unwrap_or(&doc.content);

    // Strip existing factbase header and title from content to avoid duplication
    let body = strip_factbase_header(if new_content.is_some() {
        content
    } else {
        &doc.content
    });

    let doc_content = format!("<!-- factbase:{id} -->\n# {title}\n\n{body}");
    fs::write(&file_path, &doc_content)?;

    // Sync content to database so subsequent tools (answer_questions, apply_review_answers)
    // see the current content instead of stale pre-edit data.
    let new_hash = crate::processor::content_hash(&doc_content);
    db.update_document_content(&id, &doc_content, &new_hash)?;

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

    let file_path = PathBuf::from(&doc.file_path);
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
        let doc_content = format!(
            "<!-- factbase:{} -->\n# {}\n\n{}",
            id, validated.title, validated.content
        );
        let file_path: PathBuf = repo.path.join(validated.path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&file_path, &doc_content)?;

        progress.report(i + 1, total, validated.title);

        created.push(serde_json::json!({
            "id": id,
            "title": validated.title,
            "file_path": file_path.to_string_lossy()
        }));
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
}
