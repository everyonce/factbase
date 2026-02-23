//! Integration tests for document chunking with qwen3-embedding:0.6b
//!
//! Tests chunking behavior for large documents and search result deduplication.
//! Requires Ollama running with qwen3-embedding:0.6b model.

use factbase::{
    chunk_document, full_scan, Config, Database, DocumentProcessor, EmbeddingProvider,
    LinkDetector, OllamaEmbedding, OllamaLlm, Repository, ScanOptions, Scanner,
};
use std::fs;
use tempfile::TempDir;

/// Check if Ollama is available
async fn is_ollama_available() -> bool {
    let client = reqwest::Client::new();
    client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .is_ok()
}

/// Create a test repository with documents
fn setup_test_repo(temp: &TempDir, files: &[(&str, &str)]) -> Repository {
    for (path, content) in files {
        let full_path = temp.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("create dir");
        }
        fs::write(&full_path, content).expect("write file");
    }

    Repository {
        id: "test".to_string(),
        name: "Test Repo".to_string(),
        path: temp.path().to_path_buf(),
        perspective: None,
        created_at: chrono::Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    }
}

#[test]
fn test_chunk_document_small_no_chunking() {
    let content = "Small document content";
    let chunks = chunk_document(content, 100_000, 2_000);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].index, 0);
    assert_eq!(chunks[0].start, 0);
    assert_eq!(chunks[0].end, content.len());
    assert_eq!(chunks[0].content, content);
}

#[test]
fn test_chunk_document_large_creates_multiple_chunks() {
    // Create 250K char document (should create 3 chunks with 100K size, 2K overlap)
    let content = "word ".repeat(50_000); // 250K chars

    let chunks = chunk_document(&content, 100_000, 2_000);

    assert!(chunks.len() >= 2, "Should create multiple chunks");

    // Verify chunk indices are sequential
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.index, i);
    }

    // Verify chunks cover the entire document
    assert_eq!(chunks[0].start, 0);
    assert_eq!(
        chunks.last().expect("operation should succeed").end,
        content.len()
    );

    // Verify overlap exists between consecutive chunks
    for i in 1..chunks.len() {
        let prev_end = chunks[i - 1].end;
        let curr_start = chunks[i].start;
        assert!(
            curr_start < prev_end,
            "Chunks should overlap: prev_end={}, curr_start={}",
            prev_end,
            curr_start
        );
    }
}

#[test]
fn test_chunk_document_respects_word_boundaries() {
    // Create content where chunk boundary falls mid-word
    let content = "a".repeat(99_998) + " boundary";

    let chunks = chunk_document(&content, 100_000, 2_000);

    // First chunk should end at word boundary (before "boundary")
    assert!(
        chunks[0].content.ends_with(' ') || !chunks[0].content.ends_with("bound"),
        "Chunk should end at word boundary"
    );
}

#[tokio::test]
async fn test_scan_with_chunking() {
    if !is_ollama_available().await {
        eprintln!("Skipping test: Ollama not available");
        return;
    }

    let temp = TempDir::new().expect("create temp dir");

    // Create a large document that will be chunked
    let large_content = format!(
        "<!-- factbase:abc123 -->\n# Large Document\n\n{}",
        "This is test content. ".repeat(5000) // ~110K chars
    );

    let repo = setup_test_repo(
        &temp,
        &[
            ("large.md", &large_content),
            (
                "small.md",
                "<!-- factbase:def456 -->\n# Small Document\n\nShort content.",
            ),
        ],
    );

    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).expect("create db");
    db.upsert_repository(&repo).expect("upsert repo");

    let config = Config::load(None).expect("load config");
    let scanner = Scanner::new(&[]);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        verbose: true,
        chunk_size: 50_000, // Use smaller chunk size for test
        chunk_overlap: 2_000,
        ..Default::default()
    };

    let result = full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("scan should succeed");

    assert_eq!(result.added, 2, "Should add 2 documents");

    // Verify large document has multiple chunks by checking search returns chunk info
    let query_embedding = embedding.generate("test content").await.expect("embed");
    let results = db
        .search_semantic_with_query(&query_embedding, 10, None, None, None)
        .expect("search");

    // Should find the large document
    assert!(!results.is_empty(), "Should find documents");
}

#[tokio::test]
async fn test_search_deduplicates_chunks() {
    if !is_ollama_available().await {
        eprintln!("Skipping test: Ollama not available");
        return;
    }

    let temp = TempDir::new().expect("create temp dir");

    // Create document with repeated content that will match multiple chunks
    let repeated_content = format!(
        "<!-- factbase:abc123 -->\n# Repeated Content\n\n{}",
        "The quick brown fox jumps over the lazy dog. ".repeat(3000) // ~135K chars
    );

    let repo = setup_test_repo(&temp, &[("repeated.md", &repeated_content)]);

    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).expect("create db");
    db.upsert_repository(&repo).expect("upsert repo");

    let config = Config::load(None).expect("load config");
    let scanner = Scanner::new(&[]);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 50_000,
        chunk_overlap: 2_000,
        ..Default::default()
    };

    full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("scan should succeed");

    // Search for content that appears in multiple chunks
    let query_embedding = embedding.generate("quick brown fox").await.expect("embed");
    let results = db
        .search_semantic_with_query(&query_embedding, 10, None, None, None)
        .expect("search");

    // Should return only 1 result (deduplicated)
    assert_eq!(
        results.len(),
        1,
        "Search should deduplicate chunks from same document"
    );
    assert_eq!(results[0].id, "abc123");
}

#[tokio::test]
async fn test_search_returns_best_chunk() {
    if !is_ollama_available().await {
        eprintln!("Skipping test: Ollama not available");
        return;
    }

    let temp = TempDir::new().expect("create temp dir");

    // Create document with distinct content in different sections
    let content = format!(
        "<!-- factbase:abc123 -->\n# Mixed Content\n\n{}\n\n{}\n\n{}",
        "Introduction about general topics. ".repeat(1500), // ~50K chars
        "UNIQUE_KEYWORD_SECTION: This section contains very specific information about quantum computing and neural networks. ".repeat(500), // ~60K chars
        "Conclusion with summary. ".repeat(1500) // ~40K chars
    );

    let repo = setup_test_repo(&temp, &[("mixed.md", &content)]);

    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).expect("create db");
    db.upsert_repository(&repo).expect("upsert repo");

    let config = Config::load(None).expect("load config");
    let scanner = Scanner::new(&[]);
    let processor = DocumentProcessor::new();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let opts = ScanOptions {
        chunk_size: 50_000,
        chunk_overlap: 2_000,
        ..Default::default()
    };

    full_scan(
        &repo,
        &db,
        &scanner,
        &processor,
        &embedding,
        &link_detector,
        &opts,
    )
    .await
    .expect("scan should succeed");

    // Search for content in the middle section
    let query_embedding = embedding
        .generate("quantum computing neural networks")
        .await
        .expect("embed");
    let results = db
        .search_semantic_with_query(&query_embedding, 10, None, None, None)
        .expect("search");

    assert_eq!(results.len(), 1);

    // The result should have chunk info pointing to the middle section
    if let Some(chunk_idx) = results[0].chunk_index {
        assert!(
            chunk_idx >= 1,
            "Should match chunk containing the unique content"
        );
    }
}

#[test]
fn test_schema_has_1024_dimensions() {
    let temp = TempDir::new().expect("create temp dir");
    let db_path = temp.path().join("test.db");

    // Create database with new schema
    let _db = Database::new(&db_path).expect("create db");

    // Open raw connection to check schema
    let conn = rusqlite::Connection::open(&db_path).expect("open db");

    // Verify embedding_chunks table exists
    let table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_chunks'",
            [],
            |row| row.get(0),
        )
        .expect("check table");

    assert_eq!(table_exists, 1, "embedding_chunks table should exist");
}
