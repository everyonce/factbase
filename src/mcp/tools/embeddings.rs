//! MCP tools for embedding import/export/status.

use crate::database::Database;
use crate::embeddings_io;
use crate::error::FactbaseError;
use crate::Config;
use serde_json::Value;

use super::helpers::{get_bool_arg, get_str_arg, resolve_repo_filter};

/// MCP tool: export embeddings to JSONL string.
pub fn embeddings_export(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo = resolve_repo_filter(db, get_str_arg(args, "repo"))?;
    let config = Config::load(None).unwrap_or_default();
    let model = config.embedding.model;

    let mut buf = Vec::new();
    let (chunk_count, fact_count) = embeddings_io::export_embeddings(db, repo.as_deref(), &model, &mut buf)?;
    let output = String::from_utf8(buf)
        .map_err(|e| FactbaseError::internal(format!("UTF-8 error: {e}")))?;

    Ok(serde_json::json!({
        "chunk_count": chunk_count,
        "fact_embedding_count": fact_count,
        "model": model,
        "format": "jsonl",
        "data": output,
    }))
}

/// MCP tool: import embeddings from JSONL string.
pub fn embeddings_import(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let data = args
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FactbaseError::internal("Missing required 'data' parameter"))?;
    let force = get_bool_arg(args, "force", false);

    let mut reader = std::io::BufReader::new(data.as_bytes());
    let result = embeddings_io::import_embeddings(db, &mut reader, force)?;

    Ok(serde_json::json!({
        "imported_chunks": result.imported_chunks,
        "skipped_chunks": result.skipped_chunks,
        "imported_facts": result.imported_facts,
        "skipped_facts": result.skipped_facts,
        "model": result.model,
        "dimension": result.dimension,
    }))
}

/// MCP tool: get embedding status.
pub fn embeddings_status_tool(db: &Database) -> Result<Value, FactbaseError> {
    let config = Config::load(None).unwrap_or_default();
    let config_model = &config.embedding.model;
    let config_dim = config.embedding.dimension;
    let info = embeddings_io::embeddings_status(db, None, config_model)?;

    let mut result = serde_json::to_value(&info)
        .map_err(|e| FactbaseError::internal(format!("Serialization error: {e}")))?;

    // Show DB model vs config model if they differ
    let db_model = db.get_stored_embedding_model().ok().flatten();
    let db_dim = db.get_stored_embedding_dim().ok().flatten();

    if let Some(obj) = result.as_object_mut() {
        obj.insert("config_model".to_string(), serde_json::json!(config_model));
        obj.insert("config_dimension".to_string(), serde_json::json!(config_dim));
        if let Some(ref stored) = db_model {
            if stored != config_model {
                obj.insert("db_model".to_string(), serde_json::json!(stored));
                obj.insert("model_mismatch".to_string(), serde_json::json!(true));
            }
        }
        if let Some(stored_dim) = db_dim {
            if stored_dim != config_dim {
                obj.insert("db_dimension".to_string(), serde_json::json!(stored_dim));
                obj.insert("dimension_mismatch".to_string(), serde_json::json!(true));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embeddings_status_returns_config_info() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let result = embeddings_status_tool(&db).unwrap();
        assert!(result.get("config_model").is_some());
        assert!(result.get("config_dimension").is_some());
    }

    #[test]
    fn test_embeddings_import_missing_data() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({});
        let result = embeddings_import(&db, &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_embeddings_import_empty_data() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({"data": ""});
        // Empty JSONL may fail or succeed depending on parser — just verify no panic
        let _ = embeddings_import(&db, &args);
    }

    #[test]
    fn test_embeddings_export_empty_db() {
        use crate::database::tests::test_db;
        let (db, _tmp) = test_db();
        let args = serde_json::json!({});
        let result = embeddings_export(&db, &args).unwrap();
        assert_eq!(result["chunk_count"], 0);
        assert_eq!(result["format"], "jsonl");
    }
}
