//! get_fact_pairs MCP tool — returns embedding-similar fact pairs for agent classification.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::mcp::tools::{get_str_arg, resolve_repo_filter};
use crate::processor::parse_review_queue;
use serde_json::Value;
use std::collections::HashSet;

/// Returns similar fact pairs across documents for agent-driven cross-validation.
///
/// Queries pre-computed fact embeddings, enriches with doc titles, and
/// excludes pairs where a cross-check review question already exists.
pub fn get_fact_pairs(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let repo_id = repo_id.as_deref();

    let config = crate::Config::load(None);
    let default_threshold = config
        .as_ref()
        .map(|c| c.cross_validate.fact_similarity_threshold)
        .unwrap_or(0.5);

    let min_similarity = args
        .get("min_similarity")
        .and_then(Value::as_f64)
        .unwrap_or(default_threshold as f64) as f32;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;

    // Resolve repo-local DB for fact embeddings
    let fact_db = repo_id.and_then(|id| db.resolve_repo_fact_db(id));
    let fdb = fact_db.as_ref().unwrap_or(db);

    let fact_count = fdb.get_fact_embedding_count().unwrap_or(0);
    if fact_count == 0 {
        return Ok(serde_json::json!({
            "pairs": [],
            "total_fact_embeddings": 0,
            "message": "No fact embeddings found. Run scan_repository to generate document and fact-level embeddings."
        }));
    }

    let all_pairs = fdb
        .find_all_cross_doc_fact_pairs(min_similarity, 5, repo_id)
        .unwrap_or_default();

    // Build set of (doc_id, line) that already have cross-check review questions
    let reviewed_lines = build_reviewed_lines(db, &all_pairs);

    // Filter out already-reviewed pairs and enrich with doc titles
    let mut doc_title_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut pairs_json: Vec<Value> = Vec::new();

    for pair in &all_pairs {
        // Skip if either fact's line already has a cross-check question
        let key_a = (&pair.fact_a.document_id, pair.fact_a.line_number);
        let key_b = (&pair.fact_b.document_id, pair.fact_b.line_number);
        if reviewed_lines.contains(&(key_a.0.as_str(), key_a.1))
            || reviewed_lines.contains(&(key_b.0.as_str(), key_b.1))
        {
            continue;
        }

        let title_a = get_doc_title(db, &pair.fact_a.document_id, &mut doc_title_cache);
        let title_b = get_doc_title(db, &pair.fact_b.document_id, &mut doc_title_cache);

        pairs_json.push(serde_json::json!({
            "fact_a": {
                "doc_id": pair.fact_a.document_id,
                "doc_title": title_a,
                "text": pair.fact_a.fact_text,
                "line": pair.fact_a.line_number,
            },
            "fact_b": {
                "doc_id": pair.fact_b.document_id,
                "doc_title": title_b,
                "text": pair.fact_b.fact_text,
                "line": pair.fact_b.line_number,
            },
            "similarity": (pair.similarity * 1000.0).round() / 1000.0,
        }));

        if pairs_json.len() >= limit {
            break;
        }
    }

    // Compute stats
    let mut docs_involved: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut min_sim = f32::MAX;
    let mut max_sim = f32::MIN;
    for p in &pairs_json {
        if let Some(s) = p.get("similarity").and_then(|v| v.as_f64()) {
            let s = s as f32;
            if s < min_sim {
                min_sim = s;
            }
            if s > max_sim {
                max_sim = s;
            }
        }
        if let Some(id) = p.pointer("/fact_a/doc_id").and_then(|v| v.as_str()) {
            docs_involved.insert(id);
        }
        if let Some(id) = p.pointer("/fact_b/doc_id").and_then(|v| v.as_str()) {
            docs_involved.insert(id);
        }
    }

    let similarity_range = if pairs_json.is_empty() {
        Value::Null
    } else {
        serde_json::json!({
            "min": (min_sim * 1000.0).round() / 1000.0,
            "max": (max_sim * 1000.0).round() / 1000.0,
        })
    };

    Ok(serde_json::json!({
        "pairs": pairs_json,
        "total_fact_embeddings": fact_count,
        "total_pairs_above_threshold": all_pairs.len(),
        "pairs_returned": pairs_json.len(),
        "docs_involved": docs_involved.len(),
        "similarity_range": similarity_range,
    }))
}

/// Build a set of (doc_id, line_number) that already have cross-check review questions.
fn build_reviewed_lines<'a>(
    db: &Database,
    pairs: &'a [crate::models::FactPair],
) -> HashSet<(&'a str, usize)> {
    let mut result = HashSet::new();

    // Collect unique doc IDs from pairs
    let doc_ids: HashSet<&str> = pairs
        .iter()
        .flat_map(|p| [p.fact_a.document_id.as_str(), p.fact_b.document_id.as_str()])
        .collect();

    for doc_id in doc_ids {
        let doc = match db.get_document(doc_id) {
            Ok(Some(d)) => d,
            _ => continue,
        };
        let questions = parse_review_queue(&doc.content).unwrap_or_default();
        for q in &questions {
            if let Some(line) = q.line_ref {
                if q.description.starts_with("Cross-check") {
                    result.insert((doc_id, line));
                }
            }
        }
    }

    result
}

fn get_doc_title(
    db: &Database,
    doc_id: &str,
    cache: &mut std::collections::HashMap<String, String>,
) -> String {
    if let Some(title) = cache.get(doc_id) {
        return title.clone();
    }
    let title = db
        .get_document(doc_id)
        .ok()
        .flatten()
        .map(|d| d.title)
        .unwrap_or_else(|| doc_id.to_string());
    cache.insert(doc_id.to_string(), title.clone());
    title
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::{near_spike, spike_embedding};

    #[test]
    fn test_get_fact_pairs_empty_db() {
        let (db, _tmp) = test_db();
        let repo = crate::database::tests::test_repo();
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({});
        let result = get_fact_pairs(&db, &args).unwrap();
        assert_eq!(result["pairs"].as_array().unwrap().len(), 0);
        assert_eq!(result["total_fact_embeddings"], 0);
    }

    #[test]
    fn test_get_fact_pairs_returns_similar_pairs() {
        let (db, _tmp) = test_db();
        let repo = crate::database::tests::test_repo();
        db.upsert_repository(&repo).unwrap();

        // Create two docs using the standard test helpers
        let mut doc_a = crate::database::tests::test_doc("fp1111", "Entity A");
        doc_a.content = "<!-- factbase:fp1111 -->\n# Entity A\n\n- Revenue: $10M\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = crate::database::tests::test_doc("fp2222", "Entity B");
        doc_b.content = "<!-- factbase:fp2222 -->\n# Entity B\n\n- Revenue: $50M\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        // Insert similar fact embeddings
        db.upsert_fact_embedding(
            "fp1111_4",
            "fp1111",
            4,
            "Revenue: $10M",
            "h1",
            &spike_embedding(0),
        )
        .unwrap();
        db.upsert_fact_embedding(
            "fp2222_4",
            "fp2222",
            4,
            "Revenue: $50M",
            "h2",
            &near_spike(0, 0.1),
        )
        .unwrap();

        let args = serde_json::json!({"min_similarity": 0.3});
        let result = get_fact_pairs(&db, &args).unwrap();
        let pairs = result["pairs"].as_array().unwrap();
        assert!(!pairs.is_empty(), "should find similar fact pairs");
        assert!(pairs[0]["fact_a"]["doc_title"].is_string());
        assert!(pairs[0]["similarity"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn test_get_fact_pairs_excludes_reviewed() {
        let (db, _tmp) = test_db();
        let repo = crate::database::tests::test_repo();
        db.upsert_repository(&repo).unwrap();

        // Doc A has a cross-check question at line 4
        let mut doc_a = crate::database::tests::test_doc("rv1111", "Entity A");
        doc_a.content = "<!-- factbase:rv1111 -->\n# Entity A\n\n- Revenue: $10M\n\n<!-- factbase:review -->\n- [ ] `@q[conflict]` Line 4: Cross-check with Entity B: Revenue mismatch\n".to_string();
        db.upsert_document(&doc_a).unwrap();

        let mut doc_b = crate::database::tests::test_doc("rv2222", "Entity B");
        doc_b.content = "<!-- factbase:rv2222 -->\n# Entity B\n\n- Revenue: $50M\n".to_string();
        db.upsert_document(&doc_b).unwrap();

        // Insert similar fact embeddings at line 4
        db.upsert_fact_embedding(
            "rv1111_4",
            "rv1111",
            4,
            "Revenue: $10M",
            "h1",
            &spike_embedding(0),
        )
        .unwrap();
        db.upsert_fact_embedding(
            "rv2222_4",
            "rv2222",
            4,
            "Revenue: $50M",
            "h2",
            &near_spike(0, 0.1),
        )
        .unwrap();

        let args = serde_json::json!({"min_similarity": 0.3});
        let result = get_fact_pairs(&db, &args).unwrap();
        let pairs = result["pairs"].as_array().unwrap();
        assert_eq!(
            pairs.len(),
            0,
            "should exclude pairs with existing cross-check questions"
        );
    }

    #[test]
    fn test_get_fact_pairs_respects_limit() {
        let (db, _tmp) = test_db();

        let args = serde_json::json!({"limit": 5});
        let result = get_fact_pairs(&db, &args).unwrap();
        assert!(result["pairs"].as_array().unwrap().len() <= 5);
    }
}
