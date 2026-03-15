//! Link suggestion and storage MCP tools.

use std::collections::{HashMap, HashSet};

use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::error::FactbaseError;
use crate::models::format::LinkStyle;
use crate::processor::{
    append_links_to_content, append_links_to_content_styled, append_referenced_by_to_content,
    append_referenced_by_to_content_styled, parse_links_block, parse_referenced_by_block,
};
use serde_json::Value;

use super::helpers::{
    get_str_arg, get_str_array_arg as get_str_array_arg_opt, get_u64_arg, resolve_repo_filter,
    run_blocking,
};

/// Parse a JSON string array argument, lowercasing values.
fn get_str_array_arg_lower(args: &Value, key: &str) -> Vec<String> {
    get_str_array_arg_opt(args, key)
        .map(|v| v.into_iter().map(|s| s.to_lowercase()).collect())
        .unwrap_or_default()
}

/// Get link suggestions: documents paired with embedding-similar candidates not yet linked.
/// Supports type filters to control which candidate types are suggested.
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
    let limit = get_u64_arg(args, "limit", 50) as usize;
    let include_types = get_str_array_arg_lower(args, "include_types");
    let exclude_types = get_str_array_arg_lower(args, "exclude_types");

    // Get all docs with link counts and types
    let db2 = db.clone();
    let repo2 = repo.clone();
    let all_docs = run_blocking(move || db2.get_document_link_counts(repo2.as_deref())).await?;

    // Build type lookup map
    let type_map: HashMap<String, String> = all_docs
        .iter()
        .map(|(id, _, doc_type, _)| (id.clone(), doc_type.clone()))
        .collect();

    let mut suggestions = Vec::new();
    let _ = embedding;

    for (doc_id, doc_title, doc_type, link_count) in &all_docs {
        if suggestions.len() >= limit {
            break;
        }

        let db3 = db.clone();
        let did = doc_id.clone();
        let threshold = min_similarity;
        let similar = match run_blocking(move || db3.find_similar_documents(&did, threshold)).await
        {
            Ok(s) => s,
            Err(_) => continue,
        };

        if similar.is_empty() {
            continue;
        }

        // Get existing link targets for this doc
        let db4 = db.clone();
        let did2 = doc_id.clone();
        let existing_links: HashSet<String> = run_blocking(move || {
            Ok(db4
                .get_links_from(&did2)?
                .into_iter()
                .map(|l| l.target_id)
                .collect())
        })
        .await?;

        let unlinked: Vec<Value> = similar
            .into_iter()
            .filter(|(sid, _, _)| {
                if existing_links.contains(sid) {
                    return false;
                }
                let candidate_type = type_map
                    .get(sid)
                    .map(|s| s.to_lowercase())
                    .unwrap_or_default();
                if !include_types.is_empty() && !include_types.contains(&candidate_type) {
                    return false;
                }
                if !exclude_types.is_empty() && exclude_types.contains(&candidate_type) {
                    return false;
                }
                true
            })
            .take(5)
            .map(|(id, title, sim)| {
                let rounded = (sim * 1000.0_f32).round() / 1000.0_f32;
                let ctype = type_map.get(&id).cloned().unwrap_or_default();
                serde_json::json!({
                    "id": id,
                    "title": title,
                    "type": ctype,
                    "similarity": rounded
                })
            })
            .collect();

        if !unlinked.is_empty() {
            suggestions.push(serde_json::json!({
                "doc_id": doc_id,
                "doc_title": doc_title,
                "doc_type": doc_type,
                "link_count": link_count,
                "candidates": unlinked
            }));
        }
    }

    let docs_analyzed = all_docs.len();
    let avg_similarity = if suggestions.is_empty() {
        0.0
    } else {
        let total_sim: f64 = suggestions
            .iter()
            .filter_map(|s| {
                s.get("candidates").and_then(|c| c.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("similarity").and_then(|v| v.as_f64()))
                        .sum::<f64>()
                        / arr.len().max(1) as f64
                })
            })
            .sum();
        ((total_sim / suggestions.len() as f64) * 1000.0).round() / 1000.0
    };

    Ok(serde_json::json!({
        "suggestions": suggestions,
        "total": suggestions.len(),
        "docs_analyzed": docs_analyzed,
        "avg_similarity": avg_similarity,
    }))
}

/// Resolve the link style for a document's repository.
fn resolve_link_style(db: &Database, repo_id: &str) -> LinkStyle {
    db.get_repository(repo_id)
        .ok()
        .flatten()
        .and_then(|r| r.perspective)
        .and_then(|p| p.format)
        .map(|f| f.resolve().link_style)
        .unwrap_or_default()
}

/// Store links by writing [[id]] references into document files.
/// Writes `References:` to source files and `Referenced by:` to target files.
/// Only the forward direction (source→target) is added to the database.
///
/// In wikilink mode, emits `[[folder/filename|Title]]` for Obsidian compatibility.
pub fn store_links(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let links = args
        .get("links")
        .and_then(Value::as_array)
        .ok_or_else(|| FactbaseError::Parse("'links' array is required".into()))?;

    // Group by source_id (for References:) and by target_id (for Referenced by:)
    let mut by_source: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_target: HashMap<String, Vec<String>> = HashMap::new();
    for link in links {
        let source_id = link
            .get("source_id")
            .and_then(Value::as_str)
            .ok_or_else(|| FactbaseError::Parse("each link needs 'source_id'".into()))?;
        let target_id = link
            .get("target_id")
            .and_then(Value::as_str)
            .ok_or_else(|| FactbaseError::Parse("each link needs 'target_id'".into()))?;
        by_source
            .entry(source_id.to_string())
            .or_default()
            .push(target_id.to_string());
        by_target
            .entry(target_id.to_string())
            .or_default()
            .push(source_id.to_string());
    }

    let mut added = 0usize;
    let mut skipped_existing = 0usize;
    let mut documents_modified = 0usize;

    // Write References: blocks to source documents
    for (source_id, target_ids) in &by_source {
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

        let style = resolve_link_style(db, &doc.repo_id);
        let updated_content = if style == LinkStyle::Factbase {
            append_links_to_content(&content, &new_ids)
        } else {
            // Look up target docs for title/path info
            let id_names: Vec<(&str, Option<String>, Option<String>)> = new_ids
                .iter()
                .map(|id| {
                    let (title, fp) = db
                        .get_document(id)
                        .ok()
                        .flatten()
                        .map(|d| (Some(d.title), Some(d.file_path)))
                        .unwrap_or((None, None));
                    (*id, title, fp)
                })
                .collect();
            let refs: Vec<(&str, Option<&str>, Option<&str>)> = id_names
                .iter()
                .map(|(id, t, fp)| (*id, t.as_deref(), fp.as_deref()))
                .collect();
            append_links_to_content_styled(&content, &refs, style)
        };
        std::fs::write(file_path, &updated_content)?;

        // Update DB links (forward direction only)
        let target_refs: Vec<&str> = new_ids.to_vec();
        let db_added = db.add_links(source_id, &target_refs)?;
        added += db_added;
        documents_modified += 1;
    }

    // Write Referenced by: blocks to target documents
    for (target_id, source_ids) in &by_target {
        let doc = match db.get_document(target_id)? {
            Some(d) => d,
            None => continue, // target may not exist yet
        };

        let file_path = match super::helpers::resolve_doc_path(db, &doc) {
            Ok(p) if p.exists() => p,
            _ => continue,
        };

        let content = std::fs::read_to_string(&file_path)?;
        let existing_ids: HashSet<String> =
            parse_referenced_by_block(&content).into_iter().collect();

        let new_ids: Vec<&str> = source_ids
            .iter()
            .filter(|id| !existing_ids.contains(id.as_str()))
            .map(String::as_str)
            .collect();

        if new_ids.is_empty() {
            continue;
        }

        let style = resolve_link_style(db, &doc.repo_id);
        let updated_content = if style == LinkStyle::Factbase {
            append_referenced_by_to_content(&content, &new_ids)
        } else {
            let id_names: Vec<(&str, Option<String>, Option<String>)> = new_ids
                .iter()
                .map(|id| {
                    let (title, fp) = db
                        .get_document(id)
                        .ok()
                        .flatten()
                        .map(|d| (Some(d.title), Some(d.file_path)))
                        .unwrap_or((None, None));
                    (*id, title, fp)
                })
                .collect();
            let refs: Vec<(&str, Option<&str>, Option<&str>)> = id_names
                .iter()
                .map(|(id, t, fp)| (*id, t.as_deref(), fp.as_deref()))
                .collect();
            append_referenced_by_to_content_styled(&content, &refs, style)
        };
        std::fs::write(file_path, &updated_content)?;
        documents_modified += 1;
    }

    Ok(serde_json::json!({
        "added": added,
        "skipped_existing": skipped_existing,
        "documents_modified": documents_modified
    }))
}

/// Migrate all cross-reference blocks in a repository to the repo's configured link style.
///
/// Rewrites `References:` and `Referenced by:` blocks in every document file.
/// Useful when switching a repo from factbase IDs to wikilink (obsidian) format.
#[allow(dead_code)] // Kept for backward compat testing; removed from MCP dispatch
pub fn migrate_repo_links(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = args
        .get("repo")
        .and_then(Value::as_str)
        .ok_or_else(|| FactbaseError::Parse("'repo' is required".into()))?;

    let repo = db
        .get_repository(repo_id)?
        .ok_or_else(|| FactbaseError::NotFound(format!("Repository '{repo_id}' not found")))?;

    let style = repo
        .perspective
        .as_ref()
        .and_then(|p| p.format.as_ref())
        .map(|f| f.resolve().link_style)
        .unwrap_or_default();

    let docs = db.get_documents_for_repo(repo_id)?;
    let mut migrated = 0usize;

    // Build lookup closure over all docs in repo
    let doc_map: HashMap<String, (String, String)> = docs
        .iter()
        .map(|(id, doc)| (id.clone(), (doc.title.clone(), doc.file_path.clone())))
        .collect();

    for doc in docs.values() {
        if doc.is_deleted {
            continue;
        }
        let file_path = repo.path.join(&doc.file_path);
        if !file_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&file_path)?;
        let updated =
            crate::processor::migrate_links(&content, style, |id| doc_map.get(id).cloned());

        if updated != content {
            std::fs::write(&file_path, &updated)?;
            migrated += 1;
        }
    }

    // Generate Obsidian CSS snippet if the repo uses the obsidian preset
    let css_written = if repo
        .perspective
        .as_ref()
        .and_then(|p| p.format.as_ref())
        .map(|f| f.preset.as_deref() == Some("obsidian"))
        .unwrap_or(false)
    {
        crate::models::write_obsidian_css_snippet(&repo.path).is_ok()
    } else {
        false
    };

    Ok(serde_json::json!({
        "migrated": migrated,
        "total_documents": docs.len(),
        "link_style": format!("{:?}", style).to_lowercase(),
        "obsidian_css_written": css_written
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
        for (_, _, _, count) in &counts {
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
        let doc1_count = counts.iter().find(|(id, _, _, _)| id == "doc1").unwrap().3;
        let doc2_count = counts.iter().find(|(id, _, _, _)| id == "doc2").unwrap().3;
        assert_eq!(doc1_count, 1);
        assert_eq!(doc2_count, 0);
    }

    #[test]
    fn test_get_document_link_counts_returns_doc_type() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let mut doc = test_doc("doc1", "Doc 1");
        doc.doc_type = Some("person".to_string());
        db.upsert_document(&doc).unwrap();

        let counts = db.get_document_link_counts(Some("test-repo")).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].2, "person");
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
        let src_path = tmp_dir.path().join("doc1.md");
        let tgt_path = tmp_dir.path().join("target.md");
        std::fs::write(&src_path, "---\nfactbase_id: doc001\n---\n# Doc 1\n\nContent.").unwrap();
        std::fs::write(
            &tgt_path,
            "---\nfactbase_id: abc123\n---\n# Target Doc\n\nContent.",
        )
        .unwrap();

        let mut doc = test_doc("doc001", "Doc 1");
        doc.file_path = src_path.to_string_lossy().to_string();
        db.upsert_document(&doc).unwrap();

        let mut target = test_doc("abc123", "Target Doc");
        target.file_path = tgt_path.to_string_lossy().to_string();
        db.upsert_document(&target).unwrap();

        let args = serde_json::json!({
            "links": [{"source_id": "doc001", "target_id": "abc123"}]
        });
        let result = store_links(&db, &args).unwrap();
        assert_eq!(result["added"], 1);
        assert_eq!(result["documents_modified"], 2); // both source and target

        // Verify source file has References:
        let src_content = std::fs::read_to_string(&src_path).unwrap();
        assert!(src_content.contains("References: [[abc123]]"));
        assert!(!src_content.contains("Links:"));

        // Verify target file has Referenced by:
        let tgt_content = std::fs::read_to_string(&tgt_path).unwrap();
        assert!(tgt_content.contains("Referenced by: [[doc001]]"));
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
            "---\nfactbase_id: doc001\n---\n# Doc 1\n\nContent.\n\nReferences: [[abc123]]",
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

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("[[abc123]]"));
        assert!(content.contains("[[def456]]"));
    }

    #[test]
    fn test_get_str_array_arg_present() {
        let args = serde_json::json!({"types": ["person", "Project"]});
        let result = get_str_array_arg_lower(&args, "types");
        assert_eq!(result, vec!["person", "project"]); // lowercased
    }

    #[test]
    fn test_get_str_array_arg_missing() {
        let args = serde_json::json!({});
        let result = get_str_array_arg_lower(&args, "types");
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_str_array_arg_empty() {
        let args = serde_json::json!({"types": []});
        let result = get_str_array_arg_lower(&args, "types");
        assert!(result.is_empty());
    }

    #[test]
    fn test_schema_has_type_filters() {
        let tools = crate::mcp::tools::schema::tools_list();
        let tools_arr = tools["tools"].as_array().unwrap();
        let fb = tools_arr.iter().find(|t| t["name"] == "factbase").unwrap();
        let props = fb["inputSchema"]["properties"].as_object().unwrap();
        assert!(
            props.contains_key("include_types"),
            "should have include_types"
        );
        assert!(
            props.contains_key("exclude_types"),
            "should have exclude_types"
        );
    }

    #[test]
    fn test_ingest_workflow_has_links_step() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        let args = serde_json::json!({"workflow": "ingest", "step": 5, "topic": "test"});
        let result = crate::mcp::tools::workflow::workflow(&db, &args).unwrap();
        let instr = result["instruction"].as_str().unwrap();
        assert!(
            instr.contains("factbase(op='links')"),
            "ingest step 5 should mention factbase(op='links')"
        );
        assert!(
            result["complete"].as_bool().unwrap_or(false),
            "ingest step 5 should be complete"
        );
    }

    fn test_repo_wikilink(path: &std::path::Path) -> crate::models::Repository {
        use crate::models::format::FormatConfig;
        use crate::models::Perspective;
        crate::models::Repository {
            id: "test-repo".to_string(),
            name: "Test Repo".to_string(),
            path: path.to_path_buf(),
            perspective: Some(Perspective {
                format: Some(FormatConfig {
                    preset: Some("obsidian".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        }
    }

    #[test]
    fn test_store_links_wikilink_format() {
        let (db, _tmp) = test_db();
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let repo = test_repo_wikilink(tmp_dir.path());
        db.upsert_repository(&repo).unwrap();

        // Create source and target files in subdirectories
        let people_dir = tmp_dir.path().join("people");
        std::fs::create_dir_all(&people_dir).unwrap();
        let src_path = people_dir.join("alice.md");
        let tgt_path = people_dir.join("bob.md");
        std::fs::write(&src_path, "# Alice\n\nContent.").unwrap();
        std::fs::write(&tgt_path, "# Bob\n\nContent.").unwrap();

        let mut doc1 = test_doc("aaa111", "Alice");
        doc1.file_path = "people/alice.md".to_string();
        db.upsert_document(&doc1).unwrap();

        let mut doc2 = test_doc("bbb222", "Bob");
        doc2.file_path = "people/bob.md".to_string();
        db.upsert_document(&doc2).unwrap();

        let args = serde_json::json!({
            "links": [{"source_id": "aaa111", "target_id": "bbb222"}]
        });
        let result = store_links(&db, &args).unwrap();
        assert_eq!(result["added"], 1);

        // Source should have wikilink with path
        let src_content = std::fs::read_to_string(&src_path).unwrap();
        assert!(
            src_content.contains("References: [[people/bob|Bob]]"),
            "Expected wikilink path format, got: {src_content}"
        );

        // Target should have Referenced by: with wikilink path
        let tgt_content = std::fs::read_to_string(&tgt_path).unwrap();
        assert!(
            tgt_content.contains("Referenced by: [[people/alice|Alice]]"),
            "Expected wikilink path format, got: {tgt_content}"
        );
    }

    #[test]
    fn test_store_links_wikilink_disambiguates_same_name() {
        let (db, _tmp) = test_db();
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let repo = test_repo_wikilink(tmp_dir.path());
        db.upsert_repository(&repo).unwrap();

        // Create directories
        let people_dir = tmp_dir.path().join("people");
        let books_dir = tmp_dir.path().join("books");
        let events_dir = tmp_dir.path().join("events");
        std::fs::create_dir_all(&people_dir).unwrap();
        std::fs::create_dir_all(&books_dir).unwrap();
        std::fs::create_dir_all(&events_dir).unwrap();

        let src_path = events_dir.join("conquest.md");
        let person_path = people_dir.join("joshua.md");
        let book_path = books_dir.join("joshua.md");
        std::fs::write(&src_path, "# The Conquest\n\nContent.").unwrap();
        std::fs::write(&person_path, "# Joshua\n\nA person.").unwrap();
        std::fs::write(&book_path, "# Joshua\n\nA book.").unwrap();

        let mut src = test_doc("src001", "The Conquest");
        src.file_path = "events/conquest.md".to_string();
        db.upsert_document(&src).unwrap();

        let mut person = test_doc("per001", "Joshua");
        person.file_path = "people/joshua.md".to_string();
        db.upsert_document(&person).unwrap();

        let mut book = test_doc("bok001", "Joshua");
        book.file_path = "books/joshua.md".to_string();
        db.upsert_document(&book).unwrap();

        let args = serde_json::json!({
            "links": [
                {"source_id": "src001", "target_id": "per001"},
                {"source_id": "src001", "target_id": "bok001"}
            ]
        });
        let result = store_links(&db, &args).unwrap();
        assert_eq!(result["added"], 2);

        let content = std::fs::read_to_string(&src_path).unwrap();
        assert!(
            content.contains("[[people/joshua|Joshua]]"),
            "Should have people path: {content}"
        );
        assert!(
            content.contains("[[books/joshua|Joshua]]"),
            "Should have books path: {content}"
        );
    }

    #[test]
    fn test_migrate_repo_links_hex_to_wikilink() {
        let (db, _tmp) = test_db();
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let repo = test_repo_wikilink(tmp_dir.path());
        db.upsert_repository(&repo).unwrap();

        let people_dir = tmp_dir.path().join("people");
        std::fs::create_dir_all(&people_dir).unwrap();

        // Create files with hex ID references
        let alice_path = people_dir.join("alice.md");
        let bob_path = people_dir.join("bob.md");
        std::fs::write(&alice_path, "# Alice\n\nContent.\n\nReferences: [[bbb222]]").unwrap();
        std::fs::write(&bob_path, "# Bob\n\nContent.\n\nReferenced by: [[aaa111]]").unwrap();

        let mut doc1 = test_doc("aaa111", "Alice");
        doc1.file_path = "people/alice.md".to_string();
        db.upsert_document(&doc1).unwrap();

        let mut doc2 = test_doc("bbb222", "Bob");
        doc2.file_path = "people/bob.md".to_string();
        db.upsert_document(&doc2).unwrap();

        let args = serde_json::json!({"repo": "test-repo"});
        let result = migrate_repo_links(&db, &args).unwrap();
        assert_eq!(result["migrated"], 2);
        assert_eq!(result["link_style"], "wikilink");

        let alice_content = std::fs::read_to_string(&alice_path).unwrap();
        assert!(
            alice_content.contains("References: [[people/bob|Bob]]"),
            "Expected migrated wikilink, got: {alice_content}"
        );

        let bob_content = std::fs::read_to_string(&bob_path).unwrap();
        assert!(
            bob_content.contains("Referenced by: [[people/alice|Alice]]"),
            "Expected migrated wikilink, got: {bob_content}"
        );
    }

    #[test]
    fn test_migrate_repo_links_no_change_for_factbase_style() {
        let (db, _tmp) = test_db();
        let repo = test_repo(); // default style = factbase
        db.upsert_repository(&repo).unwrap();

        let args = serde_json::json!({"repo": "test-repo"});
        let result = migrate_repo_links(&db, &args).unwrap();
        // No docs to migrate, but should succeed
        assert_eq!(result["migrated"], 0);
        assert_eq!(result["link_style"], "factbase");
    }
}
