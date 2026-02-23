//! Semantic search operations using sqlite-vec.
//!
//! Provides vector similarity search with cosine distance.
//! Results are deduplicated by document (best chunk wins).

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use super::{append_type_repo_filters, generate_snippet, push_type_repo_params};
use crate::database::{decode_content_lossy, Database};
use crate::error::FactbaseError;
use crate::models::{PaginatedSearchResult, SearchResult};
use zerocopy::IntoBytes;

impl Database {
    /// Find documents similar to the given document (excluding itself)
    pub fn find_similar_documents(
        &self,
        doc_id: &str,
        threshold: f32,
    ) -> Result<Vec<(String, String, f32)>, FactbaseError> {
        let conn = self.get_conn()?;

        // Get the embedding for the source document (first chunk)
        let chunk_id = format!("{}_0", doc_id);
        let embedding: Vec<u8> = conn.query_row(
            "SELECT embedding FROM document_embeddings WHERE id = ?1",
            [&chunk_id],
            |r| r.get(0),
        )?;

        // Find similar documents using KNN search
        // sqlite-vec requires k parameter in the MATCH clause
        let mut stmt = conn.prepare_cached(
            "SELECT c.document_id, d.title, e.distance
             FROM document_embeddings e
             JOIN embedding_chunks c ON e.id = c.id
             JOIN documents d ON c.document_id = d.id
             WHERE d.is_deleted = FALSE
             AND e.embedding MATCH ?1 AND k = 20
             ORDER BY e.distance",
        )?;

        let max_distance = 1.0 - threshold;
        let mut results = Vec::with_capacity(10);
        let mut seen_docs = HashSet::new();
        let mut rows = stmt.query(rusqlite::params![embedding])?;
        while let Some(row) = rows.next()? {
            let similar_id: String = row.get(0)?;
            // Skip self and already seen docs (dedup chunks)
            if similar_id == doc_id || seen_docs.contains(&similar_id) {
                continue;
            }
            let distance: f32 = row.get(2)?;
            // Only include if above threshold
            if distance < max_distance {
                let similar_title: String = row.get(1)?;
                let similarity = 1.0 - distance;
                results.push((similar_id.clone(), similar_title, similarity));
                seen_docs.insert(similar_id);
            }
        }

        Ok(results)
    }

    /// Performs semantic search using vector similarity.
    ///
    /// Finds documents most similar to the provided embedding vector.
    /// Results are deduplicated by document (best chunk wins).
    /// Pass `query` for snippet highlighting, or `None` for distance-based snippets.
    pub fn search_semantic_with_query(
        &self,
        embedding: &[f32],
        limit: usize,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
        query: Option<&str>,
    ) -> Result<Vec<SearchResult>, FactbaseError> {
        let result =
            self.search_semantic_paginated(embedding, limit, 0, doc_type, repo_id, query)?;
        Ok(result.results)
    }

    /// Performs paginated semantic search with full result metadata.
    pub fn search_semantic_paginated(
        &self,
        embedding: &[f32],
        limit: usize,
        offset: usize,
        doc_type: Option<&str>,
        repo_id: Option<&str>,
        query: Option<&str>,
    ) -> Result<PaginatedSearchResult, FactbaseError> {
        let conn = self.get_conn()?;

        // First, get total count of unique documents
        let mut count_sql = String::from(
            "SELECT COUNT(DISTINCT c.document_id)
             FROM document_embeddings e
             JOIN embedding_chunks c ON e.id = c.id
             JOIN documents d ON c.document_id = d.id
             WHERE d.is_deleted = FALSE",
        );

        append_type_repo_filters(&mut count_sql, 1, doc_type, repo_id, "d.");

        let mut count_params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2);
        push_type_repo_params(&mut count_params, &doc_type, &repo_id);

        let total_count: usize =
            conn.query_row(&count_sql, count_params.as_slice(), |row| row.get(0))?;

        // Search with KNN, then deduplicate by document
        let fetch_limit = (limit + offset) * 3;
        let mut sql = String::from(
            "SELECT c.document_id, d.title, d.doc_type, d.file_path, d.content, e.distance,
                    c.chunk_index, c.chunk_start, c.chunk_end
             FROM document_embeddings e
             JOIN embedding_chunks c ON e.id = c.id
             JOIN documents d ON c.document_id = d.id
             WHERE d.is_deleted = FALSE
             AND e.embedding MATCH ?1
             AND k = ?2",
        );

        append_type_repo_filters(&mut sql, 3, doc_type, repo_id, "d.");

        sql.push_str(" ORDER BY e.distance");

        let mut stmt = conn.prepare_cached(&sql)?;

        // Collect results, keeping best chunk per document
        let mut doc_results: HashMap<String, SearchResult> = HashMap::new();

        let fetch_limit_i32 = fetch_limit as i32;
        let embedding_bytes = embedding.as_bytes();
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&embedding_bytes, &fetch_limit_i32];
        push_type_repo_params(&mut params, &doc_type, &repo_id);

        let mut process_rows = |params: &[&dyn rusqlite::ToSql]| -> Result<(), FactbaseError> {
            let mut rows = stmt.query(params)?;
            while let Some(row) = rows.next()? {
                let doc_id: String = row.get(0)?;
                let distance: f32 = row.get(5)?;

                // Only keep best (lowest distance) chunk per document
                if let Some(existing) = doc_results.get(&doc_id) {
                    if existing.relevance_score >= 1.0 - distance {
                        continue;
                    }
                }

                let result = Self::row_to_search_result_with_chunk(row, query)?;
                doc_results.insert(doc_id, result);
            }
            Ok(())
        };

        process_rows(params.as_slice())?;

        // Sort by relevance and apply pagination
        let mut results: Vec<SearchResult> = doc_results.into_values().collect();
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let before_dedup = results.len();

        // Deduplicate by content hash (snippet hash as proxy)
        let mut seen_hashes: HashSet<u64> = HashSet::new();
        results.retain(|r| {
            let mut hasher = DefaultHasher::new();
            r.snippet.hash(&mut hasher);
            let hash = hasher.finish();
            seen_hashes.insert(hash)
        });

        let deduplicated_count = before_dedup.saturating_sub(results.len());

        let paginated: Vec<SearchResult> = results.into_iter().skip(offset).take(limit).collect();

        Ok(PaginatedSearchResult {
            results: paginated,
            total_count,
            offset,
            limit,
            deduplicated_count: if deduplicated_count > 0 {
                Some(deduplicated_count)
            } else {
                None
            },
        })
    }

    fn row_to_search_result_with_chunk(
        row: &rusqlite::Row,
        query: Option<&str>,
    ) -> Result<SearchResult, FactbaseError> {
        let stored_content: String = row.get(4)?;
        let content = decode_content_lossy(stored_content);
        let distance: f32 = row.get(5)?;
        let chunk_index: i64 = row.get(6)?;
        let chunk_start: i64 = row.get(7)?;
        let chunk_end: i64 = row.get(8)?;

        // Generate snippet from the matching chunk region
        let snippet_content = if chunk_end > chunk_start && (chunk_start as usize) < content.len() {
            let start = chunk_start as usize;
            let end = (chunk_end as usize).min(content.len());
            &content[start..end]
        } else {
            &content
        };

        let snippet = generate_snippet(snippet_content);

        let highlighted_snippet = query.map(|q| Self::highlight_terms(&snippet, q));

        Ok(SearchResult {
            id: row.get(0)?,
            title: row.get(1)?,
            doc_type: row.get(2).ok(),
            file_path: row.get(3)?,
            snippet,
            highlighted_snippet,
            relevance_score: 1.0 - distance,
            chunk_index: if chunk_index > 0 {
                Some(chunk_index as u32)
            } else {
                None
            },
            chunk_start: if chunk_start > 0 {
                Some(chunk_start as u32)
            } else {
                None
            },
            chunk_end: if chunk_end > 0 {
                Some(chunk_end as u32)
            } else {
                None
            },
        })
    }

    pub(crate) fn highlight_terms(text: &str, query: &str) -> String {
        let terms: Vec<&str> = query.split_whitespace().filter(|t| t.len() >= 2).collect();

        if terms.is_empty() {
            return text.to_string();
        }

        // Lowercase once
        let lower_text = text.to_lowercase();

        // Collect all match ranges (start, end) across all terms
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for term in &terms {
            let lower_term = term.to_lowercase();
            let mut i = 0;
            while i + lower_term.len() <= lower_text.len() {
                if let Some(pos) = lower_text[i..].find(&lower_term) {
                    let start = i + pos;
                    ranges.push((start, start + lower_term.len()));
                    i = start + lower_term.len();
                } else {
                    break;
                }
            }
        }

        if ranges.is_empty() {
            return text.to_string();
        }

        // Sort and merge overlapping ranges
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = vec![ranges[0]];
        for &(start, end) in &ranges[1..] {
            let last = merged.last_mut().expect("non-empty after initial push");
            if start <= last.1 {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }

        // Build result in a single pass, preserving original casing
        let mut result = String::with_capacity(text.len() + merged.len() * 4);
        let mut pos = 0;
        for (start, end) in merged {
            result.push_str(&text[pos..start]);
            result.push_str("**");
            result.push_str(&text[start..end]);
            result.push_str("**");
            pos = end;
        }
        result.push_str(&text[pos..]);
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::database::tests::{test_db, test_doc, test_repo};
    use crate::models::PaginatedSearchResult;
    use crate::Database;

    #[test]
    fn test_highlight_terms_single_term() {
        let text = "The quick brown fox jumps over the lazy dog";
        let result = Database::highlight_terms(text, "fox");
        assert_eq!(result, "The quick brown **fox** jumps over the lazy dog");
    }

    #[test]
    fn test_highlight_terms_multiple_terms() {
        let text = "The quick brown fox jumps over the lazy dog";
        let result = Database::highlight_terms(text, "fox dog");
        assert!(result.contains("**fox**"));
        assert!(result.contains("**dog**"));
    }

    #[test]
    fn test_highlight_terms_case_insensitive() {
        let text = "The Quick Brown FOX jumps";
        let result = Database::highlight_terms(text, "fox quick");
        assert!(result.contains("**FOX**"));
        assert!(result.contains("**Quick**"));
    }

    #[test]
    fn test_highlight_terms_no_match() {
        let text = "The quick brown fox";
        let result = Database::highlight_terms(text, "cat");
        assert_eq!(result, text);
    }

    #[test]
    fn test_highlight_terms_short_terms_ignored() {
        let text = "A is a letter";
        let result = Database::highlight_terms(text, "a");
        assert_eq!(result, text); // Single char terms ignored
    }

    #[test]
    fn test_find_similar_documents() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo)
            .expect("upsert_repository should succeed");
        db.upsert_document(&test_doc("doc1", "Doc 1"))
            .expect("upsert doc1 should succeed");
        db.upsert_document(&test_doc("doc2", "Doc 2"))
            .expect("upsert doc2 should succeed");

        // Create similar embeddings (small difference)
        let emb1: Vec<f32> = vec![0.5; 1024];
        let mut emb2: Vec<f32> = vec![0.5; 1024];
        emb2[0] = 0.51; // Very similar

        db.upsert_embedding("doc1", &emb1)
            .expect("upsert_embedding doc1 should succeed");
        db.upsert_embedding("doc2", &emb2)
            .expect("upsert_embedding doc2 should succeed");

        // Find similar with high threshold
        let similar = db
            .find_similar_documents("doc1", 0.95)
            .expect("find_similar_documents should succeed");

        // Should find doc2 as similar
        assert!(!similar.is_empty(), "should find similar document");
        assert_eq!(similar[0].0, "doc2");
        assert!(similar[0].2 > 0.95, "similarity should be > 0.95");
    }

    #[test]
    fn test_paginated_search_result_struct() {
        let result = PaginatedSearchResult {
            results: vec![],
            total_count: 100,
            offset: 20,
            limit: 10,
            deduplicated_count: None,
        };

        assert_eq!(result.total_count, 100);
        assert_eq!(result.offset, 20);
        assert_eq!(result.limit, 10);
        assert_eq!(result.deduplicated_count, None);
    }

    #[test]
    fn test_content_hash_deduplication() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let snippet1 = "This is some test content";
        let snippet2 = "This is some test content";
        let snippet3 = "Different content here";

        let mut h1 = DefaultHasher::new();
        snippet1.hash(&mut h1);
        let hash1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        snippet2.hash(&mut h2);
        let hash2 = h2.finish();

        let mut h3 = DefaultHasher::new();
        snippet3.hash(&mut h3);
        let hash3 = h3.finish();

        assert_eq!(hash1, hash2, "Same content should have same hash");
        assert_ne!(hash1, hash3, "Different content should have different hash");
    }

    #[test]
    fn test_search_semantic_paginated_offset_limit() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // Create 5 documents with embeddings
        for i in 0..5 {
            let id = format!("doc{}", i);
            db.upsert_document(&test_doc(&id, &format!("Doc {}", i)))
                .unwrap();
            let mut emb: Vec<f32> = vec![0.5; 1024];
            emb[0] = 0.5 + (i as f32 * 0.01); // Slightly different embeddings
            db.upsert_embedding(&id, &emb).unwrap();
        }

        let query_emb: Vec<f32> = vec![0.5; 1024];

        // Test limit
        let result = db
            .search_semantic_paginated(&query_emb, 2, 0, None, None, None)
            .unwrap();
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.limit, 2);
        assert_eq!(result.offset, 0);

        // Test offset
        let result_offset = db
            .search_semantic_paginated(&query_emb, 2, 2, None, None, None)
            .unwrap();
        assert_eq!(result_offset.results.len(), 2);
        assert_eq!(result_offset.offset, 2);

        // Results should be different due to offset
        assert_ne!(result.results[0].id, result_offset.results[0].id);
    }

    #[test]
    fn test_search_semantic_empty_results() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // No documents, search should return empty
        let query_emb: Vec<f32> = vec![0.5; 1024];
        let result = db
            .search_semantic_paginated(&query_emb, 10, 0, None, None, None)
            .unwrap();

        assert!(result.results.is_empty());
        assert_eq!(result.total_count, 0);
    }

    #[test]
    fn test_search_semantic_type_filtering() {
        let (db, _tmp) = test_db();
        let repo = test_repo();
        db.upsert_repository(&repo).unwrap();

        // Create docs with different types
        let mut person_doc = test_doc("person1", "John Doe");
        person_doc.doc_type = Some("person".to_string());
        db.upsert_document(&person_doc).unwrap();
        db.upsert_embedding("person1", &vec![0.5; 1024]).unwrap();

        let mut project_doc = test_doc("project1", "Project Alpha");
        project_doc.doc_type = Some("project".to_string());
        db.upsert_document(&project_doc).unwrap();
        db.upsert_embedding("project1", &vec![0.51; 1024]).unwrap();

        let query_emb: Vec<f32> = vec![0.5; 1024];

        // Filter by type "person"
        let result = db
            .search_semantic_paginated(&query_emb, 10, 0, Some("person"), None, None)
            .unwrap();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].doc_type, Some("person".to_string()));
    }

    #[test]
    fn test_search_semantic_repo_filtering() {
        use crate::database::tests::test_repo_with_id;

        let (db, _tmp) = test_db();

        // Create two repos
        let repo1 = test_repo_with_id("repo1");
        let repo2 = test_repo_with_id("repo2");
        db.upsert_repository(&repo1).unwrap();
        db.upsert_repository(&repo2).unwrap();

        // Create doc in repo1
        let mut doc1 = test_doc("doc1", "Doc in Repo 1");
        doc1.repo_id = "repo1".to_string();
        db.upsert_document(&doc1).unwrap();
        db.upsert_embedding("doc1", &vec![0.5; 1024]).unwrap();

        // Create doc in repo2
        let mut doc2 = test_doc("doc2", "Doc in Repo 2");
        doc2.repo_id = "repo2".to_string();
        db.upsert_document(&doc2).unwrap();
        db.upsert_embedding("doc2", &vec![0.51; 1024]).unwrap();

        let query_emb: Vec<f32> = vec![0.5; 1024];

        // Filter by repo1
        let result = db
            .search_semantic_paginated(&query_emb, 10, 0, None, Some("repo1"), None)
            .unwrap();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].id, "doc1");
    }
}
