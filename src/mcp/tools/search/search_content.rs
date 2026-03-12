//! search_content MCP tool - text/regex search (like grep)

use crate::database::{ContentSearchParams, Database};
use crate::error::FactbaseError;
use crate::mcp::tools::{
    extract_type_repo_filters, get_str_arg_required, get_u64_arg, resolve_repo_filter,
};
use crate::models::ContentSearchResult;
use crate::ProgressReporter;
use serde_json::Value;
use tracing::instrument;

/// Searches document content for exact text matches (like grep).
///
/// # Arguments (from JSON)
/// - `pattern` (required): Text pattern to search for
/// - `limit` (optional): Max results (default: 10)
/// - `doc_type` (optional): Filter by document type
/// - `repo` (optional): Filter by repository ID
/// - `context` (optional): Lines of context around matches (default: 0)
///
/// # Returns
/// JSON with `results` array, `count`, and `pattern`.
#[instrument(name = "mcp_search_content", skip(db, args, progress))]
pub fn search_content(
    db: &Database,
    args: &Value,
    progress: &ProgressReporter,
) -> Result<Value, FactbaseError> {
    let pattern = get_str_arg_required(args, "pattern")?;
    let limit = get_u64_arg(args, "limit", 10) as usize;
    let (doc_type, repo) = extract_type_repo_filters(args);
    let repo = resolve_repo_filter(db, repo.as_deref())?;
    let context = get_u64_arg(args, "context", 0) as usize;

    let params = ContentSearchParams {
        pattern: &pattern,
        limit,
        doc_type: doc_type.as_deref(),
        repo_id: repo.as_deref(),
        context_lines: context,
        since: None,
        progress,
    };
    let results = db.search_content(&params)?;

    let items: Vec<Value> = results
        .into_iter()
        .map(ContentSearchResult::to_json)
        .collect();

    Ok(serde_json::json!({
        "results": items,
        "count": items.len(),
        "pattern": pattern
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::{get_str_arg, get_u64_arg};

    #[test]
    fn test_pattern_required() {
        let args = serde_json::json!({});
        let result = search_content(
            &crate::database::tests::test_db().0,
            &args,
            &crate::ProgressReporter::Silent,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pattern"));
    }

    #[test]
    fn test_defaults() {
        let args = serde_json::json!({"pattern": "test"});
        let limit = get_u64_arg(&args, "limit", 10);
        let context = get_u64_arg(&args, "context", 0);
        let doc_type = get_str_arg(&args, "doc_type");
        let repo = get_str_arg(&args, "repo");
        assert_eq!(limit, 10);
        assert_eq!(context, 0);
        assert!(doc_type.is_none());
        assert!(repo.is_none());
    }

    #[test]
    fn test_custom_args() {
        let args = serde_json::json!({
            "pattern": "hello",
            "limit": 25,
            "context": 3,
            "doc_type": "person",
            "repo": "notes"
        });
        assert_eq!(get_u64_arg(&args, "limit", 10), 25);
        assert_eq!(get_u64_arg(&args, "context", 0), 3);
        assert_eq!(get_str_arg(&args, "doc_type"), Some("person"));
        assert_eq!(get_str_arg(&args, "repo"), Some("notes"));
    }

    #[test]
    fn test_doc_type_not_type() {
        let args = serde_json::json!({"pattern": "x", "doc_type": "person"});
        assert_eq!(get_str_arg(&args, "doc_type"), Some("person"));
        assert_eq!(get_str_arg(&args, "type"), None);
    }

    #[test]
    fn test_empty_results_response_format() {
        let (db, _dir) = crate::database::tests::test_db();
        let args = serde_json::json!({"pattern": "nonexistent"});
        let result = search_content(&db, &args, &crate::ProgressReporter::Silent).unwrap();
        assert_eq!(result["count"], 0);
        assert_eq!(result["pattern"], "nonexistent");
        assert!(result["results"].as_array().unwrap().is_empty());
    }
}
