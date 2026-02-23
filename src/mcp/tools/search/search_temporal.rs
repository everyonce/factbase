//! search_temporal MCP tool - temporal-aware semantic search

use super::{apply_temporal_filter, fetch_docs_content, parse_during_range};
use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::mcp::tools::{
    extract_type_repo_filters, get_bool_arg, get_str_arg, get_str_arg_required, get_u64_arg,
    run_blocking,
};
use crate::models::{TemporalTag, TemporalTagType};
use crate::processor::{calculate_recency_boost, parse_temporal_tags};
use serde_json::Value;
use std::collections::HashMap;
use tracing::instrument;

/// Dedicated temporal-aware search with rich metadata.
///
/// Combines semantic search with temporal filtering and returns detailed
/// temporal metadata for each result including tag types, date ranges,
/// and confidence levels.
///
/// # Arguments (from JSON)
/// - `query` (required): Semantic search query text
/// - `as_of` (optional): Filter to facts valid at date (YYYY, YYYY-MM, YYYY-MM-DD)
/// - `during` (optional): Filter to facts valid during range (YYYY..YYYY)
/// - `exclude_unknown` (optional): Exclude facts with @t[?] tags (default: false)
/// - `boost_recent` (optional): Boost ranking of recent @t[~...] dates (default: false)
/// - `limit` (optional): Max results (default: 10)
/// - `doc_type` (optional): Filter by document type
/// - `repo` (optional): Filter by repository ID
///
/// # Returns
/// JSON with `results` array (includes `temporal` metadata per result),
/// `count`, `query`, and `filters` applied.
///
/// # Errors
/// - `FactbaseError::Parse` if during format is invalid
#[instrument(name = "mcp_search_temporal", skip(db, embedding, args))]
pub async fn search_temporal<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let query = get_str_arg_required(args, "query")?;
    let limit = get_u64_arg(args, "limit", 10) as usize;
    let (doc_type, repo) = extract_type_repo_filters(args);
    let as_of = get_str_arg(args, "as_of").map(String::from);
    let during = get_str_arg(args, "during").map(String::from);
    let exclude_unknown = get_bool_arg(args, "exclude_unknown", false);
    let boost_recent = get_bool_arg(args, "boost_recent", false);

    // Parse during range if provided
    let during_range = during.as_deref().map(parse_during_range).transpose()?;

    // Generate query embedding
    let query_embedding = embedding.generate(&query).await?;

    // Fetch more results to ensure enough after filtering (5x limit)
    let fetch_limit = limit * 5;

    // Combined spawn_blocking: search + fetch content in single blocking call
    let db_clone = db.clone();
    let query_for_search = query.clone();
    let (paginated, docs_content) = run_blocking(move || {
        let paginated = db_clone.search_semantic_paginated(
            &query_embedding,
            fetch_limit,
            0,
            doc_type.as_deref(),
            repo.as_deref(),
            Some(&query_for_search),
        )?;

        // Fetch document content for temporal analysis in same blocking context
        let ids: Vec<String> = paginated.results.iter().map(|r| r.id.clone()).collect();
        let map = fetch_docs_content(&db_clone, &ids)?;
        Ok((paginated, map))
    })
    .await?;

    let mut results = paginated.results;

    // Apply temporal filtering
    apply_temporal_filter(
        &mut results,
        &docs_content,
        as_of.as_deref(),
        during_range.as_ref(),
        exclude_unknown,
    );

    // Apply recency boosting if requested
    if boost_recent {
        // Default: 180 days window, 0.2 boost factor
        let window_days = 180u32;
        let boost_factor = 0.2f32;

        for r in &mut results {
            if let Some(content) = docs_content.get(&r.id) {
                let tags = parse_temporal_tags(content);
                let boost = calculate_recency_boost(&tags, window_days, boost_factor);
                r.relevance_score *= boost;
            }
        }

        // Re-sort by boosted relevance score
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Truncate to requested limit
    results.truncate(limit);

    // Build response with temporal metadata computed inline
    let items: Vec<Value> = results
        .into_iter()
        .map(|r| {
            let id = r.id.clone();
            let mut item = r.to_json();

            // Compute temporal metadata inline (avoids HashMap and clone)
            if let Some(content) = docs_content.get(&id) {
                let tags = parse_temporal_tags(content);
                if !tags.is_empty() {
                    item["temporal"] = build_temporal_metadata(&tags);
                }
            }

            item
        })
        .collect();

    Ok(serde_json::json!({
        "results": items,
        "count": items.len(),
        "query": query,
        "filters": {
            "as_of": as_of,
            "during": during,
            "exclude_unknown": exclude_unknown,
            "boost_recent": boost_recent
        }
    }))
}

/// Build temporal metadata for a document's tags
pub(crate) fn build_temporal_metadata(tags: &[TemporalTag]) -> Value {
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut date_range_start: Option<&str> = None;
    let mut date_range_end: Option<&str> = None;
    let mut has_unknown = false;

    for tag in tags {
        // Count by type
        let type_name = match tag.tag_type {
            TemporalTagType::PointInTime => "point_in_time",
            TemporalTagType::LastSeen => "last_seen",
            TemporalTagType::Range => "range",
            TemporalTagType::Ongoing => "ongoing",
            TemporalTagType::Historical => "historical",
            TemporalTagType::Unknown => {
                has_unknown = true;
                "unknown"
            }
        };
        *by_type.entry(type_name.to_string()).or_insert(0) += 1;

        // Track date range using references
        if let Some(ref start) = tag.start_date {
            if date_range_start.is_none_or(|s| start.as_str() < s) {
                date_range_start = Some(start.as_str());
            }
        }
        if let Some(ref end) = tag.end_date {
            if date_range_end.is_none_or(|e| end.as_str() > e) {
                date_range_end = Some(end.as_str());
            }
        }
    }

    // Determine confidence level based on tag types
    let confidence = if has_unknown {
        "low"
    } else if by_type.contains_key("point_in_time") || by_type.contains_key("range") {
        "high"
    } else {
        "medium"
    };

    serde_json::json!({
        "tag_count": tags.len(),
        "by_type": by_type,
        "date_range": {
            "start": date_range_start,
            "end": date_range_end
        },
        "confidence": confidence
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tag(tag_type: TemporalTagType, start: Option<&str>, end: Option<&str>) -> TemporalTag {
        TemporalTag {
            tag_type,
            start_date: start.map(String::from),
            end_date: end.map(String::from),
            line_number: 1,
            raw_text: String::new(),
        }
    }

    #[test]
    fn test_build_temporal_metadata_empty() {
        let tags: Vec<TemporalTag> = vec![];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["tag_count"], 0);
        assert_eq!(result["confidence"], "medium");
    }

    #[test]
    fn test_build_temporal_metadata_point_in_time() {
        let tags = vec![make_tag(
            TemporalTagType::PointInTime,
            Some("2024-01"),
            None,
        )];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["tag_count"], 1);
        assert_eq!(result["by_type"]["point_in_time"], 1);
        assert_eq!(result["confidence"], "high");
        assert_eq!(result["date_range"]["start"], "2024-01");
    }

    #[test]
    fn test_build_temporal_metadata_range() {
        let tags = vec![make_tag(TemporalTagType::Range, Some("2020"), Some("2022"))];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["tag_count"], 1);
        assert_eq!(result["by_type"]["range"], 1);
        assert_eq!(result["confidence"], "high");
        assert_eq!(result["date_range"]["start"], "2020");
        assert_eq!(result["date_range"]["end"], "2022");
    }

    #[test]
    fn test_build_temporal_metadata_unknown_low_confidence() {
        let tags = vec![make_tag(TemporalTagType::Unknown, None, None)];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["tag_count"], 1);
        assert_eq!(result["by_type"]["unknown"], 1);
        assert_eq!(result["confidence"], "low");
    }

    #[test]
    fn test_build_temporal_metadata_ongoing_medium_confidence() {
        let tags = vec![make_tag(TemporalTagType::Ongoing, Some("2023"), None)];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["confidence"], "medium");
        assert_eq!(result["by_type"]["ongoing"], 1);
    }

    #[test]
    fn test_build_temporal_metadata_multiple_tags() {
        let tags = vec![
            make_tag(TemporalTagType::Range, Some("2018"), Some("2020")),
            make_tag(TemporalTagType::Range, Some("2021"), Some("2023")),
            make_tag(TemporalTagType::LastSeen, Some("2024-06"), None),
        ];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["tag_count"], 3);
        assert_eq!(result["by_type"]["range"], 2);
        assert_eq!(result["by_type"]["last_seen"], 1);
        // Date range: start is min of all start_dates, end is max of all end_dates
        // LastSeen has start_date but no end_date
        assert_eq!(result["date_range"]["start"], "2018");
        assert_eq!(result["date_range"]["end"], "2023");
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
    fn test_build_temporal_metadata_date_range_tracking() {
        // Test that date range correctly finds min start and max end
        let tags = vec![
            make_tag(TemporalTagType::Range, Some("2020-06"), Some("2021-03")),
            make_tag(TemporalTagType::Range, Some("2019-01"), Some("2022-12")),
        ];
        let result = build_temporal_metadata(&tags);
        assert_eq!(result["date_range"]["start"], "2019-01");
        assert_eq!(result["date_range"]["end"], "2022-12");
    }
}
