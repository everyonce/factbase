//! Entity service — transport-independent business logic for document/entity operations.

use crate::database::Database;
use crate::error::FactbaseError;
use crate::models::load_perspective_from_file;
use crate::output::truncate_at_word_boundary;
use serde_json::Value;
use tracing::instrument;

use super::review::helpers::resolve_repo_filter;

/// Typed parameters for entity retrieval.
#[derive(Debug, Default)]
pub struct GetEntityParams {
    pub id: String,
    pub detail: Option<String>,
    pub include_preview: bool,
    pub max_content_length: usize,
}

/// Typed parameters for listing entities.
#[derive(Debug, Default)]
pub struct ListEntitiesParams {
    pub doc_type: Option<String>,
    pub repo: Option<String>,
    pub title_filter: Option<String>,
    pub limit: usize,
}

/// Retrieves a document by ID with its link relationships.
#[instrument(name = "svc_get_entity", skip(db))]
pub fn get_entity(db: &Database, params: &GetEntityParams) -> Result<Value, FactbaseError> {
    if params.detail.as_deref() == Some("stats") {
        return get_document_stats(db, &params.id);
    }

    let doc = db.require_document(&params.id)?;
    let links_to = db.get_links_from(&params.id)?;
    let linked_from = db.get_links_to(&params.id)?;

    let mut result = doc.to_summary_json();
    let obj = result.as_object_mut().expect("to_summary_json returns object");

    if params.include_preview {
        obj.insert("preview".into(), serde_json::json!(generate_preview(&doc.content, 500)));
    }

    if params.max_content_length > 0 && doc.content.len() > params.max_content_length {
        obj.insert("content".into(), serde_json::json!(truncate_at_word_boundary(&doc.content, params.max_content_length)));
        obj.insert("content_truncated".into(), serde_json::json!(true));
    } else {
        obj.insert("content".into(), serde_json::json!(doc.content));
    }

    obj.insert("links_to".into(), serde_json::json!(links_to));
    obj.insert("linked_from".into(), serde_json::json!(linked_from));
    obj.insert("indexed_at".into(), serde_json::json!(doc.indexed_at.to_rfc3339()));

    Ok(result)
}

/// Lists documents with optional filtering.
#[instrument(name = "svc_list_entities", skip(db))]
pub fn list_entities(db: &Database, params: &ListEntitiesParams) -> Result<Value, FactbaseError> {
    let repo = resolve_repo_filter(db, params.repo.as_deref())?;
    let limit = if params.limit == 0 { 50 } else { params.limit };
    let docs = db.list_documents(params.doc_type.as_deref(), repo.as_deref(), params.title_filter.as_deref(), limit)?;
    let items: Vec<Value> = docs.into_iter().map(|d| d.to_summary_json()).collect();
    Ok(serde_json::json!({ "entities": items }))
}

/// Gets repository perspective, including all repos info (replaces list_repositories).
#[instrument(name = "svc_get_perspective", skip(db))]
pub fn get_perspective(db: &Database, repo_id: Option<&str>) -> Result<Value, FactbaseError> {
    let repo_filter = resolve_repo_filter(db, repo_id)?;
    let repos = db.list_repositories_with_stats()?;
    let (repo, doc_count) = if let Some(id) = repo_filter {
        repos.iter().find(|(r, _)| r.id == id).cloned()
    } else {
        repos.first().cloned()
    }
    .ok_or_else(|| FactbaseError::not_found("No repository found"))?;

    let mut json = repo.to_summary_json(doc_count);
    let perspective = repo.perspective.or_else(|| load_perspective_from_file(&repo.path));
    let obj = json.as_object_mut().expect("to_summary_json returns object");
    obj.insert("perspective".into(), serde_json::json!(perspective));

    // Include all repos summary (replaces the removed list_repositories/repos op)
    if repos.len() > 1 {
        let all_repos: Vec<Value> = repos.iter().map(|(r, c)| r.to_summary_json(*c)).collect();
        obj.insert("all_repositories".into(), serde_json::json!(all_repos));
    }

    Ok(json)
}

/// Lists all registered repositories with document counts.
#[instrument(name = "svc_list_repositories", skip(db))]
pub fn list_repositories(db: &Database) -> Result<Value, FactbaseError> {
    let repos = db.list_repositories_with_stats()?;
    let items: Vec<Value> = repos.into_iter().map(|(r, c)| r.to_summary_json(c)).collect();
    Ok(serde_json::json!({ "repositories": items }))
}

/// Gets detailed statistics for a document.
#[instrument(name = "svc_get_document_stats", skip(db))]
pub fn get_document_stats(db: &Database, id: &str) -> Result<Value, FactbaseError> {
    use crate::mcp::tools::helpers::{
        build_link_stats_json, build_review_stats_json, build_source_stats_json,
        build_temporal_stats_json,
    };

    let doc = db.require_document(id)?;
    let links_to = db.get_links_from(id)?;
    let linked_from = db.get_links_to(id)?;

    let mut result = doc.to_summary_json();
    let obj = result.as_object_mut().expect("to_summary_json returns object");
    obj.insert("temporal".into(), build_temporal_stats_json(&doc.content));
    obj.insert("sources".into(), build_source_stats_json(&doc.content));
    obj.insert("links".into(), build_link_stats_json(links_to.len(), linked_from.len()));
    obj.insert("word_count".into(), serde_json::json!(crate::models::word_count(&doc.content)));
    obj.insert("review_queue".into(), build_review_stats_json(&doc.content));
    Ok(result)
}

/// Generate a content preview, truncating at word boundary.
pub fn generate_preview(content: &str, max_len: usize) -> String {
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim().starts_with("<!-- factbase:") && !l.trim().is_empty())
        .collect();
    let text = lines.join("\n");
    truncate_at_word_boundary(&text, max_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_repositories_format() {
        let repos: Vec<(crate::models::Repository, usize)> = vec![(
            crate::models::Repository {
                id: "test".to_string(), name: "Test Repo".to_string(),
                path: std::path::PathBuf::from("/tmp/test"), perspective: None,
                created_at: chrono::Utc::now(), last_indexed_at: None, last_check_at: None,
            },
            5,
        )];
        let items: Vec<Value> = repos.iter().map(|(r, c)| r.to_summary_json(*c)).collect();
        let result = serde_json::json!({ "repositories": items });
        assert_eq!(result["repositories"][0]["id"], "test");
        assert_eq!(result["repositories"][0]["document_count"], 5);
    }

    #[test]
    fn test_generate_preview_short() {
        assert_eq!(generate_preview("Short", 500), "Short");
    }

    #[test]
    fn test_generate_preview_skips_header() {
        let content = "<!-- factbase:abc123 -->\n\n# Title\n\nContent";
        let preview = generate_preview(content, 500);
        assert!(!preview.contains("factbase:"));
        assert!(preview.contains("Title"));
    }

    #[test]
    fn test_generate_preview_truncates() {
        let content = "This is a longer piece of content that needs truncation";
        let preview = generate_preview(content, 30);
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn test_get_entity_returns_content_and_links() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let doc = test_doc("abc123", "Test Entity");
        db.upsert_document(&doc).unwrap();
        let params = GetEntityParams {
            id: "abc123".into(),
            detail: None,
            include_preview: false,
            max_content_length: 0,
        };
        let result = get_entity(&db, &params).unwrap();
        assert_eq!(result["id"], "abc123");
        assert_eq!(result["title"], "Test Entity");
        assert!(result["content"].as_str().unwrap().contains("Test Entity"));
        assert!(result["links_to"].is_array());
        assert!(result["linked_from"].is_array());
    }

    #[test]
    fn test_get_entity_with_preview() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let doc = test_doc("abc123", "Test Entity");
        db.upsert_document(&doc).unwrap();
        let params = GetEntityParams {
            id: "abc123".into(),
            detail: None,
            include_preview: true,
            max_content_length: 0,
        };
        let result = get_entity(&db, &params).unwrap();
        assert!(result.get("preview").is_some());
    }

    #[test]
    fn test_get_entity_truncates_content() {
        use crate::database::tests::{test_db, test_repo};
        use crate::models::Document;
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let doc = Document {
            id: "abc123".into(),
            content: "x ".repeat(500),
            ..Document::test_default()
        };
        db.upsert_document(&doc).unwrap();
        let params = GetEntityParams {
            id: "abc123".into(),
            detail: None,
            include_preview: false,
            max_content_length: 50,
        };
        let result = get_entity(&db, &params).unwrap();
        assert_eq!(result["content_truncated"], true);
    }

    #[test]
    fn test_get_entity_not_found() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let params = GetEntityParams {
            id: "nonexistent".into(),
            ..Default::default()
        };
        assert!(get_entity(&db, &params).is_err());
    }

    #[test]
    fn test_get_entity_stats_detail() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let doc = test_doc("abc123", "Test Entity");
        db.upsert_document(&doc).unwrap();
        let params = GetEntityParams {
            id: "abc123".into(),
            detail: Some("stats".into()),
            include_preview: false,
            max_content_length: 0,
        };
        let result = get_entity(&db, &params).unwrap();
        assert!(result.get("temporal").is_some());
        assert!(result.get("sources").is_some());
        assert!(result.get("links").is_some());
        assert!(result.get("word_count").is_some());
    }

    #[test]
    fn test_list_entities_empty() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let params = ListEntitiesParams { limit: 10, ..Default::default() };
        let result = list_entities(&db, &params).unwrap();
        assert_eq!(result["entities"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_list_entities_with_docs() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        db.upsert_document(&test_doc("a1", "Doc A")).unwrap();
        db.upsert_document(&test_doc("b2", "Doc B")).unwrap();
        let params = ListEntitiesParams { limit: 10, ..Default::default() };
        let result = list_entities(&db, &params).unwrap();
        assert_eq!(result["entities"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_list_entities_respects_limit() {
        use crate::database::tests::{test_db, test_doc, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        for i in 0..5 {
            db.upsert_document(&test_doc(&format!("d{i:02}"), &format!("Doc {i}"))).unwrap();
        }
        let params = ListEntitiesParams { limit: 2, ..Default::default() };
        let result = list_entities(&db, &params).unwrap();
        assert_eq!(result["entities"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_list_repositories_returns_repos() {
        use crate::database::tests::{test_db, test_repo};
        let (db, _tmp) = test_db();
        db.upsert_repository(&test_repo()).unwrap();
        let result = list_repositories(&db).unwrap();
        let repos = result["repositories"].as_array().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0]["id"], "test-repo");
    }

    #[test]
    fn test_generate_preview_empty_content() {
        assert_eq!(generate_preview("", 500), "");
    }
}
