//! Entity-related MCP tools — thin wrappers delegating to service layer.

use super::{get_bool_arg, get_str_arg, get_u64_arg, resolve_repo_filter};
use crate::database::Database;
use crate::error::FactbaseError;
use crate::services;
use crate::services::entity::{GetEntityParams, ListEntitiesParams};
use serde_json::Value;
use tracing::instrument;

/// Retrieves a document by ID with its link relationships.
#[instrument(name = "mcp_get_entity", skip(db, args))]
pub fn get_entity(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let params = GetEntityParams {
        id: super::get_str_arg_required(args, "id")?,
        detail: get_str_arg(args, "detail").map(String::from),
        include_preview: get_bool_arg(args, "include_preview", false),
        max_content_length: get_u64_arg(args, "max_content_length", 0) as usize,
    };
    services::get_entity(db, &params)
}

/// Lists documents with optional filtering.
#[instrument(name = "mcp_list_entities", skip(db, args))]
pub fn list_entities(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let params = ListEntitiesParams {
        doc_type: get_str_arg(args, "doc_type").map(String::from),
        repo: get_str_arg(args, "repo").map(String::from),
        title_filter: get_str_arg(args, "title_filter").map(String::from),
        limit: get_u64_arg(args, "limit", 50) as usize,
    };
    services::list_entities(db, &params)
}

/// Gets repository perspective.
#[instrument(name = "mcp_get_perspective", skip(db, args))]
pub fn get_perspective(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo_id = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    services::get_perspective(db, repo_id.as_deref())
}

/// Lists all registered repositories with document counts.
#[allow(dead_code)] // Kept for backward compat; removed from MCP dispatch, still used by web API via services layer
#[instrument(name = "mcp_list_repositories", skip(db))]
pub fn list_repositories(db: &Database) -> Result<Value, FactbaseError> {
    services::list_repositories(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::entity::generate_preview;

    #[test]
    fn test_list_repositories_tool_format() {
        let repos: Vec<(crate::models::Repository, usize)> = vec![(
            crate::models::Repository {
                id: "test".to_string(),
                name: "Test Repo".to_string(),
                path: std::path::PathBuf::from("/tmp/test"),
                perspective: None,
                created_at: chrono::Utc::now(),
                last_indexed_at: None,
                last_check_at: None,
            },
            5,
        )];
        let items: Vec<serde_json::Value> = repos
            .iter()
            .map(|(r, count)| r.to_summary_json(*count))
            .collect();
        let result = serde_json::json!({ "repositories": items });
        assert!(result["repositories"].is_array());
        assert_eq!(result["repositories"][0]["id"], "test");
        assert_eq!(result["repositories"][0]["document_count"], 5);
    }

    #[test]
    fn test_generate_preview_short_content() {
        assert_eq!(generate_preview("Short content", 500), "Short content");
    }

    #[test]
    fn test_generate_preview_skips_header() {
        let content = "---\nfactbase_id: abc123\n---\n\n# Title\n\nActual content here";
        let preview = generate_preview(content, 500);
        assert!(!preview.contains("factbase:"));
        assert!(preview.contains("Title"));
    }

    #[test]
    fn test_list_entities_doc_type_filter_extracted() {
        let args = serde_json::json!({ "doc_type": "person" });
        let doc_type = get_str_arg(&args, "doc_type");
        assert_eq!(doc_type, Some("person"));
    }

    #[test]
    fn test_get_perspective_falls_back_to_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("perspective.yaml"),
            "focus: \"test focus\"\norganization: \"test org\"\n",
        )
        .unwrap();

        let repo = crate::models::Repository {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: tmp.path().to_path_buf(),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_check_at: None,
        };
        let perspective = repo
            .perspective
            .or_else(|| crate::models::load_perspective_from_file(&repo.path));
        assert!(perspective.is_some());
        assert_eq!(perspective.unwrap().focus.as_deref(), Some("test focus"));
    }
}
