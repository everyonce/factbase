//! Embedding import/export operations.
//!
//! Provides JSONL-based export and import of vector embeddings with model metadata,
//! enabling portable distribution and backup of pre-computed embeddings.
//!
//! # Format (v2)
//!
//! The JSONL file has a header line followed by chunk and fact embedding records:
//!
//! ```jsonl
//! {"format_version":2,"model":"...","dimension":1024,"exported_at":"...","chunk_count":42,"fact_embedding_count":10}
//! {"record_type":"chunk","doc_id":"a1cb2b","chunk_index":0,"chunk_start":0,"chunk_end":1000,"embedding":[...]}
//! {"record_type":"fact","doc_id":"a1cb2b","fact_id":"a1cb2b_3","line_number":3,"fact_text":"...","fact_hash":"...","embedding":[...]}
//! ```
//!
//! Records without `record_type` (v1 format) are treated as chunks for backward compatibility.

use crate::database::Database;
use crate::error::FactbaseError;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;

/// Current format version for embedding exports.
pub const FORMAT_VERSION: u32 = 2;

/// Header line in the JSONL export file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingExportHeader {
    pub format_version: u32,
    pub model: String,
    pub dimension: usize,
    pub exported_at: String,
    pub chunk_count: usize,
    /// Number of fact embeddings (added in v2, defaults to 0 for v1 imports).
    #[serde(default)]
    pub fact_embedding_count: usize,
}

/// A single embedding chunk record in the JSONL export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    #[serde(default = "default_chunk_type")]
    pub record_type: String,
    pub doc_id: String,
    pub chunk_index: usize,
    pub chunk_start: usize,
    pub chunk_end: usize,
    pub embedding: Vec<f32>,
}

fn default_chunk_type() -> String {
    "chunk".to_string()
}

/// A fact embedding record in the JSONL export (v2+).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactEmbeddingRecord {
    pub doc_id: String,
    pub fact_id: String,
    pub line_number: usize,
    pub fact_text: String,
    pub fact_hash: String,
    pub embedding: Vec<f32>,
}

/// Result of an embedding status check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsStatusInfo {
    pub total_chunks: usize,
    pub total_documents: usize,
    pub dimension: Option<usize>,
    pub model: String,
    pub orphaned_chunks: usize,
    pub documents_without_embeddings: usize,
    pub total_fact_embeddings: usize,
    pub documents_with_fact_embeddings: usize,
    pub documents_without_fact_embeddings: usize,
}

/// Result of an embedding import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub imported_chunks: usize,
    pub skipped_chunks: usize,
    #[serde(default)]
    pub imported_facts: usize,
    #[serde(default)]
    pub skipped_facts: usize,
    pub model: String,
    pub dimension: usize,
}

/// Serializable wrapper for writing fact records with a type tag.
#[derive(Serialize)]
struct TaggedFactRecord<'a> {
    record_type: &'static str,
    doc_id: &'a str,
    fact_id: &'a str,
    line_number: usize,
    fact_text: &'a str,
    fact_hash: &'a str,
    embedding: &'a [f32],
}

/// Export all embeddings for a repository (or all repos) to a writer.
/// Returns (chunk_count, fact_count).
pub fn export_embeddings<W: Write>(
    db: &Database,
    repo_id: Option<&str>,
    model: &str,
    writer: &mut W,
) -> Result<(usize, usize), FactbaseError> {
    let chunk_records = db.export_all_embeddings(repo_id)?;
    let fact_records = db.export_all_fact_embeddings(repo_id)?;
    let dimension = db.get_embedding_dimension()?.unwrap_or(1024);

    let header = EmbeddingExportHeader {
        format_version: FORMAT_VERSION,
        model: model.to_string(),
        dimension,
        exported_at: chrono::Utc::now().to_rfc3339(),
        chunk_count: chunk_records.len(),
        fact_embedding_count: fact_records.len(),
    };

    serde_json::to_writer(&mut *writer, &header)
        .map_err(|e| FactbaseError::internal(format!("Failed to write header: {e}")))?;
    writer.write_all(b"\n")?;

    for record in &chunk_records {
        serde_json::to_writer(&mut *writer, record)
            .map_err(|e| FactbaseError::internal(format!("Failed to write record: {e}")))?;
        writer.write_all(b"\n")?;
    }

    for record in &fact_records {
        let tagged = TaggedFactRecord {
            record_type: "fact",
            doc_id: &record.doc_id,
            fact_id: &record.fact_id,
            line_number: record.line_number,
            fact_text: &record.fact_text,
            fact_hash: &record.fact_hash,
            embedding: &record.embedding,
        };
        serde_json::to_writer(&mut *writer, &tagged)
            .map_err(|e| FactbaseError::internal(format!("Failed to write fact record: {e}")))?;
        writer.write_all(b"\n")?;
    }

    Ok((chunk_records.len(), fact_records.len()))
}

/// Export embeddings to a file path. Returns (chunk_count, fact_count).
pub fn export_embeddings_to_file(
    db: &Database,
    repo_id: Option<&str>,
    model: &str,
    path: &Path,
) -> Result<(usize, usize), FactbaseError> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    let counts = export_embeddings(db, repo_id, model, &mut writer)?;
    writer.flush()?;
    Ok(counts)
}

/// Import embeddings from a reader, validating model and dimension compatibility.
pub fn import_embeddings<R: BufRead>(
    db: &Database,
    reader: &mut R,
    force: bool,
) -> Result<ImportResult, FactbaseError> {
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let header: EmbeddingExportHeader = serde_json::from_str(line.trim()).map_err(|e| {
        FactbaseError::internal(format!("Invalid embedding export header: {e}"))
    })?;

    if header.format_version > FORMAT_VERSION {
        return Err(FactbaseError::internal(format!(
            "Unsupported format version {} (this binary supports up to {})",
            header.format_version, FORMAT_VERSION
        )));
    }

    // Check dimension compatibility
    if let Some(current_dim) = db.get_embedding_dimension()? {
        if current_dim != header.dimension && !force {
            return Err(FactbaseError::internal(format!(
                "Dimension mismatch: database has {current_dim}-dim embeddings, file has {}-dim. Use --force to overwrite.",
                header.dimension
            )));
        }
    }

    let mut imported_chunks = 0usize;
    let mut skipped_chunks = 0usize;
    let mut imported_facts = 0usize;
    let mut skipped_facts = 0usize;

    // Get set of existing document IDs to skip orphaned embeddings
    let existing_docs = db.get_all_document_ids()?;

    for line_result in reader.lines() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        // Peek at record_type to determine how to parse
        let value: serde_json::Value = serde_json::from_str(&line).map_err(|e| {
            FactbaseError::internal(format!("Invalid record: {e}"))
        })?;

        let record_type = value
            .get("record_type")
            .and_then(|v| v.as_str())
            .unwrap_or("chunk");

        match record_type {
            "fact" => {
                let record: FactEmbeddingRecord =
                    serde_json::from_value(value).map_err(|e| {
                        FactbaseError::internal(format!("Invalid fact record: {e}"))
                    })?;
                if !existing_docs.contains(&record.doc_id) {
                    skipped_facts += 1;
                    continue;
                }
                db.upsert_fact_embedding(
                    &record.fact_id,
                    &record.doc_id,
                    record.line_number,
                    &record.fact_text,
                    &record.fact_hash,
                    &record.embedding,
                )?;
                imported_facts += 1;
            }
            _ => {
                let record: EmbeddingRecord =
                    serde_json::from_value(value).map_err(|e| {
                        FactbaseError::internal(format!("Invalid embedding record: {e}"))
                    })?;
                if !existing_docs.contains(&record.doc_id) {
                    skipped_chunks += 1;
                    continue;
                }
                db.upsert_embedding_chunk(
                    &record.doc_id,
                    record.chunk_index,
                    record.chunk_start,
                    record.chunk_end,
                    &record.embedding,
                )?;
                imported_chunks += 1;
            }
        }
    }

    Ok(ImportResult {
        imported_chunks,
        skipped_chunks,
        imported_facts,
        skipped_facts,
        model: header.model,
        dimension: header.dimension,
    })
}

/// Import embeddings from a file path.
pub fn import_embeddings_from_file(
    db: &Database,
    path: &Path,
    force: bool,
) -> Result<ImportResult, FactbaseError> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    import_embeddings(db, &mut reader, force)
}

/// Get embedding status information.
pub fn embeddings_status(
    db: &Database,
    repo_id: Option<&str>,
    model: &str,
) -> Result<EmbeddingsStatusInfo, FactbaseError> {
    let status = if let Some(rid) = repo_id {
        db.check_embedding_status(rid)?
    } else {
        // Check across all repos
        let repos = db.list_repositories()?;
        let mut combined = crate::database::EmbeddingStatus {
            with_embeddings: Vec::new(),
            without_embeddings: Vec::new(),
            orphaned: Vec::new(),
        };
        for repo in &repos {
            let s = db.check_embedding_status(&repo.id)?;
            combined.with_embeddings.extend(s.with_embeddings);
            combined.without_embeddings.extend(s.without_embeddings);
            combined.orphaned.extend(s.orphaned);
        }
        combined
    };

    let total_chunks = db.count_embedding_chunks()?;
    let dimension = db.get_embedding_dimension()?;
    let total_fact_embeddings = db.get_fact_embedding_count()?;
    let documents_with_fact_embeddings = db.count_documents_with_fact_embeddings()?;
    let total_docs_in_db = status.with_embeddings.len() + status.without_embeddings.len();

    Ok(EmbeddingsStatusInfo {
        total_chunks,
        total_documents: status.with_embeddings.len(),
        dimension,
        model: model.to_string(),
        orphaned_chunks: status.orphaned.len(),
        documents_without_embeddings: status.without_embeddings.len(),
        total_fact_embeddings,
        documents_with_fact_embeddings,
        documents_without_fact_embeddings: total_docs_in_db.saturating_sub(documents_with_fact_embeddings),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_doc, test_repo};

    fn setup_db_with_embeddings() -> (Database, tempfile::TempDir) {
        let (db, tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let emb1: Vec<f32> = vec![0.1; 1024];
        let emb2: Vec<f32> = vec![0.2; 1024];
        db.upsert_embedding_chunk("doc1", 0, 0, 500, &emb1).unwrap();
        db.upsert_embedding_chunk("doc1", 1, 500, 1000, &emb1).unwrap();
        db.upsert_embedding("doc2", &emb2).unwrap();

        (db, tmp)
    }

    fn add_fact_embeddings(db: &Database) {
        let emb: Vec<f32> = vec![0.3; 1024];
        db.upsert_fact_embedding("doc1_1", "doc1", 1, "Fact A", "h1", &emb).unwrap();
        db.upsert_fact_embedding("doc1_2", "doc1", 2, "Fact B", "h2", &emb).unwrap();
        db.upsert_fact_embedding("doc2_1", "doc2", 1, "Fact C", "h3", &emb).unwrap();
    }

    #[test]
    fn test_export_header_format() {
        let header = EmbeddingExportHeader {
            format_version: FORMAT_VERSION,
            model: "test-model".into(),
            dimension: 1024,
            exported_at: "2026-01-01T00:00:00Z".into(),
            chunk_count: 5,
            fact_embedding_count: 3,
        };
        let json = serde_json::to_string(&header).unwrap();
        assert!(json.contains("\"format_version\":2"));
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"dimension\":1024"));
        assert!(json.contains("\"fact_embedding_count\":3"));
    }

    #[test]
    fn test_embedding_record_roundtrip() {
        let record = EmbeddingRecord {
            record_type: "chunk".into(),
            doc_id: "abc123".into(),
            chunk_index: 0,
            chunk_start: 0,
            chunk_end: 1000,
            embedding: vec![0.1, 0.2, 0.3],
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: EmbeddingRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.doc_id, "abc123");
        assert_eq!(parsed.record_type, "chunk");
        assert_eq!(parsed.embedding.len(), 3);
    }

    #[test]
    fn test_embedding_record_defaults_to_chunk() {
        // v1 records without record_type should default to "chunk"
        let json = r#"{"doc_id":"abc","chunk_index":0,"chunk_start":0,"chunk_end":100,"embedding":[0.1]}"#;
        let parsed: EmbeddingRecord = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.record_type, "chunk");
    }

    #[test]
    fn test_fact_embedding_record_roundtrip() {
        let record = FactEmbeddingRecord {
            doc_id: "abc123".into(),
            fact_id: "abc123_3".into(),
            line_number: 3,
            fact_text: "Some fact".into(),
            fact_hash: "deadbeef".into(),
            embedding: vec![0.1, 0.2, 0.3],
        };
        let json = serde_json::to_string(&record).unwrap();
        let parsed: FactEmbeddingRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.doc_id, "abc123");
        assert_eq!(parsed.fact_id, "abc123_3");
        assert_eq!(parsed.line_number, 3);
        assert_eq!(parsed.fact_text, "Some fact");
        assert_eq!(parsed.fact_hash, "deadbeef");
    }

    #[test]
    fn test_export_and_import_roundtrip() {
        let (db, _tmp) = setup_db_with_embeddings();

        // Export
        let mut buf = Vec::new();
        let (chunks, facts) = export_embeddings(&db, None, "test-model", &mut buf).unwrap();
        assert_eq!(chunks, 3); // 2 chunks for doc1 + 1 for doc2
        assert_eq!(facts, 0);

        // Create a fresh DB with same docs but no embeddings
        let (db2, _tmp2) = test_db();
        let repo = test_repo();
        db2.upsert_repository(&repo).unwrap();
        db2.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db2.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        // Import
        let mut reader = std::io::BufReader::new(&buf[..]);
        let result = import_embeddings(&db2, &mut reader, false).unwrap();
        assert_eq!(result.imported_chunks, 3);
        assert_eq!(result.skipped_chunks, 0);
        assert_eq!(result.imported_facts, 0);
        assert_eq!(result.skipped_facts, 0);
        assert_eq!(result.model, "test-model");
        assert_eq!(result.dimension, 1024);

        // Verify chunk metadata survived
        let meta = db2.get_chunk_metadata("doc1_0").unwrap().unwrap();
        assert_eq!(meta.0, "doc1");
        assert_eq!(meta.2, 0);   // chunk_start
        assert_eq!(meta.3, 500); // chunk_end
    }

    #[test]
    fn test_export_includes_fact_embeddings() {
        let (db, _tmp) = setup_db_with_embeddings();
        add_fact_embeddings(&db);

        let mut buf = Vec::new();
        let (chunks, facts) = export_embeddings(&db, None, "test-model", &mut buf).unwrap();
        assert_eq!(chunks, 3);
        assert_eq!(facts, 3);

        // Verify header
        let first_line = std::str::from_utf8(&buf).unwrap().lines().next().unwrap();
        let header: EmbeddingExportHeader = serde_json::from_str(first_line).unwrap();
        assert_eq!(header.chunk_count, 3);
        assert_eq!(header.fact_embedding_count, 3);
        assert_eq!(header.format_version, 2);
    }

    #[test]
    fn test_import_restores_fact_embeddings() {
        let (db, _tmp) = setup_db_with_embeddings();
        add_fact_embeddings(&db);

        let mut buf = Vec::new();
        export_embeddings(&db, None, "test-model", &mut buf).unwrap();

        // Fresh DB
        let (db2, _tmp2) = test_db();
        let repo = test_repo();
        db2.upsert_repository(&repo).unwrap();
        db2.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db2.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let mut reader = std::io::BufReader::new(&buf[..]);
        let result = import_embeddings(&db2, &mut reader, false).unwrap();
        assert_eq!(result.imported_chunks, 3);
        assert_eq!(result.imported_facts, 3);

        // Verify fact embeddings exist
        assert_eq!(db2.get_fact_embedding_count().unwrap(), 3);
        assert_eq!(db2.count_documents_with_fact_embeddings().unwrap(), 2);
    }

    #[test]
    fn test_roundtrip_preserves_status_counts() {
        let (db, _tmp) = setup_db_with_embeddings();
        add_fact_embeddings(&db);

        let before = embeddings_status(&db, Some("test-repo"), "test-model").unwrap();

        // Export
        let mut buf = Vec::new();
        export_embeddings(&db, None, "test-model", &mut buf).unwrap();

        // Import into fresh DB
        let (db2, _tmp2) = test_db();
        let repo = test_repo();
        db2.upsert_repository(&repo).unwrap();
        db2.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db2.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let mut reader = std::io::BufReader::new(&buf[..]);
        import_embeddings(&db2, &mut reader, false).unwrap();

        let after = embeddings_status(&db2, Some("test-repo"), "test-model").unwrap();
        assert_eq!(before.total_chunks, after.total_chunks);
        assert_eq!(before.total_fact_embeddings, after.total_fact_embeddings);
        assert_eq!(before.documents_with_fact_embeddings, after.documents_with_fact_embeddings);
    }

    #[test]
    fn test_import_v1_format_still_works() {
        // Simulate a v1 export (no record_type, no fact_embedding_count)
        let header = serde_json::json!({
            "format_version": 1,
            "model": "test-model",
            "dimension": 1024,
            "exported_at": "2026-01-01T00:00:00Z",
            "chunk_count": 1
        });
        let record = serde_json::json!({
            "doc_id": "doc1",
            "chunk_index": 0,
            "chunk_start": 0,
            "chunk_end": 500,
            "embedding": vec![0.1f32; 1024]
        });
        let data = format!("{}\n{}\n", header, record);

        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let mut reader = std::io::BufReader::new(data.as_bytes());
        let result = import_embeddings(&db, &mut reader, false).unwrap();
        assert_eq!(result.imported_chunks, 1);
        assert_eq!(result.imported_facts, 0);
        assert_eq!(result.model, "test-model");
    }

    #[test]
    fn test_import_skips_missing_docs() {
        let (db, _tmp) = setup_db_with_embeddings();
        add_fact_embeddings(&db);

        let mut buf = Vec::new();
        export_embeddings(&db, None, "test-model", &mut buf).unwrap();

        // Create DB with only doc1 (not doc2)
        let (db2, _tmp2) = test_db();
        let repo = test_repo();
        db2.upsert_repository(&repo).unwrap();
        db2.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let mut reader = std::io::BufReader::new(&buf[..]);
        let result = import_embeddings(&db2, &mut reader, false).unwrap();
        assert_eq!(result.imported_chunks, 2); // only doc1's chunks
        assert_eq!(result.skipped_chunks, 1); // doc2's chunk skipped
        assert_eq!(result.imported_facts, 2); // only doc1's facts
        assert_eq!(result.skipped_facts, 1); // doc2's fact skipped
    }

    #[test]
    fn test_import_rejects_future_format_version() {
        let header = EmbeddingExportHeader {
            format_version: 999,
            model: "test".into(),
            dimension: 1024,
            exported_at: "2026-01-01T00:00:00Z".into(),
            chunk_count: 0,
            fact_embedding_count: 0,
        };
        let data = format!("{}\n", serde_json::to_string(&header).unwrap());
        let (db, _tmp) = test_db();
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let result = import_embeddings(&db, &mut reader, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported format version"));
    }

    #[test]
    fn test_import_rejects_dimension_mismatch() {
        let (db, _tmp) = setup_db_with_embeddings();

        let header = EmbeddingExportHeader {
            format_version: 1,
            model: "test".into(),
            dimension: 768, // mismatch
            exported_at: "2026-01-01T00:00:00Z".into(),
            chunk_count: 0,
            fact_embedding_count: 0,
        };
        let data = format!("{}\n", serde_json::to_string(&header).unwrap());
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let result = import_embeddings(&db, &mut reader, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Dimension mismatch"));
    }

    #[test]
    fn test_import_force_bypasses_dimension_check() {
        let (db, _tmp) = setup_db_with_embeddings();

        let header = EmbeddingExportHeader {
            format_version: 1,
            model: "test".into(),
            dimension: 768,
            exported_at: "2026-01-01T00:00:00Z".into(),
            chunk_count: 0,
            fact_embedding_count: 0,
        };
        let data = format!("{}\n", serde_json::to_string(&header).unwrap());
        let mut reader = std::io::BufReader::new(data.as_bytes());
        let result = import_embeddings(&db, &mut reader, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_embeddings_status() {
        let (db, _tmp) = setup_db_with_embeddings();
        let info = embeddings_status(&db, Some("test-repo"), "test-model").unwrap();
        assert_eq!(info.total_documents, 2);
        assert_eq!(info.total_chunks, 3);
        assert_eq!(info.dimension, Some(1024));
        assert_eq!(info.documents_without_embeddings, 0);
        assert_eq!(info.total_fact_embeddings, 0);
        assert_eq!(info.documents_with_fact_embeddings, 0);
        assert_eq!(info.documents_without_fact_embeddings, 2);
    }

    #[test]
    fn test_embeddings_status_includes_fact_counts() {
        let (db, _tmp) = setup_db_with_embeddings();
        let emb: Vec<f32> = vec![0.1; 1024];
        db.upsert_fact_embedding("doc1_1", "doc1", 1, "Fact A", "h1", &emb).unwrap();
        db.upsert_fact_embedding("doc1_2", "doc1", 2, "Fact B", "h2", &emb).unwrap();
        db.upsert_fact_embedding("doc2_1", "doc2", 1, "Fact C", "h3", &emb).unwrap();

        let info = embeddings_status(&db, Some("test-repo"), "test-model").unwrap();
        assert_eq!(info.total_fact_embeddings, 3);
        assert_eq!(info.documents_with_fact_embeddings, 2);
        assert_eq!(info.documents_without_fact_embeddings, 0);
    }

    #[test]
    fn test_embeddings_status_with_zero_fact_embeddings() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();
        db.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();

        let info = embeddings_status(&db, Some("test-repo"), "test-model").unwrap();
        assert_eq!(info.total_fact_embeddings, 0);
        assert_eq!(info.documents_with_fact_embeddings, 0);
        assert_eq!(info.documents_without_fact_embeddings, 1);
    }

    #[test]
    fn test_export_to_file_and_import() {
        let (db, _tmp) = setup_db_with_embeddings();
        add_fact_embeddings(&db);
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("embeddings.jsonl");

        let (chunks, facts) = export_embeddings_to_file(&db, None, "test-model", &path).unwrap();
        assert_eq!(chunks, 3);
        assert_eq!(facts, 3);

        // Import into fresh DB
        let (db2, _tmp2) = test_db();
        let repo = test_repo();
        db2.upsert_repository(&repo).unwrap();
        db2.upsert_document(&test_doc("doc1", "Doc 1")).unwrap();
        db2.upsert_document(&test_doc("doc2", "Doc 2")).unwrap();

        let result = import_embeddings_from_file(&db2, &path, false).unwrap();
        assert_eq!(result.imported_chunks, 3);
        assert_eq!(result.imported_facts, 3);
    }

    #[test]
    fn test_export_empty_db() {
        let (db, _tmp) = test_db();
        let mut buf = Vec::new();
        let (chunks, facts) = export_embeddings(&db, None, "test-model", &mut buf).unwrap();
        assert_eq!(chunks, 0);
        assert_eq!(facts, 0);

        // Verify header is still written
        let lines: Vec<&str> = std::str::from_utf8(&buf).unwrap().trim().lines().collect();
        assert_eq!(lines.len(), 1); // just header
        let header: EmbeddingExportHeader = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header.chunk_count, 0);
        assert_eq!(header.fact_embedding_count, 0);
    }
}
