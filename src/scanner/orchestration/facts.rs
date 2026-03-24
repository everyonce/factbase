//! Fact-level embedding generation phase for document scanning.
//!
//! Extracts individual facts from changed documents and generates
//! per-fact embeddings for cross-document validation.

use std::collections::HashSet;
use std::time::Instant;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::progress::ProgressReporter;
use crate::question_generator::facts::extract_all_facts;
use crate::{Database, EmbeddingProvider};

/// Minimum number of documents before emitting interval progress messages.
const PROGRESS_THRESHOLD: usize = 50;
/// Emit a progress line every N documents processed.
const PROGRESS_INTERVAL_DOCS: usize = 50;
/// Also emit a progress line if this many seconds have elapsed since the last one.
const PROGRESS_INTERVAL_SECS: u64 = 5;

/// Input for the fact embedding phase.
pub struct FactEmbeddingInput<'a> {
    pub changed_ids: &'a HashSet<String>,
    pub embedding: &'a dyn EmbeddingProvider,
    pub db: &'a Database,
    pub embedding_batch_size: usize,
    pub progress: &'a ProgressReporter,
    pub deadline: Option<std::time::Instant>,
}

/// Output from the fact embedding phase.
#[derive(Debug, PartialEq)]
pub struct FactEmbeddingOutput {
    pub generated: usize,
    pub docs_processed: usize,
}

/// Run fact embedding generation for changed documents.
///
/// For each changed document, extracts facts, skips those with unchanged
/// hashes, and generates embeddings for new/modified facts.
/// Respects an optional deadline — returns early if time runs out.
///
/// Emits granular progress via `input.progress` for repos with ≥50 documents.
pub async fn run_fact_embedding_phase(
    input: &FactEmbeddingInput<'_>,
) -> anyhow::Result<FactEmbeddingOutput> {
    if input.changed_ids.is_empty() {
        return Ok(FactEmbeddingOutput {
            generated: 0,
            docs_processed: 0,
        });
    }

    let total_docs = input.changed_ids.len();
    let start = Instant::now();
    let large_repo = total_docs >= PROGRESS_THRESHOLD;

    if large_repo {
        input.progress.log(&format!(
            "Generating fact embeddings for {total_docs} documents..."
        ));
    }

    let mut total_generated = 0usize;
    let mut docs_processed = 0usize;
    let mut last_progress = Instant::now();

    for doc_id in input.changed_ids {
        // Check deadline before processing each document
        if let Some(dl) = input.deadline {
            if Instant::now() > dl {
                break;
            }
        }

        let doc = match input.db.get_document(doc_id)? {
            Some(d) => d,
            None => continue,
        };

        // Delete old fact embeddings — we'll re-insert current ones
        input.db.delete_fact_embeddings_for_doc(doc_id)?;

        let facts = extract_all_facts(&doc.content);
        if facts.is_empty() {
            docs_processed += 1;
            continue;
        }

        // Collect facts with their hashes and IDs
        let mut texts: Vec<&str> = Vec::with_capacity(facts.len());
        let mut meta: Vec<(String, usize, String, String)> = Vec::with_capacity(facts.len());

        for fact in &facts {
            let fact_id = format!("{}_{}", doc_id, fact.line_number);
            let fact_hash = hex::encode(Sha256::digest(fact.text.as_bytes()));
            texts.push(&fact.text);
            meta.push((fact_id, fact.line_number, fact.text.clone(), fact_hash));
        }

        // Generate embeddings in batches
        for (batch_texts, batch_meta) in texts
            .chunks(input.embedding_batch_size)
            .zip(meta.chunks(input.embedding_batch_size))
        {
            if crate::shutdown::is_shutdown_requested() {
                return Ok(FactEmbeddingOutput {
                    generated: total_generated,
                    docs_processed,
                });
            }

            let embeddings = input.embedding.generate_batch(batch_texts).await?;

            for (emb, (fact_id, line_number, fact_text, fact_hash)) in
                embeddings.into_iter().zip(batch_meta.iter())
            {
                input.db.upsert_fact_embedding(
                    fact_id,
                    doc_id,
                    *line_number,
                    fact_text,
                    fact_hash,
                    &emb,
                )?;
                total_generated += 1;
            }
        }

        debug!(
            doc_id,
            facts = facts.len(),
            "Generated fact embeddings for document"
        );

        docs_processed += 1;

        // Emit interval progress for large repos
        if large_repo {
            let since_last = last_progress.elapsed().as_secs();
            if docs_processed.is_multiple_of(PROGRESS_INTERVAL_DOCS)
                || since_last >= PROGRESS_INTERVAL_SECS
            {
                input.progress.report(
                    docs_processed,
                    total_docs,
                    "Generating Fact Embeddings",
                );
                last_progress = Instant::now();
            }
        }
    }

    let elapsed_secs = start.elapsed().as_secs();
    if total_generated > 0 {
        input.progress.log(&format!(
            "Fact embeddings complete: {docs_processed} documents, {total_generated} facts in {elapsed_secs}s"
        ));
    }

    Ok(FactEmbeddingOutput {
        generated: total_generated,
        docs_processed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::{test_db, test_repo};
    use crate::embedding::test_helpers::MockEmbedding;
    use crate::models::Document;
    use chrono::Utc;

    fn make_doc(id: &str, content: &str) -> Document {
        Document {
            id: id.to_string(),
            repo_id: "test-repo".to_string(),
            file_path: format!("{id}.md"),
            file_hash: "hash".to_string(),
            title: id.to_string(),
            doc_type: Some("note".to_string()),
            content: content.to_string(),
            file_modified_at: Some(Utc::now()),
            indexed_at: Utc::now(),
            is_deleted: false,
        }
    }

    #[tokio::test]
    async fn test_fact_embedding_generation() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let content =
            "---\nfactbase_id: abc123\n---\n# Test\n\n- Fact one\n- Fact two\n- Fact three\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let result = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(result.generated, 3);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 3);
    }

    #[tokio::test]
    async fn test_fact_embedding_skips_empty_docs() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let content = "---\nfactbase_id: abc123\n---\n# Test\n\nNo bullet points here.\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let result = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(result.generated, 0);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 0);
    }

    #[tokio::test]
    async fn test_fact_embedding_replaces_on_rescan() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // First scan
        let content1 = "---\nfactbase_id: abc123\n---\n# Test\n\n- Old fact\n";
        db.upsert_document(&make_doc("abc123", content1)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 1);

        // Rescan with different content
        let content2 = "---\nfactbase_id: abc123\n---\n# Test\n\n- New fact A\n- New fact B\n";
        db.upsert_document(&make_doc("abc123", content2)).unwrap();

        let result = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(result.generated, 2);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 2);
    }

    #[tokio::test]
    async fn test_fact_embedding_excludes_review_queue() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let content = "---\nfactbase_id: abc123\n---\n# Test\n\n- Real fact\n\n## Review Queue\n\n- @q[temporal] Not a fact\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let result = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(result.generated, 1);
    }

    #[tokio::test]
    async fn test_fact_embedding_empty_changed_ids() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = HashSet::new();
        let progress = ProgressReporter::Silent;

        let result = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        assert_eq!(
            result,
            FactEmbeddingOutput {
                generated: 0,
                docs_processed: 0
            }
        );
    }

    /// Small repos (<50 docs) should not emit start/interval progress messages.
    #[tokio::test]
    async fn test_small_repo_no_interval_progress() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = ProgressReporter::Mcp { sender: Some(tx) };
        let embedding = MockEmbedding::new(1024);

        // Insert 5 docs (below PROGRESS_THRESHOLD)
        let mut changed = HashSet::new();
        for i in 0..5 {
            let id = format!("sm{i:04}");
            let content = format!("---\nfactbase_id: {id}\n---\n# Doc {i}\n\n- Fact {i}\n");
            db.upsert_document(&make_doc(&id, &content)).unwrap();
            changed.insert(id);
        }

        run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        // Collect all messages
        let mut messages: Vec<serde_json::Value> = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        // No "progress" (interval) messages for small repos
        let has_interval = messages.iter().any(|m| m.get("progress").is_some());
        assert!(
            !has_interval,
            "small repos should not emit interval progress"
        );

        // Completion log should still appear (total_generated > 0)
        let has_completion = messages.iter().any(|m| {
            m.get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("Fact embeddings complete"))
                .unwrap_or(false)
        });
        assert!(
            has_completion,
            "completion log should always appear when facts were generated"
        );
    }

    /// Large repos (≥50 docs) should emit start log and interval progress messages.
    #[tokio::test]
    async fn test_large_repo_emits_progress() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = ProgressReporter::Mcp { sender: Some(tx) };
        let embedding = MockEmbedding::new(1024);

        // Insert PROGRESS_THRESHOLD docs (exactly at threshold)
        let mut changed = HashSet::new();
        for i in 0..PROGRESS_THRESHOLD {
            let id = format!("lg{i:04}");
            let content = format!("---\nfactbase_id: {id}\n---\n# Doc {i}\n\n- Fact {i}\n");
            db.upsert_document(&make_doc(&id, &content)).unwrap();
            changed.insert(id);
        }

        run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
            deadline: None,
        })
        .await
        .unwrap();

        let mut messages: Vec<serde_json::Value> = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        // Start message should appear
        let has_start = messages.iter().any(|m| {
            m.get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("Generating fact embeddings for"))
                .unwrap_or(false)
        });
        assert!(has_start, "large repos should emit start message");

        // At least one interval progress message should appear
        let has_interval = messages.iter().any(|m| m.get("progress").is_some());
        assert!(has_interval, "large repos should emit interval progress");

        // Completion log should appear
        let has_completion = messages.iter().any(|m| {
            m.get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.contains("Fact embeddings complete"))
                .unwrap_or(false)
        });
        assert!(has_completion, "large repos should emit completion log");
    }
}
