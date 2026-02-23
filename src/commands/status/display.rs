use chrono::{DateTime, Utc};
use factbase::{
    format_bytes, DetailedStats, PoolStats, RepoStats, Repository, SourceStats, TemporalStats,
};

use super::format_coverage;

/// Print repository status in text format
pub fn print_repo_status_text(
    repo: &Repository,
    stats: &RepoStats,
    detailed: Option<&DetailedStats>,
    pool_stats: Option<&PoolStats>,
    temporal_stats: Option<&TemporalStats>,
    source_stats: Option<&SourceStats>,
    since: Option<&DateTime<Utc>>,
) {
    println!("Repository: {} ({})", repo.name, repo.id);
    println!("Path: {}", repo.path.display());
    if let Some(since_dt) = since {
        println!(
            "Documents (since {}): {} active",
            since_dt.format("%Y-%m-%d %H:%M"),
            stats.active
        );
    } else {
        println!(
            "Documents: {} active, {} deleted",
            stats.active, stats.deleted
        );
    }
    if !stats.by_type.is_empty() {
        println!("By type:");
        for (t, c) in &stats.by_type {
            println!("  {}: {}", t, c);
        }
    }
    if let Some(d) = detailed {
        print_detailed_stats(d, pool_stats, temporal_stats, source_stats);
    }
}

fn print_detailed_stats(
    d: &DetailedStats,
    pool_stats: Option<&PoolStats>,
    temporal_stats: Option<&TemporalStats>,
    source_stats: Option<&SourceStats>,
) {
    println!();
    println!(
        "Average document size: {}",
        format_bytes(d.avg_doc_size as u64)
    );
    println!(
        "Total words: {} ({} avg per doc)",
        d.total_words, d.avg_words_per_doc
    );
    if let Some((id, title, date)) = &d.oldest_doc {
        println!(
            "Oldest document: {} ({}) - {}",
            title,
            id,
            date.format("%Y-%m-%d")
        );
    }
    if let Some((id, title, date)) = &d.newest_doc {
        println!(
            "Newest document: {} ({}) - {}",
            title,
            id,
            date.format("%Y-%m-%d")
        );
    }
    if let Some(cs) = &d.compression_stats {
        println!();
        println!("Compression:");
        println!(
            "  {} of {} documents compressed",
            cs.compressed_docs, cs.total_docs
        );
        println!(
            "  Storage: {} bytes (original: {} bytes)",
            cs.compressed_size, cs.original_size
        );
        println!("  Space saved: {:.1}%", cs.savings_percent);
    }
    if let Some(ps) = pool_stats {
        println!();
        println!("Connection pool:");
        println!(
            "  Active: {}, Idle: {}, Max: {}",
            ps.connections, ps.idle_connections, ps.max_size
        );
    }
    if let Some(ts) = temporal_stats {
        print_temporal_stats(ts);
    }
    if let Some(ss) = source_stats {
        print_source_stats(ss);
    }
    if !d.most_linked.is_empty() {
        println!();
        println!("Most linked documents:");
        for (id, title, count) in &d.most_linked {
            println!("  {} ({}) - {} incoming links", title, id, count);
        }
    }
    if !d.orphans.is_empty() {
        println!();
        println!("Orphan documents (no links):");
        for (id, title) in &d.orphans {
            println!("  {} ({})", title, id);
        }
    }
}

fn print_temporal_stats(ts: &TemporalStats) {
    print!("{}", format_temporal_stats(ts));
}

/// Format temporal stats as a multi-line string
fn format_temporal_stats(ts: &TemporalStats) -> String {
    let mut lines = Vec::new();
    lines.push(String::new()); // blank line
    lines.push(format!(
        "Temporal: {} coverage ({}/{} facts)",
        format_coverage(ts.coverage_percent),
        ts.facts_with_tags,
        ts.total_facts
    ));
    if !ts.by_type.is_empty() {
        let types: Vec<_> = ts
            .by_type
            .iter()
            .map(|(t, c)| format!("{}: {}", t, c))
            .collect();
        lines.push(format!("  Tag types: {}", types.join(", ")));
    }
    if let Some(oldest) = &ts.oldest_date {
        if let Some(newest) = &ts.newest_date {
            lines.push(format!("  Date range: {} to {}", oldest, newest));
        } else {
            lines.push(format!("  Date range: {}", oldest));
        }
    }
    lines.join("\n") + "\n"
}

fn print_source_stats(ss: &SourceStats) {
    print!("{}", format_source_stats(ss));
}

/// Format source stats as a multi-line string
fn format_source_stats(ss: &SourceStats) -> String {
    let mut lines = Vec::new();
    lines.push(String::new()); // blank line
    lines.push(format!(
        "Sources: {} coverage ({}/{} facts)",
        format_coverage(ss.coverage_percent),
        ss.facts_with_sources,
        ss.total_facts
    ));
    if ss.orphan_references > 0 || ss.orphan_definitions > 0 {
        lines.push(format!(
            "  Orphans: {} refs, {} defs",
            ss.orphan_references, ss.orphan_definitions
        ));
    }
    if !ss.by_type.is_empty() {
        let types: Vec<_> = ss
            .by_type
            .iter()
            .map(|(t, c)| format!("{}: {}", t, c))
            .collect();
        lines.push(format!("  Source types: {}", types.join(", ")));
    }
    if let Some(oldest) = &ss.oldest_source_date {
        if let Some(newest) = &ss.newest_source_date {
            lines.push(format!("  Date range: {} to {}", oldest, newest));
        } else {
            lines.push(format!("  Date range: {}", oldest));
        }
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_format_temporal_stats_basic() {
        let ts = TemporalStats {
            total_facts: 10,
            facts_with_tags: 8,
            coverage_percent: 80.0,
            by_type: HashMap::new(),
            oldest_date: None,
            newest_date: None,
        };
        let output = format_temporal_stats(&ts);
        assert!(output.contains("Temporal: 80.0% coverage (8/10 facts)"));
        assert!(!output.contains("Tag types:"));
        assert!(!output.contains("Date range:"));
    }

    #[test]
    fn test_format_temporal_stats_with_types_and_dates() {
        let mut by_type = HashMap::new();
        by_type.insert("point".to_string(), 5);
        by_type.insert("range".to_string(), 3);
        let ts = TemporalStats {
            total_facts: 10,
            facts_with_tags: 8,
            coverage_percent: 80.0,
            by_type,
            oldest_date: Some("2020-01".to_string()),
            newest_date: Some("2024-06".to_string()),
        };
        let output = format_temporal_stats(&ts);
        assert!(output.contains("Tag types:"));
        assert!(output.contains("Date range: 2020-01 to 2024-06"));
    }

    #[test]
    fn test_format_temporal_stats_zero_facts() {
        let ts = TemporalStats {
            total_facts: 0,
            facts_with_tags: 0,
            coverage_percent: 0.0,
            by_type: HashMap::new(),
            oldest_date: None,
            newest_date: None,
        };
        let output = format_temporal_stats(&ts);
        assert!(output.contains("0.00% coverage (0/0 facts)"));
    }

    #[test]
    fn test_format_source_stats_basic() {
        let ss = SourceStats {
            total_facts: 20,
            facts_with_sources: 15,
            coverage_percent: 75.0,
            orphan_references: 0,
            orphan_definitions: 0,
            by_type: HashMap::new(),
            oldest_source_date: None,
            newest_source_date: None,
        };
        let output = format_source_stats(&ss);
        assert!(output.contains("Sources: 75.0% coverage (15/20 facts)"));
        assert!(!output.contains("Orphans:"));
    }

    #[test]
    fn test_format_source_stats_with_orphans() {
        let ss = SourceStats {
            total_facts: 20,
            facts_with_sources: 15,
            coverage_percent: 75.0,
            orphan_references: 2,
            orphan_definitions: 1,
            by_type: HashMap::new(),
            oldest_source_date: None,
            newest_source_date: None,
        };
        let output = format_source_stats(&ss);
        assert!(output.contains("Orphans: 2 refs, 1 defs"));
    }

    #[test]
    fn test_format_source_stats_with_types_and_dates() {
        let mut by_type = HashMap::new();
        by_type.insert("LinkedIn".to_string(), 10);
        by_type.insert("Website".to_string(), 5);
        let ss = SourceStats {
            total_facts: 20,
            facts_with_sources: 15,
            coverage_percent: 75.0,
            orphan_references: 0,
            orphan_definitions: 0,
            by_type,
            oldest_source_date: Some("2023-01-15".to_string()),
            newest_source_date: Some("2024-06-20".to_string()),
        };
        let output = format_source_stats(&ss);
        assert!(output.contains("Source types:"));
        assert!(output.contains("Date range: 2023-01-15 to 2024-06-20"));
    }
}
