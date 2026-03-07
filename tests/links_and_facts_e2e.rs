//! E2E test for link suggestions, store_links, and fact pairs.
//!
//! Tests the complete workflow:
//! 1. Create related documents without manual Links: blocks
//! 2. Scan to index and generate embeddings (including fact embeddings)
//! 3. Get link suggestions for under-linked documents
//! 4. Store suggested links
//! 5. Verify Links: blocks in files and links in DB
//! 6. Get fact pairs for cross-document validation

mod common;

use common::TestContext;
use factbase::{
    error::FactbaseError,
    mcp::tools::{get_link_suggestions, store_links, get_fact_pairs},
    processor::DocumentProcessor,
    scanner::{full_scan, ScanContext, ScanOptions, Scanner},
    EmbeddingProvider, LinkDetector, ProgressReporter,
};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;

/// Deterministic embedding provider for tests.
/// Uses word-frequency bag-of-words approach so documents about similar topics
/// produce similar vectors (unlike pure hash which gives random vectors).
struct TestEmbedding {
    dim: usize,
}

impl TestEmbedding {
    fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl EmbeddingProvider for TestEmbedding {
    fn generate<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, FactbaseError>> + Send + 'a>> {
        Box::pin(async move {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut vec = vec![0.0f32; self.dim];
            // For each word, hash it to a dimension index and increment
            for word in text.split_whitespace() {
                let w = word.to_lowercase();
                let mut hasher = DefaultHasher::new();
                w.hash(&mut hasher);
                let idx = (hasher.finish() as usize) % self.dim;
                vec[idx] += 1.0;
                // Also set a few nearby dimensions for spread
                vec[(idx + 1) % self.dim] += 0.5;
                vec[(idx + 2) % self.dim] += 0.25;
            }
            // Normalize
            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in &mut vec {
                    *v /= norm;
                }
            }
            Ok(vec)
        })
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

/// Create a test context with related space documents.
fn setup_space_docs() -> TestContext {
    TestContext::with_files(
        "space",
        &[
            (
                "earth.md",
                "<!-- factbase:ea0001 -->\n# Earth\n\nEarth is the third planet from the Sun. @t[?]\n\n- Diameter: 12,742 km @t[?]\n- One natural satellite: the Moon @t[?]\n",
            ),
            (
                "jupiter.md",
                "<!-- factbase:ab0002 -->\n# Jupiter\n\nJupiter is the largest planet in the Solar System. @t[?]\n\n- Diameter: 139,820 km @t[?]\n- Jupiter has at least 95 known moons @t[~2024]\n",
            ),
            (
                "saturn.md",
                "<!-- factbase:ca0003 -->\n# Saturn\n\nSaturn is the sixth planet from the Sun, known for its ring system. @t[?]\n\n- Diameter: 116,460 km @t[?]\n- Saturn has at least 146 known moons @t[~2024]\n",
            ),
            (
                "the-moon.md",
                "<!-- factbase:de0004 -->\n# The Moon\n\nThe Moon is Earth's only natural satellite. @t[?]\n\n- Diameter: 3,474 km @t[?]\n- Distance from Earth: 384,400 km @t[?]\n",
            ),
            (
                "europa.md",
                "<!-- factbase:ef0005 -->\n# Europa\n\nEuropa is one of Jupiter's Galilean moons. @t[?]\n\n- Diameter: 3,121 km @t[?]\n- Europa has a subsurface ocean @t[~2024]\n",
            ),
            (
                "titan.md",
                "<!-- factbase:fa0006 -->\n# Titan\n\nTitan is the largest moon of Saturn. @t[?]\n\n- Diameter: 5,149 km @t[?]\n- Titan has lakes of liquid methane @t[~2024]\n",
            ),
        ],
    )
}

/// Scan the repo with TestEmbedding (deterministic, no external deps).
async fn scan_repo(ctx: &TestContext) -> factbase::ScanResult {
    let embedding = TestEmbedding::new(1024);
    let scanner = Scanner::new(&[]);
    let processor = DocumentProcessor::new();
    let link_detector = LinkDetector::new();
    let opts = ScanOptions::default();
    let progress = ProgressReporter::Silent;

    let scan_ctx = ScanContext {
        scanner: &scanner,
        processor: &processor,
        embedding: &embedding,
        link_detector: &link_detector,
        opts: &opts,
        progress: &progress,
    };

    full_scan(&ctx.repo, &ctx.db, &scan_ctx).await.unwrap()
}

#[tokio::test]
async fn test_scan_generates_fact_embeddings() {
    let ctx = setup_space_docs();
    let result = scan_repo(&ctx).await;

    assert_eq!(result.added, 6, "Should index 6 documents");
    assert!(
        result.fact_embeddings_generated > 0,
        "Scan should generate fact embeddings, got {}",
        result.fact_embeddings_generated
    );
    assert_eq!(
        result.fact_embeddings_needed, 0,
        "No fact embeddings should be deferred"
    );

    // Verify fact embeddings exist in DB
    let count = ctx.db.get_fact_embedding_count().unwrap();
    assert!(count > 0, "DB should have fact embeddings, got {}", count);
}

#[tokio::test]
async fn test_link_suggestions_for_underlinked_docs() {
    let ctx = setup_space_docs();
    scan_repo(&ctx).await;

    let embedding = TestEmbedding::new(1024);
    let args = json!({
        "max_existing_links": 10,
        "min_similarity": 0.01
    });

    let result = get_link_suggestions(&ctx.db, &embedding, &args).await.unwrap();
    let suggestions = result["suggestions"].as_array().unwrap();

    assert!(
        !suggestions.is_empty(),
        "Should have link suggestions for under-linked docs"
    );

    // Each suggestion should have candidates
    for suggestion in suggestions {
        let candidates = suggestion["candidates"].as_array().unwrap();
        assert!(
            !candidates.is_empty(),
            "Each suggestion should have at least one candidate"
        );
        // Candidates should have required fields
        for candidate in candidates {
            assert!(candidate["id"].is_string());
            assert!(candidate["title"].is_string());
            assert!(candidate["similarity"].is_number());
        }
    }
}

#[tokio::test]
async fn test_store_links_writes_to_files_and_db() {
    let ctx = setup_space_docs();
    scan_repo(&ctx).await;

    // Store links between documents
    // Note: some links may already exist from automatic link detection during scan
    let args = json!({
        "links": [
            {"source_id": "de0004", "target_id": "ef0005"},
            {"source_id": "fa0006", "target_id": "ca0003"}
        ]
    });

    let result = store_links(&ctx.db, &args).unwrap();
    let added = result["added"].as_u64().unwrap();
    let modified = result["documents_modified"].as_u64().unwrap();
    assert!(added >= 1, "Should add at least 1 link, got {}", added);
    assert!(modified >= 1, "Should modify at least 1 document, got {}", modified);

    // Verify Links: block was added to at least one file
    let moon_content = std::fs::read_to_string(ctx.repo_path.join("the-moon.md")).unwrap();
    let titan_content = std::fs::read_to_string(ctx.repo_path.join("titan.md")).unwrap();
    let has_links = moon_content.contains("Links:") || titan_content.contains("Links:");
    assert!(has_links, "At least one file should have a Links: block");

    // Verify links in DB — check that the target docs are reachable
    let moon_links = ctx.db.get_links_from("de0004").unwrap();
    let titan_links = ctx.db.get_links_from("fa0006").unwrap();
    assert!(
        !moon_links.is_empty() || !titan_links.is_empty(),
        "At least one document should have outgoing links in DB"
    );
}

#[tokio::test]
async fn test_store_links_skips_existing() {
    let ctx = setup_space_docs();
    scan_repo(&ctx).await;

    let args = json!({
        "links": [{"source_id": "de0004", "target_id": "ef0005"}]
    });

    // Store once
    let r1 = store_links(&ctx.db, &args).unwrap();
    assert_eq!(r1["added"], 1);

    // Store again — should skip
    let r2 = store_links(&ctx.db, &args).unwrap();
    assert_eq!(r2["added"], 0);
    assert_eq!(r2["skipped_existing"], 1);
}

#[tokio::test]
async fn test_fact_pairs_returns_cross_document_pairs() {
    let ctx = setup_space_docs();
    scan_repo(&ctx).await;

    let args = json!({
        "min_similarity": 0.01,
        "limit": 50
    });

    let result = get_fact_pairs(&ctx.db, &args).unwrap();

    let total = result["total_fact_embeddings"].as_u64().unwrap();
    assert!(total > 0, "Should have fact embeddings, got {}", total);

    let pairs = result["pairs"].as_array().unwrap();
    assert!(
        !pairs.is_empty(),
        "Should have fact pairs with similarity > 0.01"
    );

    // Verify pair structure
    for pair in pairs {
        let similarity = pair["similarity"].as_f64().unwrap();
        assert!(similarity > 0.0, "Similarity should be positive");

        let fact_a = &pair["fact_a"];
        let fact_b = &pair["fact_b"];

        assert!(fact_a["doc_id"].is_string());
        assert!(fact_a["doc_title"].is_string());
        assert!(fact_a["text"].is_string());
        assert!(fact_b["doc_id"].is_string());
        assert!(fact_b["doc_title"].is_string());
        assert!(fact_b["text"].is_string());

        // Facts should be from different documents
        assert_ne!(
            fact_a["doc_id"], fact_b["doc_id"],
            "Fact pairs should be cross-document"
        );
    }
}

#[tokio::test]
async fn test_rescan_does_not_duplicate_fact_embeddings() {
    let ctx = setup_space_docs();

    // First scan
    let r1 = scan_repo(&ctx).await;
    let count1 = ctx.db.get_fact_embedding_count().unwrap();
    assert!(r1.fact_embeddings_generated > 0);

    // Second scan (no changes)
    let _r2 = scan_repo(&ctx).await;
    let count2 = ctx.db.get_fact_embedding_count().unwrap();

    // Fact embedding count should not grow on unchanged rescan
    assert_eq!(
        count1, count2,
        "Fact embedding count should be stable across rescans ({} vs {})",
        count1, count2
    );
}
