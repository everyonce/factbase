use chrono::{DateTime, Utc};
use factbase::models::{DetailedStats, PoolStats, RepoStats, Repository, SourceStats, TemporalStats};

/// Format repository status as JSON
pub fn format_repo_status_json(
    repo: &Repository,
    stats: &RepoStats,
    detailed: Option<&DetailedStats>,
    pool_stats: Option<&PoolStats>,
    temporal_stats: Option<&TemporalStats>,
    source_stats: Option<&SourceStats>,
    since: Option<&DateTime<Utc>>,
) -> serde_json::Value {
    let mut output = serde_json::json!({
        "id": repo.id,
        "name": repo.name,
        "path": repo.path,
        "active": stats.active,
        "deleted": stats.deleted,
        "by_type": stats.by_type,
    });
    if let Some(since_dt) = since {
        output["since"] = serde_json::json!(since_dt.to_rfc3339());
    }
    if let Some(d) = detailed {
        add_detailed_json(&mut output, d, pool_stats, temporal_stats, source_stats);
    }
    output
}

fn add_detailed_json(
    output: &mut serde_json::Value,
    d: &DetailedStats,
    pool_stats: Option<&PoolStats>,
    temporal_stats: Option<&TemporalStats>,
    source_stats: Option<&SourceStats>,
) {
    output["avg_doc_size"] = serde_json::json!(d.avg_doc_size);
    output["total_words"] = serde_json::json!(d.total_words);
    output["avg_words_per_doc"] = serde_json::json!(d.avg_words_per_doc);
    output["most_linked"] = serde_json::json!(d.most_linked.iter()
        .map(|(id, title, count)| serde_json::json!({"id": id, "title": title, "incoming_links": count}))
        .collect::<Vec<_>>());
    output["orphans"] = serde_json::json!(d
        .orphans
        .iter()
        .map(|(id, title)| serde_json::json!({"id": id, "title": title}))
        .collect::<Vec<_>>());
    if let Some((id, title, date)) = &d.oldest_doc {
        output["oldest_doc"] =
            serde_json::json!({"id": id, "title": title, "date": date.to_rfc3339()});
    }
    if let Some((id, title, date)) = &d.newest_doc {
        output["newest_doc"] =
            serde_json::json!({"id": id, "title": title, "date": date.to_rfc3339()});
    }
    if let Some(cs) = &d.compression_stats {
        output["compression"] = serde_json::json!({
            "compressed_docs": cs.compressed_docs,
            "total_docs": cs.total_docs,
            "compressed_size": cs.compressed_size,
            "original_size": cs.original_size,
            "savings_percent": cs.savings_percent,
        });
    }
    if let Some(ps) = pool_stats {
        output["pool"] = serde_json::json!({
            "connections": ps.connections,
            "idle_connections": ps.idle_connections,
            "max_size": ps.max_size,
        });
    }
    if let Some(ts) = temporal_stats {
        output["temporal"] = serde_json::json!({
            "total_facts": ts.total_facts,
            "facts_with_tags": ts.facts_with_tags,
            "coverage_percent": ts.coverage_percent,
            "by_type": ts.by_type,
            "oldest_date": ts.oldest_date,
            "newest_date": ts.newest_date,
        });
    }
    if let Some(ss) = source_stats {
        output["sources"] = serde_json::json!({
            "total_facts": ss.total_facts,
            "facts_with_sources": ss.facts_with_sources,
            "coverage_percent": ss.coverage_percent,
            "by_type": ss.by_type,
            "oldest_source_date": ss.oldest_source_date,
            "newest_source_date": ss.newest_source_date,
            "orphan_references": ss.orphan_references,
            "orphan_definitions": ss.orphan_definitions,
        });
    }
}
