//! Document CRUD MCP tools: create_document, update_document, delete_document, bulk_create_documents

use super::helpers::{load_glossary_terms, resolve_doc_path};
use super::{get_str_arg, get_str_arg_required, resolve_repo};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::patterns::{body_end_offset, ID_REGEX};
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

/// Get the resolved format config for a repository.
fn resolve_repo_format(repo: &crate::models::Repository) -> crate::models::ResolvedFormat {
    repo.perspective
        .as_ref()
        .and_then(|p| p.format.as_ref())
        .map(|f| f.resolve())
        .unwrap_or_default()
}

/// Merge new frontmatter fields into an existing set, overriding by key.
fn merge_frontmatter_fields(base: &mut Vec<String>, overrides: &[String]) {
    for new_line in overrides {
        let new_key = new_line.split(':').next().unwrap_or("").trim();
        if let Some(pos) = base.iter().position(|l| l.split(':').next().unwrap_or("").trim() == new_key) {
            base[pos] = new_line.clone();
        } else {
            base.push(new_line.clone());
        }
    }
}

/// Strip the `<!-- factbase:ID -->` header, YAML frontmatter, and first `# Title` line
/// from content, returning only the body.
pub(crate) fn strip_factbase_header(content: &str) -> String {
    let mut lines = content.lines().peekable();

    // Skip factbase HTML comment header if present
    if let Some(first) = lines.peek() {
        if ID_REGEX.is_match(first) {
            lines.next();
        }
    }

    // Skip YAML frontmatter block if present
    if let Some(first) = lines.peek() {
        if first.trim() == "---" {
            lines.next(); // skip opening ---
            for line in lines.by_ref() {
                if line.trim() == "---" {
                    break;
                }
            }
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
    let repo = resolve_repo(db, get_str_arg(args, "repo"))?;
    let repo_id = &repo.id;
    let path = get_str_arg_required(args, "path")?;
    let title = get_str_arg_required(args, "title")?;
    let content = get_str_arg(args, "content").unwrap_or("");

    validate_title(&title)?;
    validate_content(content)?;

    let processor = DocumentProcessor::new();
    let id = processor.generate_unique_id(db);

    // Strip duplicate title from content if the agent included it
    let content_trimmed = strip_leading_title(content, &title);

    // Deduplicate inline acronym expansions
    let glossary = load_glossary_terms(db, Some(repo_id));
    let content_deduped = crate::processor::dedup_acronym_expansions(content_trimmed, &glossary);

    // Build document content with header and title using format config
    let resolved_format = resolve_repo_format(&repo);
    let doc_type = crate::processor::DocumentProcessor::new()
        .derive_type(&repo.path.join(&path), &repo.path);
    let header = crate::processor::build_document_header(
        &id,
        &title,
        Some(&doc_type),
        &resolved_format,
        &[],
    );
    let doc_content = format!("{header}{content_deduped}");

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
        "doc_type": doc_type,
        "file_path": file_path.to_string_lossy(),
        "message": format!("Document '{}' ({}) created at {}. Run scan to index.", title, id, path)
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
    let suggested_move = get_str_arg(args, "suggested_move");
    let suggested_rename = get_str_arg(args, "suggested_rename");
    let suggested_title = get_str_arg(args, "suggested_title");

    let has_suggestions = suggested_move.is_some() || suggested_rename.is_some() || suggested_title.is_some();
    let has_content_update = new_title.is_some() || new_content.is_some();

    if !has_content_update && !has_suggestions {
        return Err(FactbaseError::parse(
            "At least one of title, content, suggested_move, suggested_rename, or suggested_title must be provided",
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

    let title = if has_content_update {
        if !file_path.exists() {
            return Err(FactbaseError::not_found(format!(
                "File not found: {}",
                file_path.display()
            )));
        }

        // When content includes a # Title line and no explicit title param was given,
        // extract the title from the content so it isn't silently reverted to the stale DB value.
        let extracted_title = if new_title.is_none() {
            new_content.and_then(crate::patterns::extract_heading_title)
        } else {
            None
        };
        let title = new_title
            .map(|t| t.to_string())
            .or(extracted_title)
            .unwrap_or_else(|| doc.title.clone());

        let content = new_content.unwrap_or(&doc.content);

        // Strip existing factbase header and title from content to avoid duplication
        let mut body = strip_factbase_header(if new_content.is_some() {
            content
        } else {
            &doc.content
        });

        // Preserve review queue: if old content had one but new content doesn't,
        // append the old review queue so unanswered questions aren't silently dropped.
        if new_content.is_some() {
            let old_rq_start = body_end_offset(&doc.content);
            let new_rq_start = body_end_offset(&body);
            if old_rq_start < doc.content.len() && new_rq_start >= body.len() {
                let old_review_queue = &doc.content[old_rq_start..];
                if !body.ends_with('\n') {
                    body.push('\n');
                }
                body.push_str(old_review_queue);
            }
        }

        // Deduplicate inline acronym expansions (e.g. "DR (Disaster Recovery)" repeated 4x).
        // Strip all expansions for terms defined in glossary documents.
        if new_content.is_some() {
            let glossary = load_glossary_terms(db, Some(&doc.repo_id));
            body = crate::processor::dedup_acronym_expansions(&body, &glossary);
        }

        let doc_content = {
            let repo = db.require_repository(&doc.repo_id)?;
            let resolved_format = resolve_repo_format(&repo);
            let doc_type = crate::processor::DocumentProcessor::new()
                .derive_type(&file_path, &repo.path);
            let mut extra = crate::processor::extract_extra_frontmatter(&doc.content);
            if new_content.is_some() {
                let new_extra = crate::processor::extract_extra_frontmatter(content);
                merge_frontmatter_fields(&mut extra, &new_extra);
            }
            let header = crate::processor::build_document_header(
                &id,
                &title,
                Some(&doc_type),
                &resolved_format,
                &extra,
            );
            format!("{header}{body}")
        };
        fs::write(&file_path, &doc_content)?;

        let new_hash = crate::processor::content_hash(&doc_content);
        db.update_document_content(&id, &doc_content, &new_hash)?;
        db.update_document_title(&id, &title)?;

        title
    } else {
        doc.title.clone()
    };

    // Store organization suggestions if provided (advisory only — not executed here)
    let mut stored_suggestions = Vec::new();
    let source = get_str_arg(args, "source").unwrap_or("update");
    if let Some(mv) = suggested_move {
        db.insert_suggestion(&id, "move", mv, source)?;
        stored_suggestions.push("move");
    }
    if let Some(rn) = suggested_rename {
        db.insert_suggestion(&id, "rename", rn, source)?;
        stored_suggestions.push("rename");
    }
    if let Some(st) = suggested_title {
        db.insert_suggestion(&id, "title", st, source)?;
        stored_suggestions.push("title");
    }

    let mut response = serde_json::json!({
        "id": id,
        "title": title,
        "file_path": file_path.to_string_lossy(),
        "title_changed": new_title.is_some(),
        "content_changed": new_content.is_some(),
        "message": format!("Document '{}' ({}) updated. Run scan to re-index.", title, id)
    });
    if !stored_suggestions.is_empty() {
        response["suggestions_stored"] = serde_json::json!(stored_suggestions);
    }

    Ok(response)
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
        "doc_type": doc.doc_type,
        "file_path": doc.file_path,
        "message": format!("Document '{}' ({}) deleted.", doc.title, id)
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
    let repo = resolve_repo(db, get_str_arg(args, "repo"))?;
    let repo_id = &repo.id;
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
    let glossary = load_glossary_terms(db, Some(repo_id));
    let resolved_format = resolve_repo_format(&repo);
    for (i, validated) in validated_docs.iter().enumerate() {
        let id = processor.generate_unique_id(db);
        let content_trimmed = strip_leading_title(validated.content, validated.title);
        let content_deduped =
            crate::processor::dedup_acronym_expansions(content_trimmed, &glossary);
        let file_path: PathBuf = repo.path.join(validated.path);
        let doc_type = processor.derive_type(&file_path, &repo.path);
        let header = crate::processor::build_document_header(
            &id,
            validated.title,
            Some(&doc_type),
            &resolved_format,
            &[],
        );
        let doc_content = format!("{header}{content_deduped}");

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
    fn test_merge_frontmatter_fields_override() {
        let mut base = vec!["reviewed: 2026-01-01".into(), "tags: old".into()];
        let overrides = vec!["tags: new".into()];
        merge_frontmatter_fields(&mut base, &overrides);
        assert_eq!(base, vec!["reviewed: 2026-01-01", "tags: new"]);
    }

    #[test]
    fn test_merge_frontmatter_fields_add_new() {
        let mut base = vec!["reviewed: 2026-01-01".into()];
        let overrides = vec!["custom: value".into()];
        merge_frontmatter_fields(&mut base, &overrides);
        assert_eq!(base, vec!["reviewed: 2026-01-01", "custom: value"]);
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
    fn test_update_document_preserves_review_queue() {
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

        let old_content = "<!-- factbase:abc123 -->\n# Title\n\nSome facts\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` When did this happen?\n  > \n";
        let file = repo_dir.path().join("test.md");
        fs::write(&file, old_content).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Title".into(),
            content: old_content.into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Agent submits new content WITHOUT review queue
        let args = serde_json::json!({"id": "abc123", "content": "Updated facts here"});
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("Updated facts here"), "new body present");
        assert!(on_disk.contains("## Review Queue"), "review queue preserved");
        assert!(on_disk.contains("@q[temporal]"), "question preserved");
    }

    #[test]
    fn test_update_document_respects_explicit_review_queue() {
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

        let old_content = "<!-- factbase:abc123 -->\n# Title\n\nFacts\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[temporal]` Old question\n  > \n";
        let file = repo_dir.path().join("test.md");
        fs::write(&file, old_content).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Title".into(),
            content: old_content.into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // Agent explicitly includes a different review queue
        let args = serde_json::json!({
            "id": "abc123",
            "content": "New facts\n\n---\n\n## Review Queue\n\n<!-- factbase:review -->\n- [ ] `@q[missing]` New question\n  > \n"
        });
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("@q[missing]"), "new queue used: {on_disk}");
        assert!(!on_disk.contains("@q[temporal]"), "old queue not preserved: {on_disk}");
    }

    #[test]
    fn test_update_document_no_old_review_queue() {
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

        let old_content = "<!-- factbase:abc123 -->\n# Title\n\nPlain facts";
        let file = repo_dir.path().join("test.md");
        fs::write(&file, old_content).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "test.md".into(),
            title: "Title".into(),
            content: old_content.into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        // No old review queue, no new review queue — nothing to preserve
        let args = serde_json::json!({"id": "abc123", "content": "Updated facts"});
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("Updated facts"), "body updated");
        assert!(!on_disk.contains("Review Queue"), "no queue injected: {on_disk}");
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

    #[test]
    fn test_update_document_deduplicates_acronym_expansions() {
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

        let args = serde_json::json!({
            "id": "abc123",
            "content": "- DR (Disaster Recovery) plan\n- DR (Disaster Recovery) site\n- DR (Disaster Recovery) budget"
        });
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        // First expansion kept, subsequent stripped
        assert!(on_disk.contains("DR (Disaster Recovery) plan"), "{on_disk}");
        assert!(on_disk.contains("- DR site"), "second should be stripped: {on_disk}");
        assert!(on_disk.contains("- DR budget"), "third should be stripped: {on_disk}");
    }

    #[test]
    fn test_update_document_strips_glossary_acronyms() {
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

        // Create a glossary document that defines "SLA"
        let glossary_doc = Document {
            id: "glos01".into(),
            repo_id: "r1".into(),
            file_path: "glossary.md".into(),
            title: "Glossary".into(),
            doc_type: Some("glossary".into()),
            content: "# Glossary\n\n**SLA**: Service Level Agreement\n**DR**: Disaster Recovery".into(),
            file_hash: "g1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&glossary_doc).unwrap();

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

        let args = serde_json::json!({
            "id": "abc123",
            "content": "- SLA (Service Level Agreement) defined\n- DR (Disaster Recovery) plan"
        });
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        // Both terms in glossary → all expansions stripped
        assert!(on_disk.contains("- SLA defined"), "glossary term stripped: {on_disk}");
        assert!(on_disk.contains("- DR plan"), "glossary term stripped: {on_disk}");
    }

    #[test]
    fn test_create_document_deduplicates_acronym_expansions() {
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
            "content": "- DR (Disaster Recovery) plan\n- DR (Disaster Recovery) site"
        });
        create_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(repo_dir.path().join("test.md")).unwrap();
        assert!(on_disk.contains("DR (Disaster Recovery) plan"), "{on_disk}");
        assert!(on_disk.contains("- DR site"), "deduped: {on_disk}");
    }

    #[test]
    fn test_strip_factbase_header_html_comment() {
        let content = "<!-- factbase:abc123 -->\n# Title\n\nBody text";
        let body = strip_factbase_header(content);
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_strip_factbase_header_frontmatter() {
        let content = "---\nfactbase_id: abc123\ntype: person\n---\n# Title\n\nBody text";
        let body = strip_factbase_header(content);
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_strip_factbase_header_comment_plus_frontmatter() {
        let content = "<!-- factbase:abc123 -->\n---\ntype: person\n---\n# Title\n\nBody text";
        let body = strip_factbase_header(content);
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_strip_factbase_header_no_header() {
        let content = "# Title\n\nBody text";
        let body = strip_factbase_header(content);
        assert_eq!(body, "Body text");
    }

    #[test]
    fn test_create_document_obsidian_format() {
        use crate::database::tests::test_db;
        use crate::models::{FormatConfig, Perspective, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: Some(Perspective {
                format: Some(FormatConfig {
                    preset: Some("obsidian".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({
            "repo": "r1",
            "path": "people/john.md",
            "title": "John Doe",
            "content": "- Works at Acme Corp"
        });
        let result = create_document(&db, &args).unwrap();
        assert!(result["id"].is_string());

        let on_disk = fs::read_to_string(repo_dir.path().join("people/john.md")).unwrap();
        // Should have YAML frontmatter with factbase_id
        assert!(on_disk.starts_with("---\n"), "should start with frontmatter: {on_disk}");
        assert!(on_disk.contains("factbase_id:"), "should have factbase_id: {on_disk}");
        assert!(on_disk.contains("type: people"), "should have type: {on_disk}");
        assert!(!on_disk.contains("<!-- factbase:"), "should NOT have HTML comment: {on_disk}");
        assert!(on_disk.contains("# John Doe"), "should have title: {on_disk}");
    }

    #[test]
    fn test_create_document_default_format() {
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
            "path": "notes/test.md",
            "title": "Test Note",
            "content": "- A fact"
        });
        create_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(repo_dir.path().join("notes/test.md")).unwrap();
        // Default format: HTML comment, no frontmatter
        assert!(on_disk.starts_with("<!-- factbase:"), "should start with HTML comment: {on_disk}");
        assert!(!on_disk.contains("---\n"), "should NOT have frontmatter: {on_disk}");
    }

    #[test]
    fn test_update_document_preserves_extra_frontmatter() {
        use crate::database::tests::test_db;
        use crate::models::{Document, FormatConfig, Perspective, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: Some(Perspective {
                format: Some(FormatConfig {
                    preset: Some("obsidian".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let original = "---\nfactbase_id: abc123\nreviewed: 2026-02-21\ntype: people\ntags: important\n---\n# Test Entity\n\n- Old fact";
        let file = repo_dir.path().join("people/test.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, original).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "people/test.md".into(),
            title: "Test Entity".into(),
            content: original.into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "id": "abc123",
            "content": "- New fact\n- Another fact"
        });
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("reviewed: 2026-02-21"), "reviewed field preserved: {on_disk}");
        assert!(on_disk.contains("tags: important"), "tags field preserved: {on_disk}");
        assert!(on_disk.contains("factbase_id: abc123"), "factbase_id preserved: {on_disk}");
        assert!(on_disk.contains("type: people"), "type preserved: {on_disk}");
        assert!(on_disk.contains("- New fact"), "new content present: {on_disk}");
    }

    #[test]
    fn test_update_document_preserves_frontmatter_comment_format() {
        use crate::database::tests::test_db;
        use crate::models::{Document, FormatConfig, Perspective, Repository};
        use tempfile::TempDir;

        let (db, _tmp) = test_db();
        let repo_dir = TempDir::new().unwrap();
        let repo = Repository {
            id: "r1".into(),
            name: "R1".into(),
            path: repo_dir.path().to_path_buf(),
            perspective: Some(Perspective {
                format: Some(FormatConfig {
                    frontmatter: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        let original = "<!-- factbase:abc123 -->\n---\ntype: people\nreviewed: 2026-03-06\n---\n# Entity\n\n- Old fact";
        let file = repo_dir.path().join("people/test.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, original).unwrap();

        let doc = Document {
            id: "abc123".into(),
            repo_id: "r1".into(),
            file_path: "people/test.md".into(),
            title: "Entity".into(),
            content: original.into(),
            file_hash: "h1".into(),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();

        let args = serde_json::json!({
            "id": "abc123",
            "content": "- Updated fact"
        });
        update_document(&db, &args).unwrap();

        let on_disk = fs::read_to_string(&file).unwrap();
        assert!(on_disk.contains("reviewed: 2026-03-06"), "reviewed preserved: {on_disk}");
        assert!(on_disk.contains("type: people"), "type preserved: {on_disk}");
        assert!(on_disk.contains("- Updated fact"), "new content: {on_disk}");
    }

    #[test]
    fn test_create_document_without_repo_param() {
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

        // No "repo" parameter — should auto-resolve to the single repo
        let args = serde_json::json!({
            "path": "test.md",
            "title": "Test",
            "content": "- A fact @t[2024] [^1]"
        });
        let result = create_document(&db, &args).unwrap();
        assert!(result["id"].is_string());
        assert_eq!(result["title"], "Test");
        assert!(repo_dir.path().join("test.md").exists());
    }

    #[test]
    fn test_create_document_no_repo_exists() {
        use crate::database::tests::test_db;

        let (db, _tmp) = test_db();
        let args = serde_json::json!({
            "path": "test.md",
            "title": "Test",
            "content": "body"
        });
        let result = create_document(&db, &args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No repository found"), "error should be helpful: {err}");
    }

    #[test]
    fn test_bulk_create_without_repo_param() {
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

        // No "repo" parameter
        let args = serde_json::json!({
            "documents": [
                {"path": "a.md", "title": "A", "content": "- fact @t[2024] [^1]"},
                {"path": "b.md", "title": "B", "content": "- fact @t[2024] [^1]"}
            ]
        });
        let result = bulk_create_documents(&db, &args, &ProgressReporter::Silent).unwrap();
        assert_eq!(result["success"], true);
        let created = result["created"].as_array().unwrap();
        assert_eq!(created.len(), 2);
        assert!(repo_dir.path().join("a.md").exists());
        assert!(repo_dir.path().join("b.md").exists());
    }

    #[test]
    fn test_bulk_create_no_repo_exists() {
        use crate::database::tests::test_db;

        let (db, _tmp) = test_db();
        let args = serde_json::json!({
            "documents": [{"path": "a.md", "title": "A", "content": "body"}]
        });
        let result = bulk_create_documents(&db, &args, &ProgressReporter::Silent);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No repository found"), "error should be helpful: {err}");
    }
}
