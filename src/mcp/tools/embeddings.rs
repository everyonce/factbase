//! MCP tools for embedding import/export/status.

use crate::database::Database;
use crate::embeddings_io;
use crate::error::FactbaseError;
use crate::Config;
use serde_json::Value;

use super::helpers::{get_bool_arg, get_str_arg};

/// MCP tool: export embeddings to JSONL string.
pub fn embeddings_export(db: &Database, args: &Value) -> Result<Value, FactbaseError> {
    let repo = get_str_arg(args, "repo");
    let config = Config::load(None).unwrap_or_default();
    let model = config.embedding.model;

    let mut buf = Vec::new();
    let (chunk_count, fact_count) = embeddings_io::export_embeddings(db, repo, &model, &mut buf)?;
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
    let model = config.embedding.model;
    let info = embeddings_io::embeddings_status(db, None, &model)?;
    Ok(serde_json::to_value(&info)
        .map_err(|e| FactbaseError::internal(format!("Serialization error: {e}")))?)
}
