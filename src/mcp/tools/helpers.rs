//! Helper functions for MCP tool argument extraction.
//!
//! These functions provide consistent argument parsing across all MCP tools.

use crate::error::FactbaseError;
use crate::processor::{
    calculate_fact_stats, count_facts_with_sources, parse_review_queue, parse_source_definitions,
    parse_source_references, parse_temporal_tags,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

// Re-export shared run_blocking for MCP tool modules
pub(crate) use crate::async_helpers::run_blocking;

/// Extract optional string argument from JSON value.
pub(crate) fn get_str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Extract required string argument, returning error if missing.
pub(crate) fn get_str_arg_required(args: &Value, key: &str) -> Result<String, FactbaseError> {
    get_str_arg(args, key)
        .map(String::from)
        .ok_or_else(|| FactbaseError::parse(format!("Missing {key} parameter")))
}

/// Extract optional u64 argument with default value.
pub(crate) fn get_u64_arg(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(default)
}

/// Extract required u64 argument, returning error if missing.
pub(crate) fn get_u64_arg_required(args: &Value, key: &str) -> Result<u64, FactbaseError> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| FactbaseError::parse(format!("Missing {key} parameter")))
}

/// Extract optional `doc_type` and `repo` filter arguments.
///
/// Used by all MCP search tools to consistently extract type/repo filters.
pub(crate) fn extract_type_repo_filters(args: &Value) -> (Option<String>, Option<String>) {
    (
        get_str_arg(args, "doc_type").map(String::from),
        get_str_arg(args, "repo").map(String::from),
    )
}

/// Extract optional bool argument with default value.
pub(crate) fn get_bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Build temporal stats JSON from document content.
pub(crate) fn build_temporal_stats_json(content: &str) -> Value {
    let fact_stats = calculate_fact_stats(content);
    let temporal_tags = parse_temporal_tags(content);
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for tag in &temporal_tags {
        *by_type.entry(format!("{:?}", tag.tag_type)).or_insert(0) += 1;
    }
    serde_json::json!({
        "total_facts": fact_stats.total_facts,
        "facts_with_tags": fact_stats.facts_with_tags,
        "coverage_percent": fact_stats.coverage,
        "tag_count": temporal_tags.len(),
        "by_type": by_type
    })
}

/// Build source stats JSON from document content.
pub(crate) fn build_source_stats_json(content: &str) -> Value {
    let fact_stats = calculate_fact_stats(content);
    let facts_with_sources = count_facts_with_sources(content);
    let source_refs = parse_source_references(content);
    let source_defs = parse_source_definitions(content);
    let ref_numbers: HashSet<_> = source_refs.iter().map(|r| r.number).collect();
    let def_numbers: HashSet<_> = source_defs.iter().map(|d| d.number).collect();
    let orphan_refs = ref_numbers.difference(&def_numbers).count();
    let orphan_defs = def_numbers.difference(&ref_numbers).count();
    let source_ref_count = source_refs.len();
    let source_def_count = source_defs.len();
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for def in source_defs {
        *by_type.entry(def.source_type).or_insert(0) += 1;
    }
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
        "by_type": by_type
    })
}

/// Build link stats JSON from outgoing/incoming counts.
pub(crate) fn build_link_stats_json(outgoing: usize, incoming: usize) -> Value {
    serde_json::json!({
        "outgoing": outgoing,
        "incoming": incoming
    })
}

/// Build review queue stats JSON from document content.
pub(crate) fn build_review_stats_json(content: &str) -> Value {
    let review_queue = parse_review_queue(content);
    let (total, answered) = match &review_queue {
        Some(questions) => (
            questions.len(),
            questions.iter().filter(|q| q.answered).count(),
        ),
        None => (0, 0),
    };
    serde_json::json!({
        "has_queue": review_queue.is_some(),
        "total_questions": total,
        "answered_questions": answered,
        "pending_questions": total - answered
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_str_arg_present() {
        let args = serde_json::json!({"name": "test"});
        assert_eq!(get_str_arg(&args, "name"), Some("test"));
    }

    #[test]
    fn test_get_str_arg_missing() {
        let args = serde_json::json!({});
        assert_eq!(get_str_arg(&args, "name"), None);
    }

    #[test]
    fn test_get_str_arg_wrong_type() {
        let args = serde_json::json!({"name": 123});
        assert_eq!(get_str_arg(&args, "name"), None);
    }

    #[test]
    fn test_get_str_arg_required_present() {
        let args = serde_json::json!({"id": "abc123"});
        let result = get_str_arg_required(&args, "id");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "abc123");
    }

    #[test]
    fn test_get_str_arg_required_missing() {
        let args = serde_json::json!({});
        let result = get_str_arg_required(&args, "id");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing id"));
    }

    #[test]
    fn test_get_u64_arg_present() {
        let args = serde_json::json!({"limit": 20});
        assert_eq!(get_u64_arg(&args, "limit", 10), 20);
    }

    #[test]
    fn test_get_u64_arg_missing_uses_default() {
        let args = serde_json::json!({});
        assert_eq!(get_u64_arg(&args, "limit", 10), 10);
    }

    #[test]
    fn test_get_u64_arg_wrong_type_uses_default() {
        let args = serde_json::json!({"limit": "twenty"});
        assert_eq!(get_u64_arg(&args, "limit", 10), 10);
    }

    #[test]
    fn test_get_u64_arg_required_present() {
        let args = serde_json::json!({"count": 5});
        let result = get_u64_arg_required(&args, "count");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[test]
    fn test_get_u64_arg_required_missing() {
        let args = serde_json::json!({});
        let result = get_u64_arg_required(&args, "count");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing count"));
    }

    #[test]
    fn test_extract_type_repo_filters_both() {
        let args = serde_json::json!({"doc_type": "person", "repo": "notes"});
        let (doc_type, repo) = extract_type_repo_filters(&args);
        assert_eq!(doc_type.as_deref(), Some("person"));
        assert_eq!(repo.as_deref(), Some("notes"));
    }

    #[test]
    fn test_extract_type_repo_filters_none() {
        let args = serde_json::json!({});
        let (doc_type, repo) = extract_type_repo_filters(&args);
        assert!(doc_type.is_none());
        assert!(repo.is_none());
    }

    #[test]
    fn test_get_bool_arg_present_true() {
        let args = serde_json::json!({"flag": true});
        assert!(get_bool_arg(&args, "flag", false));
    }

    #[test]
    fn test_get_bool_arg_present_false() {
        let args = serde_json::json!({"flag": false});
        assert!(!get_bool_arg(&args, "flag", true));
    }

    #[test]
    fn test_get_bool_arg_missing_uses_default() {
        let args = serde_json::json!({});
        assert!(get_bool_arg(&args, "flag", true));
        assert!(!get_bool_arg(&args, "flag", false));
    }

    #[test]
    fn test_get_bool_arg_wrong_type_uses_default() {
        let args = serde_json::json!({"flag": "yes"});
        assert!(get_bool_arg(&args, "flag", true));
    }
}
