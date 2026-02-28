//! Fact-level embedding generation phase for document scanning.
//!
//! Extracts individual facts from changed documents and generates
//! per-fact embeddings for cross-document validation.

use std::collections::HashSet;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::progress::ProgressReporter;
use crate::question_generator::facts::extract_all_facts;
use crate::{Database, EmbeddingProvider};

/// Input for the fact embedding phase.
pub struct FactEmbeddingInput<'a> {
    pub changed_ids: &'a HashSet<String>,
    pub embedding: &'a dyn EmbeddingProvider,
    pub db: &'a Database,
    pub embedding_batch_size: usize,
    pub progress: &'a ProgressReporter,
}

/// Run fact embedding generation for changed documents.
///
/// For each changed document, extracts facts, skips those with unchanged
/// hashes, and generates embeddings for new/modified facts.
pub async fn run_fact_embedding_phase(input: &FactEmbeddingInput<'_>) -> anyhow::Result<usize> {
    if input.changed_ids.is_empty() {
        return Ok(0);
    }

    let mut total_generated = 0usize;

    for doc_id in input.changed_ids {
        let doc = match input.db.get_document(doc_id)? {
            Some(d) => d,
            None => continue,
        };

        // Delete old fact embeddings — we'll re-insert current ones
        input.db.delete_fact_embeddings_for_doc(doc_id)?;

        let facts = extract_all_facts(&doc.content);
        if facts.is_empty() {
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
                return Ok(total_generated);
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
    }

    if total_generated > 0 {
        input
            .progress
            .log(&format!("{total_generated} fact embeddings generated"));
    }

    Ok(total_generated)
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

        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Fact one\n- Fact two\n- Fact three\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let count = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(count, 3);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 3);
    }

    #[tokio::test]
    async fn test_fact_embedding_skips_empty_docs() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let content = "<!-- factbase:abc123 -->\n# Test\n\nNo bullet points here.\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let count = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(count, 0);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 0);
    }

    #[tokio::test]
    async fn test_fact_embedding_replaces_on_rescan() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // First scan
        let content1 = "<!-- factbase:abc123 -->\n# Test\n\n- Old fact\n";
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
        })
        .await
        .unwrap();

        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 1);

        // Rescan with different content
        let content2 = "<!-- factbase:abc123 -->\n# Test\n\n- New fact A\n- New fact B\n";
        db.upsert_document(&make_doc("abc123", content2)).unwrap();

        let count = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(count, 2);
        assert_eq!(db.get_fact_embedding_count_for_doc("abc123").unwrap(), 2);
    }

    #[tokio::test]
    async fn test_fact_embedding_excludes_review_queue() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        let content = "<!-- factbase:abc123 -->\n# Test\n\n- Real fact\n\n## Review Queue\n\n- @q[temporal] Not a fact\n";
        db.upsert_document(&make_doc("abc123", content)).unwrap();

        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = ["abc123".to_string()].into();
        let progress = ProgressReporter::Silent;

        let count = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_fact_embedding_empty_changed_ids() {
        let (db, _tmp) = test_db();
        let embedding = MockEmbedding::new(1024);
        let changed: HashSet<String> = HashSet::new();
        let progress = ProgressReporter::Silent;

        let count = run_fact_embedding_phase(&FactEmbeddingInput {
            changed_ids: &changed,
            embedding: &embedding,
            db: &db,
            embedding_batch_size: 10,
            progress: &progress,
        })
        .await
        .unwrap();

        assert_eq!(count, 0);
    }
}
