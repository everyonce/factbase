//! Link suggestion and storage MCP tools.

use std::collections::{HashMap, HashSet};

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::processor::{append_links_to_content, parse_links_block};
use serde_json::Value;

use super::helpers::{get_str_arg, get_u64_arg, resolve_repo_filter, run_blocking};

/// Get link suggestions: documents with few links paired with embedding-similar candidates.
pub async fn get_link_suggestions<E: EmbeddingProvider>(
    db: &Database,
    embedding: &E,
    args: &Value,
) -> Result<Value, FactbaseError> {
    let repo = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let min_similarity = args
        .get("min_similarity")
        .and_then(Value::as_f64)
        .unwrap_or(0.6) as f32;
    let max_existing_links = get_u64_arg(args, "max_existing_links", 2) as usize;
    let limit = get_u64_arg(args, "limit", 50) as usize;

    // Get link counts for all docs
    let db2 = db.clone();
    let repo2 = repo.clone();
    let link_counts = run_blocking(move || {
        db2.get_document_link_counts(repo2.as_deref())
    })
    .await?;

    // Filter to docs with few links
    let candidates: Vec<&(String, String, usize)> = link_counts
        .iter()
        .filter(|(_, _, count)| *count <= max_existing_links)
        .collect();

    // For each candidate, find similar documents not already linked
    let mut suggestions = Vec::new();
    let _ = embedding; // embedding provider available but we use DB-level similarity

    for (doc_id, doc_title, link_count) in &candidates {
        if suggestions.len() >= limit {
            break;
        }

        let db3 = db.clone();
        let did = doc_id.clone();
        let threshold = min_similarity;
        let similar = match run_blocking(move || {
            db3.find_similar_documents(&did, threshold)
        })
        .await
        {
            Ok(s) => s,
            Err(_) => continue, // skip if no embedding exists
        };

        if similar.is_empty() {
            continue;
        }

        // Get existing link targets for this doc
        let db4 = db.clone();
        let did2 = doc_id.clone();
        let existing_links: HashSet<String> = run_blocking(move || {
            Ok(db4.get_links_from(&did2)?.into_iter().map(|l| l.target_id).collect())
        })
        .await?;

        let unlinked: Vec<Value> = similar
            .into_iter()
            .filter(|(sid, _, _)| !existing_links.contains(sid))
            .take(5)
            .map(|(id, title, sim)| {
                let rounded = (sim * 1000.0_f32).round() / 1000.0_f32;
                serde_json::json!({
                    "id": id,
                    "title": title,
                    "similarity": rounded
                })
            })
            .collect();

        if !unlinked.is_empty() {
            suggestions.push(serde_json::json!({
                "doc_id": doc_id,
                "doc_title": doc_title,
                "link_count": link_count,
                "candidates": unlinked
            }));
        }
    }

    // Compute stats
    let docs_analyzed = candidates.len();
    let avg_similarity = if suggestions.is_empty() {
        0.0
    } else {
        let total_sim: f64 = suggestions.iter().filter_map(|s| {
            s.get("candidates").and_then(|c| c.as_array()).map(|arr| {
                arr.iter().filter_map(|c| c.get("similarity").and_then(|v| v.as_f64())).sum::<f64>()
                    / arr.len().max(1) as f64
            })
        }).sum();
        ((total_sim / suggestions.len() as f64) * 1000.0).round() / 1000.0
    };

    Ok(serde_json::json!({
        "suggestions": suggestions,
        "total": suggestions.len(),
        "docs_analyzed": docs_analyzed,
        "avg_similarity": avg_similarity,
    }))
}

/// Store links by writing [[id]] references into document files' Links: blocks.
pub fn store_links(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let links = args
        .get("links")
        .and_then(Value::as_array)
        .ok_or_else(|| FactbaseError::Parse("'links' array is required".into()))?;

    // Group by source_id
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    for link in links {
        let source_id = link
            .get("source_id")
            .and_then(Value::as_str)
            .ok_or_else(|| FactbaseError::Parse("each link needs 'source_id'".into()))?;
        let target_id = link
            .get("target_id")
            .and_then(Value::as_str)
            .ok_or_else(|| FactbaseError::Parse("each link needs 'target_id'".into()))?;
        grouped
            .entry(source_id.to_string())
            .or_default()
            .push(target_id.to_string());
    }

    let mut added = 0usize;
    let mut skipped_existing = 0usize;
    let mut documents_modified = 0usize;

    for (source_id, target_ids) in &grouped {
        let doc = db
            .get_document(source_id)?
            .ok_or_else(|| FactbaseError::NotFound(format!("Document {source_id} not found")))?;

        let file_path = super::helpers::resolve_doc_path(db, &doc)?;
        if !file_path.exists() {
            return Err(FactbaseError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {}", file_path.display()),
            )));
        }

        let content = std::fs::read_to_string(&file_path)?;
        let existing_ids: HashSet<String> = parse_links_block(&content).into_iter().collect();

        let new_ids: Vec<&str> = target_ids
            .iter()
            .filter(|id| !existing_ids.contains(id.as_str()))
            .map(String::as_str)
            .collect();

        skipped_existing += target_ids.len() - new_ids.len();

        if new_ids.is_empty() {
            continue;
        }

        let updated_content = append_links_to_content(&content, &new_ids);
        std::fs::write(file_path, &updated_content)?;

        // Update DB links
        let target_refs: Vec<&str> = new_ids.iter().copied().collect();
        let db_added = db.add_links(source_id, &target_refs)?;
        added += db_added;
        documents_modified += 1;
    }

    Ok(serde_json::json!({
        "added": added,
        "skipped_existing": skipped_existing,
        "documents_modified": documents_modified
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo};

    #[test]
    fn test_get_document_link_counts() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let counts = db.get_document_link_counts(Some("test-repo")).unwrap();
        assert_eq!(counts.len(), 2);
        // All should have 0 links
        for (_, _, count) in &counts {
            assert_eq!(*count, 0);
        }
    }

    #[test]
    fn test_get_document_link_counts_with_links() {
        use crate::link_detection::DetectedLink;

        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        db.update_links(
            "doc1",
            &[DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "".to_string(),
            }],
        )
        .unwrap();

        let counts = db.get_document_link_counts(Some("test-repo")).unwrap();
        let doc1_count = counts.iter().find(|(id, _, _)| id == "doc1").unwrap().2;
        let doc2_count = counts.iter().find(|(id, _, _)| id == "doc2").unwrap().2;
        assert_eq!(doc1_count, 1);
        assert_eq!(doc2_count, 0);
    }

    #[test]
    fn test_add_links() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();
        db.upsert_document(&test_doc("doc3", "Doc 3")).unwrap();

        let added = db.add_links("doc1", &["doc2", "doc3"]).unwrap();
        assert_eq!(added, 2);

        let links = db.get_links_from("doc1").unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_add_links_skips_existing() {
        use crate::link_detection::DetectedLink;

        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        db.update_links(
            "doc1",
            &[DetectedLink {
                target_id: "doc2".to_string(),
                target_title: "Doc 2".to_string(),
                mention_text: "Doc 2".to_string(),
                context: "".to_string(),
            }],
        )
        .unwrap();

        // Adding doc2 again should be skipped
        let added = db.add_links("doc1", &["doc2"]).unwrap();
        assert_eq!(added, 0);

        let links = db.get_links_from("doc1").unwrap();
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_store_links_missing_links_array() {
        let (db, _tmp) = test_db();
        let args = serde_json::json!({});
        let result = store_links(&db, &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_links_document_not_found() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({
            "links": [{"source_id": "nonexist", "target_id": "abc123"}]
        });
        let result = store_links(&db, &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_links_writes_file() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let tmp_dir = tempfile::TempDir::new().unwrap();
        let file_path = tmp_dir.path().join("doc1.md");
        std::fs::write(&file_path, "<!-- factbase:doc001 -->\n# Doc 1\n\nContent.").unwrap();

        let mut doc = test_doc("doc001", "Doc 1");
        doc.file_path = file_path.to_string_lossy().to_string();
        db.upsert_document(&doc).unwrap();

        db.upsert_document(&test_doc("abc123", "Target Doc")).unwrap();

        let args = serde_json::json!({
            "links": [{"source_id": "doc001", "target_id": "abc123"}]
        });
        let result = store_links(&db, &args).unwrap();
        assert_eq!(result["added"], 1);
        assert_eq!(result["documents_modified"], 1);

        // Verify file was updated
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Links: [[abc123]]"));
    }

    #[test]
    fn test_store_links_skips_existing_in_file() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let tmp_dir = tempfile::TempDir::new().unwrap();
        let file_path = tmp_dir.path().join("doc1.md");
        std::fs::write(
            &file_path,
            "<!-- factbase:doc001 -->\n# Doc 1\n\nContent.\n\nLinks: [[abc123]]",
        )
        .unwrap();

        let mut doc = test_doc("doc001", "Doc 1");
        doc.file_path = file_path.to_string_lossy().to_string();
        db.upsert_document(&doc).unwrap();
        db.upsert_document(&test_doc("abc123", "Target A")).unwrap();
        db.upsert_document(&test_doc("def456", "Target B")).unwrap();

        let args = serde_json::json!({
            "links": [
                {"source_id": "doc001", "target_id": "abc123"},
                {"source_id": "doc001", "target_id": "def456"}
            ]
        });
        let result = store_links(&db, &args).unwrap();
        assert_eq!(result["added"], 1);
        assert_eq!(result["skipped_existing"], 1);
        assert_eq!(result["documents_modified"], 1);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("[[abc123]]"));
        assert!(content.contains("[[def456]]"));
    }
}
