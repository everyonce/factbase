//! Entity-related MCP tools: get_entity, list_entities, get_perspective, list_repositories, get_document_stats

use super::{get_bool_arg, get_str_arg, get_u64_arg};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::output::truncate_at_word_boundary;
use crate::processor::{
    calculate_fact_stats, count_facts_with_sources, parse_review_queue, parse_source_definitions,
    parse_source_references, parse_temporal_tags,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tracing::instrument;

/// Retrieves a document by ID with its link relationships.
///
/// Returns full document content plus incoming and outgoing links.
/// Optionally truncates content and generates a preview.
///
/// # Arguments (from JSON)
/// - `id` (required): Document ID (6-char hex)
/// - `include_preview` (optional): Generate 500-char preview (default: false)
/// - `max_content_length` (optional): Truncate content at word boundary (0 = no limit)
///
/// # Returns
/// JSON with `id`, `title`, `type`, `file_path`, `content`, `links_to`, `linked_from`,
/// `indexed_at`, and optionally `preview` and `content_truncated`.
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist
#[instrument(name = "mcp_get_entity", skip(db, args))]
pub fn get_entity(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let id = super::get_str_arg_required(args, "id")?;
    let include_preview = get_bool_arg(args, "include_preview", false);
    let max_content_length = get_u64_arg(args, "max_content_length", 0) as usize;

    let doc = db.require_document(&id)?;

    let links_to = db.get_links_from(&id)?;
    let linked_from = db.get_links_to(&id)?;

    // Build base from to_summary_json() before any moves
    let mut result = doc.to_summary_json();
    let obj = result
        .as_object_mut()
        .expect("to_summary_json returns object");

    // Generate preview if needed (before potentially moving content)
    if include_preview {
        obj.insert(
            "preview".into(),
            serde_json::json!(generate_preview(&doc.content, 500)),
        );
    }

    // Truncate content if max_content_length specified
    if max_content_length > 0 && doc.content.len() > max_content_length {
        obj.insert(
            "content".into(),
            serde_json::json!(truncate_at_word_boundary(&doc.content, max_content_length)),
        );
        obj.insert("content_truncated".into(), serde_json::json!(true));
    } else {
        obj.insert("content".into(), serde_json::json!(doc.content));
    }

    obj.insert("links_to".into(), serde_json::json!(links_to));
    obj.insert("linked_from".into(), serde_json::json!(linked_from));
    obj.insert(
        "indexed_at".into(),
        serde_json::json!(doc.indexed_at.to_rfc3339()),
    );

    Ok(result)
}

/// Lists documents with optional filtering.
///
/// Returns document metadata without full content.
///
/// # Arguments (from JSON)
/// - `doc_type` (optional): Filter by document type
/// - `repo` (optional): Filter by repository ID
/// - `title_filter` (optional): Filter by title (partial match)
/// - `limit` (optional): Max results (default: 50)
///
/// # Returns
/// JSON with `entities` array containing `id`, `title`, `type`, `file_path` for each.
#[instrument(name = "mcp_list_entities", skip(db, args))]
pub fn list_entities(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let doc_type = get_str_arg(args, "doc_type");
    let repo = get_str_arg(args, "repo");
    let title_filter = get_str_arg(args, "title_filter");
    let limit = get_u64_arg(args, "limit", 50) as usize;

    let docs = db.list_documents(doc_type, repo, title_filter, limit)?;

    let items: Vec<Value> = docs.into_iter().map(|d| d.to_summary_json()).collect();

    Ok(serde_json::json!({ "entities": items }))
}

/// Gets repository perspective (context from perspective.yaml).
///
/// Returns the perspective configuration for a repository, which provides
/// context about the knowledge base's purpose and allowed document types.
///
/// # Arguments (from JSON)
/// - `repo` (optional): Repository ID (uses first repo if not specified)
///
/// # Returns
/// JSON with repository summary (`id`, `name`, `path`, `document_count`,
/// `last_indexed_at`) plus `perspective` (parsed YAML content).
///
/// # Errors
/// - `FactbaseError::NotFound` if no repositories exist
#[instrument(name = "mcp_get_perspective", skip(db, args))]
pub fn get_perspective(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = get_str_arg(args, "repo");

    let repos = db.list_repositories_with_stats()?;
    let (repo, doc_count) = if let Some(id) = repo_id {
        repos.into_iter().find(|(r, _)| r.id == id)
    } else {
        repos.into_iter().next()
    }
    .ok_or_else(|| FactbaseError::not_found("No repository found"))?;

    let mut json = repo.to_summary_json(doc_count);
    json.as_object_mut()
        .expect("to_summary_json returns object")
        .insert("perspective".into(), serde_json::json!(repo.perspective));

    Ok(json)
}

/// Lists all registered repositories with document counts.
///
/// # Returns
/// JSON with `repositories` array containing `id`, `name`, `path`,
/// `document_count`, and `last_indexed_at` for each repository.
#[instrument(name = "mcp_list_repositories", skip(db))]
pub fn list_repositories(db: &Database) -> Result<Value, FactbaseError> {
    let repos = db.list_repositories_with_stats()?;

    let items: Vec<Value> = repos
        .into_iter()
        .map(|(r, doc_count)| r.to_summary_json(doc_count))
        .collect();

    Ok(serde_json::json!({ "repositories": items }))
}

/// Gets detailed statistics for a document.
///
/// Analyzes temporal tag coverage, source citations, link counts,
/// and review queue status.
///
/// # Arguments (from JSON)
/// - `id` (required): Document ID (6-char hex)
///
/// # Returns
/// JSON with:
/// - `temporal`: tag counts, coverage percentage, breakdown by type
/// - `sources`: reference/definition counts, orphan detection, by type
/// - `links`: incoming and outgoing counts
/// - `word_count`: total words
/// - `review_queue`: question counts and status
///
/// # Errors
/// - `FactbaseError::NotFound` if document doesn't exist
#[instrument(name = "mcp_get_document_stats", skip(db, args))]
pub fn get_document_stats(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let id = super::get_str_arg_required(args, "id")?;

    let doc = db.require_document(&id)?;

    // Temporal coverage
    let fact_stats = calculate_fact_stats(&doc.content);
    let temporal_tags = parse_temporal_tags(&doc.content);
    let mut temporal_by_type: HashMap<String, usize> = HashMap::new();
    for tag in &temporal_tags {
        let type_name = format!("{:?}", tag.tag_type);
        *temporal_by_type.entry(type_name).or_insert(0) += 1;
    }

    // Source coverage
    let facts_with_sources = count_facts_with_sources(&doc.content);
    let source_refs = parse_source_references(&doc.content);
    let source_defs = parse_source_definitions(&doc.content);

    // Orphan detection (collect numbers before consuming source_defs)
    let ref_numbers: HashSet<_> = source_refs.iter().map(|r| r.number).collect();
    let def_numbers: HashSet<_> = source_defs.iter().map(|d| d.number).collect();
    let orphan_refs = ref_numbers.difference(&def_numbers).count();
    let orphan_defs = def_numbers.difference(&ref_numbers).count();

    // Capture counts before consuming
    let source_ref_count = source_refs.len();
    let source_def_count = source_defs.len();

    // Count by type (consume source_defs to avoid clone)
    let mut source_by_type: HashMap<String, usize> = HashMap::new();
    for def in source_defs {
        *source_by_type.entry(def.source_type).or_insert(0) += 1;
    }

    // Link counts
    let links_to = db.get_links_from(&id)?;
    let linked_from = db.get_links_to(&id)?;

    // Word count (simple whitespace split)
    let word_count = crate::models::word_count(&doc.content);

    // Review queue status
    let review_queue = parse_review_queue(&doc.content);
    let (total_questions, answered_questions) = match &review_queue {
        Some(questions) => {
            let answered = questions.iter().filter(|q| q.answered).count();
            (questions.len(), answered)
        }
        None => (0, 0),
    };

    let mut result = doc.to_summary_json();
    let obj = result
        .as_object_mut()
        .expect("to_summary_json returns object");
    obj.insert(
        "temporal".into(),
        serde_json::json!({
            "total_facts": fact_stats.total_facts,
            "facts_with_tags": fact_stats.facts_with_tags,
            "coverage_percent": fact_stats.coverage,
            "tag_count": temporal_tags.len(),
            "by_type": temporal_by_type
        }),
    );
    obj.insert(
        "sources".into(),
        serde_json::json!({
            "total_facts": fact_stats.total_facts,
            "facts_with_sources": facts_with_sources,
            "coverage_percent": if fact_stats.total_facts > 0 {
                (facts_with_sources as f32 / fact_stats.total_facts as f32) * 100.0
            } else {
                0.0
            },
            "reference_count": source_ref_count,
            "definition_count": source_def_count,
            "orphan_refs": orphan_refs,
            "orphan_defs": orphan_defs,
            "by_type": source_by_type
        }),
    );
    obj.insert(
        "links".into(),
        serde_json::json!({
            "outgoing": links_to.len(),
            "incoming": linked_from.len()
        }),
    );
    obj.insert("word_count".into(), serde_json::json!(word_count));
    obj.insert(
        "review_queue".into(),
        serde_json::json!({
            "has_queue": review_queue.is_some(),
            "total_questions": total_questions,
            "answered_questions": answered_questions,
            "pending_questions": total_questions - answered_questions
        }),
    );

    Ok(result)
}

/// Generate a content preview, truncating at word boundary
fn generate_preview(content: &str, max_len: usize) -> String {
    // Skip factbase header and empty lines
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().starts_with("<!-- factbase:") && !l.trim().is_empty())
        .collect();

    let text = lines.join("\n");
    crate::output::truncate_at_word_boundary(&text, max_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_repositories_tool_format() {
        let repos: Vec<(crate::models::Repository, usize)> = vec![(
            crate::models::Repository {
                id: "test".to_string(),
                name: "Test Repo".to_string(),
                path: std::path::PathBuf::from("/tmp/test"),
                perspective: None,
                created_at: chrono::Utc::now(),
                last_indexed_at: None,
                last_lint_at: None,
            },
            5,
        )];

        let items: Vec<serde_json::Value> = repos
            .iter()
            .map(|(r, count)| r.to_summary_json(*count))
            .collect();

        let result = serde_json::json!({ "repositories": items });
        assert!(result["repositories"].is_array());
        assert_eq!(result["repositories"][0]["id"], "test");
        assert_eq!(result["repositories"][0]["document_count"], 5);
    }

    #[test]
    fn test_generate_preview_short_content() {
        let content = "Short content";
        assert_eq!(generate_preview(content, 500), "Short content");
    }

    #[test]
    fn test_generate_preview_truncates_at_word() {
        let content = "This is a longer piece of content that needs truncation";
        let preview = generate_preview(content, 30);
        assert!(preview.ends_with("..."));
        assert!(preview.len() <= 33); // 30 + "..."
    }

    #[test]
    fn test_generate_preview_skips_header() {
        let content = "<!-- factbase:abc123 -->\n\n# Title\n\nActual content here";
        let preview = generate_preview(content, 500);
        assert!(!preview.contains("factbase:"));
        assert!(preview.contains("Title"));
    }

    #[test]
    fn test_get_document_stats_response_structure() {
        // Test the JSON structure returned by get_document_stats
        // This tests the structure without needing a database
        let fact_stats =
            crate::processor::calculate_fact_stats("- Fact one @t[2020..2022]\n- Fact two");
        assert_eq!(fact_stats.total_facts, 2);
        assert_eq!(fact_stats.facts_with_tags, 1);

        let tags = crate::processor::parse_temporal_tags("- Fact @t[2020..2022]");
        assert_eq!(tags.len(), 1);

        let refs = crate::processor::parse_source_references("- Fact [^1]");
        assert_eq!(refs.len(), 1);

        let defs = crate::processor::parse_source_definitions("[^1]: LinkedIn, 2024-01-15");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].source_type, "LinkedIn");
    }

    #[test]
    fn test_list_entities_doc_type_filter_extracted() {
        let args = serde_json::json!({ "doc_type": "person" });
        let doc_type = get_str_arg(&args, "doc_type");
        assert_eq!(doc_type, Some("person"));

        // "type" should NOT work (old incorrect key)
        let doc_type_old = get_str_arg(&args, "type");
        assert_eq!(doc_type_old, None);
    }

    #[test]
    fn test_generate_preview_only_header() {
        let content = "<!-- factbase:abc123 -->\n\n";
        let preview = generate_preview(content, 500);
        assert_eq!(preview, "");
    }

    #[test]
    fn test_generate_preview_multiple_empty_lines() {
        let content = "<!-- factbase:abc123 -->\n\n\n\n# Title\n\n\nContent";
        let preview = generate_preview(content, 500);
        assert!(!preview.contains("factbase:"));
        assert!(preview.contains("Title"));
        assert!(preview.contains("Content"));
    }

    #[test]
    fn test_generate_preview_preserves_newlines() {
        let content = "Line one\nLine two\nLine three";
        let preview = generate_preview(content, 500);
        assert!(preview.contains('\n'));
        assert!(preview.contains("Line one"));
        assert!(preview.contains("Line three"));
    }
}
