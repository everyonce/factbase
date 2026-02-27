//! Helper functions for MCP tool argument extraction.
//!
//! These functions provide consistent argument parsing across all MCP tools.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::processor::{
    calculate_fact_stats, count_facts_with_sources, parse_review_queue, parse_source_definitions,
    parse_source_references, parse_temporal_tags,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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

/// Build a compact quality stats object for a single document.
/// Returns temporal coverage, source coverage, link counts, review queue, and an attention score.
pub(crate) fn build_quality_stats(
    content: &str,
    outgoing_links: usize,
    incoming_links: usize,
) -> Value {
    let fact_stats = calculate_fact_stats(content);
    let facts_with_sources = count_facts_with_sources(content);
    let review_queue = parse_review_queue(content);
    let (total_q, answered_q) = match &review_queue {
        Some(qs) => (qs.len(), qs.iter().filter(|q| q.answered).count()),
        None => (0, 0),
    };
    let pending_questions = total_q - answered_q;
    let facts_without_temporal = fact_stats.total_facts.saturating_sub(fact_stats.facts_with_tags);
    let facts_without_sources = fact_stats.total_facts.saturating_sub(facts_with_sources);
    let attention_score = pending_questions * 2 + facts_without_sources + facts_without_temporal;

    serde_json::json!({
        "temporal_coverage_pct": fact_stats.coverage,
        "facts_with_dates": fact_stats.facts_with_tags,
        "source_coverage_pct": if fact_stats.total_facts > 0 {
            (facts_with_sources as f32 / fact_stats.total_facts as f32 * 100.0).round()
        } else { 0.0 },
        "facts_with_sources": facts_with_sources,
        "total_facts": fact_stats.total_facts,
        "links": { "outgoing": outgoing_links, "incoming": incoming_links },
        "pending_questions": pending_questions,
        "attention_score": attention_score
    })
}

/// Detect whether a document title appears to be a weak/incomplete identifier.
///
/// Checks the document's own content (first paragraph after the heading) and
/// incoming link contexts for a longer, more canonical form of the entity name.
///
/// Returns the suggested better name if one is found.
pub(crate) fn detect_weak_identification(
    title: &str,
    content: &str,
    incoming_contexts: &[&str],
) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    // Matches sequences of 2+ capitalized words (proper noun phrases)
    static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[A-Z][a-zA-Z.'\-]*(?:\s+[A-Z][a-zA-Z.'\-]*)+")
            .expect("name regex should be valid")
    });

    let title_lower = title.to_lowercase();
    // For single-word titles (e.g. "pvn", "DS"), treat the whole title as the
    // initials string. For multi-word titles, extract first letter of each word.
    let title_initials: String = if !title.contains(char::is_whitespace) {
        title_lower.clone()
    } else {
        title
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .collect::<String>()
            .to_lowercase()
    };

    let mut best: Option<String> = None;
    let mut best_len = title.len();

    // Extract intro text: first ~500 chars after the H1 heading line
    let intro = content
        .lines()
        .skip_while(|l| !l.starts_with("# "))
        .skip(1) // skip the H1 itself
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");

    // Collect candidate phrases from intro and link contexts
    let sources: Vec<&str> = std::iter::once(intro.as_str())
        .chain(incoming_contexts.iter().copied())
        .collect();

    for text in &sources {
        for m in NAME_RE.find_iter(text) {
            let candidate = m.as_str();
            let cand_lower = candidate.to_lowercase();
            if cand_lower == title_lower || candidate.len() <= best_len {
                continue;
            }
            // Check: title is a substring of candidate
            let is_substring = cand_lower.contains(&title_lower);
            // Check: title matches initials of candidate
            let cand_initials: String = candidate
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .collect::<String>()
                .to_lowercase();
            let is_initials =
                title_initials.len() >= 2 && cand_initials == title_initials;

            if is_substring || is_initials {
                best = Some(candidate.to_string());
                best_len = candidate.len();
            }
        }
    }

    best
}

/// Resolve the effective time budget in seconds from tool args and config.
///
/// Priority: per-call `time_budget_secs` arg > config `server.time_budget_secs`.
/// Returns `None` for CLI-originated calls (no MCP config context).
pub(crate) fn resolve_time_budget(args: &Value) -> Option<u64> {
    // Per-call override takes priority
    if let Some(v) = args.get("time_budget_secs").and_then(Value::as_u64) {
        return Some(v.clamp(5, 60));
    }
    // Fall back to config
    let config = crate::Config::load(None).unwrap_or_default();
    Some(config.server.time_budget_secs)
}

/// Load perspective for a repository (first repo if repo_id is None).
pub(crate) fn load_perspective(
    db: &Database,
    repo_id: Option<&str>,
) -> Option<crate::models::Perspective> {
    let repos = db.list_repositories().ok()?;
    let repo = if let Some(id) = repo_id {
        repos.into_iter().find(|r| r.id == id)
    } else {
        repos.into_iter().next()
    };
    repo.and_then(|r| r.perspective)
}

/// Resolve a document's absolute file path by joining the repository root with
/// the document's relative `file_path`.
///
/// This ensures all MCP tools read/write the same physical file regardless of
/// the process working directory.
pub(crate) fn resolve_doc_path(
    db: &Database,
    doc: &crate::models::Document,
) -> Result<PathBuf, FactbaseError> {
    let repo = db.get_repository(&doc.repo_id)?.ok_or_else(|| {
        FactbaseError::not_found(format!(
            "Repository '{}' not found for document {}",
            doc.repo_id, doc.id
        ))
    })?;
    Ok(repo.path.join(&doc.file_path))
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

    // --- detect_weak_identification tests ---

    #[test]
    fn test_weak_id_title_is_substring_of_longer_form_in_content() {
        let title = "ACME";
        let content = "# ACME\n\nACME Corporation was founded in 1990.";
        let result = detect_weak_identification(title, content, &[]);
        assert_eq!(result.as_deref(), Some("ACME Corporation"));
    }

    #[test]
    fn test_weak_id_initials_match_in_content() {
        let title = "pvn";
        let content = "# pvn\n\nPrasad V. Narasimhan is the CEO.";
        let result = detect_weak_identification(title, content, &[]);
        assert_eq!(result.as_deref(), Some("Prasad V. Narasimhan"));
    }

    #[test]
    fn test_weak_id_from_link_context() {
        let title = "ACME";
        let content = "# ACME\n\nSome content here.";
        let contexts = ["worked at ACME Corporation for years"];
        let result = detect_weak_identification(title, content, &contexts);
        assert_eq!(result.as_deref(), Some("ACME Corporation"));
    }

    #[test]
    fn test_weak_id_no_match_when_title_is_already_full() {
        let title = "Prasad V. Narasimhan";
        let content = "# Prasad V. Narasimhan\n\nSenior engineer.";
        let result = detect_weak_identification(title, content, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_weak_id_no_match_for_unrelated_names() {
        let title = "jsmith";
        let content = "# jsmith\n\nSome notes about this entity.";
        let contexts = ["Alice Johnson mentioned the project"];
        let result = detect_weak_identification(title, content, &contexts);
        assert!(result.is_none());
    }

    #[test]
    fn test_weak_id_picks_longest_candidate() {
        let title = "DS";
        let content = "# DS\n\nDesert Storm was a military operation. Operation Desert Storm began in 1991.";
        let result = detect_weak_identification(title, content, &[]);
        // "DS" matches initials of "Desert Storm" but not "Operation Desert Storm" (ODS)
        assert_eq!(result.as_deref(), Some("Desert Storm"));
    }

    #[test]
    fn test_weak_id_case_insensitive_substring() {
        let title = "acme";
        let content = "# acme\n\nAcme Industries is a manufacturer.";
        let result = detect_weak_identification(title, content, &[]);
        assert_eq!(result.as_deref(), Some("Acme Industries"));
    }

    #[test]
    fn test_weak_id_empty_contexts() {
        let title = "Test";
        let content = "# Test\n\nJust a test document.";
        let result = detect_weak_identification(title, content, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_time_budget_from_args() {
        let args = serde_json::json!({"time_budget_secs": 30});
        assert_eq!(resolve_time_budget(&args), Some(30));
    }

    #[test]
    fn test_resolve_time_budget_clamps_to_range() {
        let args = serde_json::json!({"time_budget_secs": 1});
        assert_eq!(resolve_time_budget(&args), Some(5));
        let args = serde_json::json!({"time_budget_secs": 999});
        assert_eq!(resolve_time_budget(&args), Some(60));
    }

    #[test]
    fn test_resolve_time_budget_falls_back_to_config() {
        let args = serde_json::json!({});
        // Should return the config default (10)
        let budget = resolve_time_budget(&args);
        assert!(budget.is_some());
        assert_eq!(budget.unwrap(), 10);
    }
}
