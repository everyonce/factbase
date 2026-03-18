//! Embedding generation phase for document scanning

use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::mem;
use std::time::Instant;

use crate::progress::ProgressReporter;
use crate::{chunk_document, Database, Document, EmbeddingProvider};

use super::{ChunkInfo, PendingDoc};
use crate::scanner::progress::OptionalProgress;

/// Input parameters for the embedding phase
pub struct EmbeddingPhaseInput<'a> {
    pub pending: Vec<PendingDoc>,
    pub repo_id: &'a str,
    pub embedding: &'a dyn EmbeddingProvider,
    pub db: &'a Database,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub embedding_batch_size: usize,
    pub show_progress: bool,
    pub verbose: bool,
    pub collect_stats: bool,
    pub progress: &'a ProgressReporter,
}

/// Output from the embedding phase
pub struct EmbeddingPhaseOutput {
    pub docs_embedded: usize,
    pub embedding_ms: u64,
    pub db_write_ms: u64,
    /// Per-file timing: (doc_idx, chunk_count, embedding_ms, total_ms)
    pub file_timings: Vec<(usize, usize, u64, u64)>,
    pub interrupted: bool,
}

/// Run the embedding generation phase
pub async fn run_embedding_phase(
    mut input: EmbeddingPhaseInput<'_>,
) -> anyhow::Result<EmbeddingPhaseOutput> {
    let embedding_start = Instant::now();
    let mut db_write_ms = 0u64;
    let mut file_timings: Vec<(usize, usize, u64, u64)> = Vec::new();

    if input.pending.is_empty() {
        return Ok(EmbeddingPhaseOutput {
            docs_embedded: 0,
            embedding_ms: 0,
            db_write_ms: 0,
            file_timings: Vec::new(),
            interrupted: false,
        });
    }

    // Chunk documents and collect all chunks for batch embedding
    let total_docs = input.pending.len();
    let mut all_chunks: Vec<ChunkInfo> = Vec::with_capacity(total_docs * 2);
    let mut chunks_per_doc: Vec<usize> = vec![0; total_docs];

    for (doc_idx, doc) in input.pending.iter().enumerate() {
        let chunks = chunk_document(&doc.content, input.chunk_size, input.chunk_overlap);
        if input.verbose && chunks.len() > 1 {
            println!(
                "  CHUNKED {} into {} chunks ({} chars)",
                doc.relative,
                chunks.len(),
                doc.content.len()
            );
        }
        chunks_per_doc[doc_idx] = chunks.len();
        for chunk in chunks {
            all_chunks.push(ChunkInfo {
                doc_idx,
                chunk_idx: chunk.index,
                chunk_start: chunk.start,
                chunk_end: chunk.end,
                content: chunk.content,
            });
        }
    }

    let total_chunks = all_chunks.len();
    let embed_pb = if input.show_progress {
        OptionalProgress::new(
            total_chunks as u64,
            "[{elapsed_precise}] {bar:40.green/blue} {pos}/{len} {msg} (ETA: {eta})",
            "embedding",
            5,
        )
    } else {
        OptionalProgress::none()
    };

    // Track saved docs: maps doc_idx -> doc_id (stored for subsequent chunk embeddings)
    let mut saved_doc_ids: HashMap<usize, String> = HashMap::new();
    let mut embedded = 0usize;
    let mut interrupted = false;
    let mut doc_timing: HashMap<usize, (Instant, u64)> = HashMap::new();

    for batch in all_chunks.chunks(input.embedding_batch_size) {
        if crate::shutdown::is_shutdown_requested() {
            interrupted = true;
            break;
        }

        let batch_start = Instant::now();
        let texts: Vec<&str> = batch.iter().map(|c| c.content.as_str()).collect();
        let embeddings = input.embedding.generate_batch(&texts).await?;
        let batch_embedding_ms = batch_start.elapsed().as_millis() as u64;
        let ms_per_chunk = if !batch.is_empty() {
            batch_embedding_ms / batch.len() as u64
        } else {
            0
        };

        for (chunk_info, emb) in batch.iter().zip(embeddings.into_iter()) {
            doc_timing
                .entry(chunk_info.doc_idx)
                .or_insert_with(|| (Instant::now(), 0))
                .1 += ms_per_chunk;

            // Get or create the doc_id for this document
            let doc_id = if let Some(id) = saved_doc_ids.get(&chunk_info.doc_idx) {
                id.as_str()
            } else {
                // First chunk for this doc - save document and store id
                doc_timing
                    .entry(chunk_info.doc_idx)
                    .or_insert_with(|| (Instant::now(), 0));

                // Take ownership of fields from PendingDoc to avoid cloning
                let doc = &mut input.pending[chunk_info.doc_idx];
                let id = mem::take(&mut doc.id);
                let file_path = mem::take(&mut doc.relative);
                let file_hash = mem::take(&mut doc.hash);
                let title = mem::take(&mut doc.title);
                let doc_type = mem::take(&mut doc.doc_type);
                let content = mem::take(&mut doc.content);

                let document = Document {
                    id: id.clone(), // Clone id once - needed for HashMap and Document
                    repo_id: input.repo_id.to_string(),
                    file_path,
                    file_hash,
                    title,
                    doc_type: Some(doc_type),
                    content,
                    file_modified_at: fs::metadata(&doc.path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(chrono::DateTime::from),
                    indexed_at: Utc::now(),
                    is_deleted: false,
                };
                let db_start = Instant::now();
                input.db.upsert_document(&document)?;
                // Sync review questions to DB for fast indexed access
                if crate::patterns::has_review_section(&document.content) {
                    if let Some(questions) = crate::processor::parse_review_queue(&document.content)
                    {
                        let _ = input.db.sync_review_questions(&document.id, &questions);
                    }
                }
                db_write_ms += db_start.elapsed().as_millis() as u64;

                saved_doc_ids.insert(chunk_info.doc_idx, id);
                input
                    .progress
                    .report(saved_doc_ids.len(), total_docs, "documents embedded");
                saved_doc_ids
                    .get(&chunk_info.doc_idx)
                    .expect("just inserted")
                    .as_str()
            };

            // Store chunk embedding (after document exists)
            let db_start = Instant::now();
            input.db.upsert_embedding_chunk(
                doc_id,
                chunk_info.chunk_idx,
                chunk_info.chunk_start,
                chunk_info.chunk_end,
                &emb,
            )?;
            db_write_ms += db_start.elapsed().as_millis() as u64;

            embedded += 1;
            embed_pb.set_position(embedded as u64);
        }
    }
    embed_pb.finish_and_clear();

    // Collect per-file timing data
    if input.collect_stats {
        for (doc_idx, (start_time, emb_ms)) in &doc_timing {
            let total_ms = start_time.elapsed().as_millis() as u64;
            file_timings.push((*doc_idx, chunks_per_doc[*doc_idx], *emb_ms, total_ms));
        }
    }

    Ok(EmbeddingPhaseOutput {
        docs_embedded: if interrupted {
            saved_doc_ids.len()
        } else {
            input.pending.len()
        },
        embedding_ms: embedding_start.elapsed().as_millis() as u64,
        db_write_ms,
        file_timings,
        interrupted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::models::Repository;
    use crate::ProgressReporter;
    use std::path::PathBuf;

    fn make_pending(id: &str, content: &str, path: PathBuf) -> PendingDoc {
        PendingDoc {
            id: id.into(),
            content: content.into(),
            relative: path.file_name().unwrap().to_string_lossy().into_owned(),
            hash: crate::processor::content_hash(content),
            title: format!("Title {id}"),
            doc_type: "document".into(),
            path,
        }
    }

    fn setup_repo(db: &Database) {
        let repo = Repository {
            id: "test".into(),
            name: "test".into(),
            path: PathBuf::from("/tmp/test"),
            perspective: None,
            created_at: chrono::Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.upsert_repository(&repo).unwrap();
    }

    #[tokio::test]
    async fn test_embedding_phase_empty() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let output = run_embedding_phase(EmbeddingPhaseInput {
            pending: vec![],
            repo_id: "test",
            embedding: &embedding,
            db: &db,
            chunk_size: 100_000,
            chunk_overlap: 2_000,
            embedding_batch_size: 10,
            show_progress: false,
            verbose: false,
            collect_stats: false,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(output.docs_embedded, 0);
        assert_eq!(output.embedding_ms, 0);
        assert!(!output.interrupted);
    }

    #[tokio::test]
    async fn test_embedding_phase_single_doc() {
        let (db, _tmp) = test_db();
        setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("doc.md");
        std::fs::write(&path, "# Doc\n\nShort content.").unwrap();

        let pending = vec![make_pending("aaa111", "# Doc\n\nShort content.", path)];

        let output = run_embedding_phase(EmbeddingPhaseInput {
            pending,
            repo_id: "test",
            embedding: &embedding,
            db: &db,
            chunk_size: 100_000,
            chunk_overlap: 2_000,
            embedding_batch_size: 10,
            show_progress: false,
            verbose: false,
            collect_stats: false,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(output.docs_embedded, 1);
        assert!(!output.interrupted);

        // Verify document was stored in DB
        let doc = db.get_document("aaa111").unwrap();
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().title, "Title aaa111");
    }

    #[tokio::test]
    async fn test_embedding_phase_multiple_docs() {
        let (db, _tmp) = test_db();
        setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let tmp = tempfile::TempDir::new().unwrap();
        let mut pending = Vec::new();
        for i in 0..3 {
            let path = tmp.path().join(format!("doc{i}.md"));
            let content = format!("# Doc {i}\n\nContent {i}.");
            std::fs::write(&path, &content).unwrap();
            pending.push(make_pending(&format!("id{i:04}"), &content, path));
        }

        let output = run_embedding_phase(EmbeddingPhaseInput {
            pending,
            repo_id: "test",
            embedding: &embedding,
            db: &db,
            chunk_size: 100_000,
            chunk_overlap: 2_000,
            embedding_batch_size: 10,
            show_progress: false,
            verbose: false,
            collect_stats: false,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(output.docs_embedded, 3);
        assert!(!output.interrupted);
    }

    #[tokio::test]
    async fn test_embedding_phase_collects_stats() {
        let (db, _tmp) = test_db();
        setup_repo(&db);
        let embedding = MockEmbedding::new(1024);
        let progress = ProgressReporter::Silent;

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("doc.md");
        std::fs::write(&path, "# Doc\n\nContent.").unwrap();

        let pending = vec![make_pending("aaa111", "# Doc\n\nContent.", path)];

        let output = run_embedding_phase(EmbeddingPhaseInput {
            pending,
            repo_id: "test",
            embedding: &embedding,
            db: &db,
            chunk_size: 100_000,
            chunk_overlap: 2_000,
            embedding_batch_size: 10,
            show_progress: false,
            verbose: false,
            collect_stats: true,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(output.docs_embedded, 1);
        assert!(!output.file_timings.is_empty());
    }
}
