//! search_knowledge MCP tool - semantic search across documents

use super::{apply_temporal_filter, fetch_docs_content, parse_during_range};
use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{
    extract_type_repo_filters, get_bool_arg, get_str_arg, get_u64_arg, resolve_repo_filter,
    run_blocking,
};
use serde_json::Value;
use std::collections::HashMap;
use tracing::instrument;

/// Performs semantic search or title-based search across documents.
///
/// Uses embedding vectors for semantic similarity matching. Supports
/// temporal filtering to find facts valid at specific dates.
///
/// # Arguments (from JSON)
/// - `query` (optional): Semantic search query text
/// - `title_filter` (optional): Filter by title (partial match, takes precedence)
/// - `limit` (optional): Max results (default: 10)
/// - `offset` (optional): Skip results for pagination (default: 0)
/// - `doc_type` (optional): Filter by document type
/// - `repo` (optional): Filter by repository ID
/// - `as_of` (optional): Filter to facts valid at date (YYYY, YYYY-MM, YYYY-MM-DD)
/// - `during` (optional): Filter to facts valid during range (YYYY..YYYY)
/// - `exclude_unknown` (optional): Exclude facts with @t[?] tags (default: false)
///
/// # Returns
/// JSON with `results` array (id, title, type, relevance_score, snippet, chunk info),
/// `type_counts`, `total_count`, `offset`, and `limit`.
///
/// # Errors
/// - `FactbaseError::Parse` if neither query nor title_filter provided
/// - `FactbaseError::Parse` if during format is invalid
#[instrument(name = "mcp_search_knowledge", skip(db, embedding, args))]
pub async fn search_knowledge<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
) -> Result<Value, FactbaseError> {
    // Delegate to search_temporal when boost_recent is requested
    let boost_recent = get_bool_arg(args, "boost_recent", false);
    if boost_recent {
        return super::search_temporal::search_temporal(db, embedding, args).await;
    }

    let query = get_str_arg(args, "query");
    let title_filter = get_str_arg(args, "title_filter");
    let limit = get_u64_arg(args, "limit", 10) as usize;
    let offset = get_u64_arg(args, "offset", 0) as usize;
    let (doc_type, repo) = extract_type_repo_filters(args);
    let repo = resolve_repo_filter(db, repo.as_deref())?;
    let as_of = get_str_arg(args, "as_of").map(String::from);
    let during = get_str_arg(args, "during").map(String::from);
    let exclude_unknown = get_bool_arg(args, "exclude_unknown", false);

    // Parse during range if provided
    let during_range = during.as_deref().map(parse_during_range).transpose()?;

    // Determine if temporal filtering is needed
    let needs_temporal_filter = as_of.is_some() || during_range.is_some() || exclude_unknown;

    // Fetch more results if filtering (5x to ensure enough after filtering)
    let fetch_limit = if needs_temporal_filter {
        limit * 5
    } else {
        limit
    };

    let (mut results, total_count) = match (query, title_filter) {
        (_, Some(tf)) => {
            // Title-based search (takes precedence)
            let db_clone = db.clone();
            let tf = tf.to_string();
            let results = run_blocking(move || {
                db_clone.search_by_title(&tf, fetch_limit, doc_type.as_deref(), repo.as_deref())
            })
            .await?;
            let count = results.len();
            (results, count)
        }
        (Some(q), None) => {
            // Semantic search with pagination
            let query_str = q.to_string();
            let query_embedding = embedding.generate(&query_str).await?;

            let db_clone = db.clone();
            let paginated = run_blocking(move || {
                db_clone.search_semantic_paginated(
                    &query_embedding,
                    fetch_limit,
                    offset,
                    doc_type.as_deref(),
                    repo.as_deref(),
                    Some(&query_str),
                )
            })
            .await?;
            (paginated.results, paginated.total_count)
        }
        (None, None) => {
            return Err(FactbaseError::parse(
                "Missing query or title_filter parameter",
            ));
        }
    };

    // Apply temporal filtering if requested
    if needs_temporal_filter {
        let db_clone = db.clone();
        let result_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();

        let docs_content = run_blocking(move || fetch_docs_content(&db_clone, &result_ids)).await?;

        apply_temporal_filter(
            &mut results,
            &docs_content,
            as_of.as_deref(),
            during_range.as_ref(),
            exclude_unknown,
        );

        // Truncate to requested limit
        results.truncate(limit);
    }

    // Aggregate type counts and build items in single pass
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    let items: Vec<Value> = results
        .into_iter()
        .map(|r| {
            let type_key = r.doc_type.as_deref().unwrap_or("unknown").to_string();
            *type_counts.entry(type_key).or_insert(0) += 1;
            r.to_json()
        })
        .collect();

    Ok(serde_json::json!({
        "results": items,
        "type_counts": type_counts,
        "total_count": total_count,
        "offset": offset,
        "limit": limit
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_args_defaults() {
        let args = serde_json::json!({});
        let limit = get_u64_arg(&args, "limit", 10);
        let offset = get_u64_arg(&args, "offset", 0);
        let exclude_unknown = get_bool_arg(&args, "exclude_unknown", false);
        assert_eq!(limit, 10);
        assert_eq!(offset, 0);
        assert!(!exclude_unknown);
    }

    #[test]
    fn test_get_args_custom_values() {
        let args = serde_json::json!({
            "limit": 25,
            "offset": 50,
            "exclude_unknown": true
        });
        let limit = get_u64_arg(&args, "limit", 10);
        let offset = get_u64_arg(&args, "offset", 0);
        let exclude_unknown = get_bool_arg(&args, "exclude_unknown", false);
        assert_eq!(limit, 25);
        assert_eq!(offset, 50);
        assert!(exclude_unknown);
    }

    #[test]
    fn test_title_filter_presence() {
        // When title_filter is present, it should be used (takes precedence)
        let args = serde_json::json!({
            "query": "semantic search",
            "title_filter": "exact title"
        });
        let query = get_str_arg(&args, "query");
        let title_filter = get_str_arg(&args, "title_filter");
        assert!(query.is_some());
        assert!(title_filter.is_some());
        // The match logic: (_, Some(tf)) matches first, so title_filter takes precedence
        assert_eq!(title_filter.unwrap(), "exact title");
    }

    #[test]
    fn test_query_only() {
        let args = serde_json::json!({
            "query": "semantic search"
        });
        let query = get_str_arg(&args, "query");
        let title_filter = get_str_arg(&args, "title_filter");
        assert!(query.is_some());
        assert!(title_filter.is_none());
    }

    #[test]
    fn test_doc_type_filter_extracted_from_args() {
        let args = serde_json::json!({
            "query": "test",
            "doc_type": "person"
        });
        let doc_type = get_str_arg(&args, "doc_type");
        assert_eq!(doc_type, Some("person"));

        // "type" should NOT work (old incorrect key)
        let doc_type_old = get_str_arg(&args, "type");
        assert_eq!(doc_type_old, None);
    }

    #[test]
    fn test_neither_query_nor_title_filter() {
        let args = serde_json::json!({
            "limit": 10
        });
        let query = get_str_arg(&args, "query");
        let title_filter = get_str_arg(&args, "title_filter");
        assert!(query.is_none());
        assert!(title_filter.is_none());
        // This would trigger the error case in the main function
    }
}
