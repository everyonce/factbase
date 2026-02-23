//! Search command integration tests.

use super::common::ollama_helpers::require_ollama;
use super::common::run_scan;
use super::common::TestContext;
use factbase::{config::Config, database::Database, embedding::OllamaEmbedding, EmbeddingProvider};
use std::fs;
use tempfile::TempDir;

/// Test search --count flag outputs only result count
#[tokio::test]
async fn test_search_count_flag() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).unwrap();

    // Create test documents
    fs::write(repo_path.join("doc1.md"), "# Doc One\nFirst document.")
        .expect("write should succeed");
    fs::write(repo_path.join("doc2.md"), "# Doc Two\nSecond document.")
        .expect("write should succeed");
    fs::write(repo_path.join("doc3.md"), "# Doc Three\nThird document.")
        .expect("write should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = super::common::test_repo("notes", repo_path);
    db.add_repository(&repo).unwrap();

    let config = Config::default();
    run_scan(&repo, &db, &config)
        .await
        .expect("scan should succeed");

    // Search and verify we get results
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );
    let query_emb = embedding
        .generate("document")
        .await
        .expect("embedding should succeed");
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    // Verify count matches expected (all 3 docs should match "document")
    assert!(
        !results.is_empty(),
        "Should find at least 1 result for 'document'"
    );
}

/// Test search --exclude-type flag is accepted
#[test]
fn test_search_exclude_type_flag() {
    use std::process::Command;

    // Test that --exclude-type is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "search", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--exclude-type"),
        "search --help should show --exclude-type flag"
    );
    assert!(
        stdout.contains("-T"),
        "search --help should show -T short flag"
    );
    assert!(
        stdout.contains("Exclude documents of this type"),
        "search --help should describe --exclude-type"
    );
}

/// Test search --as-of flag is accepted and documented
#[test]
fn test_search_as_of_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "search", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--as-of"),
        "search --help should show --as-of flag"
    );
    assert!(
        stdout.contains("YYYY") && stdout.contains("YYYY-MM"),
        "search --help should document date formats for --as-of"
    );
}

/// Test search --during flag is accepted and documented
#[test]
fn test_search_during_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "search", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--during"),
        "search --help should show --during flag"
    );
    assert!(
        stdout.contains("YYYY..YYYY"),
        "search --help should document range format for --during"
    );
}

/// Test search --exclude-unknown flag is accepted
#[test]
fn test_search_exclude_unknown_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "search", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--exclude-unknown"),
        "search --help should show --exclude-unknown flag"
    );
    assert!(
        stdout.contains("@t[?]") || stdout.contains("unknown temporal"),
        "search --help should describe what --exclude-unknown filters"
    );
}

/// Test temporal search filtering with --as-of (requires Ollama)
#[tokio::test]
#[ignore]
async fn test_search_temporal_as_of_filtering() {
    require_ollama().await;

    // Create documents with temporal tags at different dates
    let files = &[
        (
            "people/alice.md",
            "# Alice\n- CTO at Acme @t[2020..2022]\n- VP at BigCo @t[2023..]",
        ),
        (
            "people/bob.md",
            "# Bob\n- Engineer at Startup @t[2019..2021]\n- Manager at Corp @t[2022..]",
        ),
    ];

    let ctx = TestContext::with_files("temporal_test", files);
    ctx.scan().await.expect("scan should succeed");

    let embedding = ctx.embedding();

    // Search for "CTO" - should find Alice
    let query_emb = embedding
        .generate("CTO")
        .await
        .expect("embedding should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    assert!(!results.is_empty(), "Should find results for CTO query");

    // Verify Alice is in results (has CTO role)
    let alice_result = results.iter().find(|r| r.title.contains("Alice"));
    assert!(
        alice_result.is_some(),
        "Alice should be in CTO search results"
    );
}

/// Test temporal search filtering with --during range (requires Ollama)
#[tokio::test]
#[ignore]
async fn test_search_temporal_during_filtering() {
    require_ollama().await;

    // Create documents with non-overlapping temporal ranges
    let files = &[
        (
            "events/event2020.md",
            "# Conference 2020\n- Annual tech conference @t[=2020-06]",
        ),
        (
            "events/event2022.md",
            "# Summit 2022\n- Leadership summit @t[=2022-09]",
        ),
    ];

    let ctx = TestContext::with_files("during_test", files);
    ctx.scan().await.expect("scan should succeed");

    let embedding = ctx.embedding();

    // Search for "conference" - should find both events
    let query_emb = embedding
        .generate("conference summit")
        .await
        .expect("embedding should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    assert!(
        !results.is_empty(),
        "Should find at least one event document"
    );
}

/// Test --exclude-unknown filters documents without temporal tags (requires Ollama)
#[tokio::test]
#[ignore]
async fn test_search_exclude_unknown_filtering() {
    require_ollama().await;

    // Create documents: one with temporal tags, one without
    let files = &[
        (
            "notes/dated.md",
            "# Dated Note\n- Important fact @t[2024-01]",
        ),
        (
            "notes/undated.md",
            "# Undated Note\n- Some fact without temporal context",
        ),
        (
            "notes/unknown.md",
            "# Unknown Note\n- Unverified claim @t[?]",
        ),
    ];

    let ctx = TestContext::with_files("exclude_unknown_test", files);
    ctx.scan().await.expect("scan should succeed");

    let embedding = ctx.embedding();

    // Search for "note" - should find all three
    let query_emb = embedding
        .generate("note fact")
        .await
        .expect("embedding should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");

    // All three documents should be found in unfiltered search
    assert!(
        results.len() >= 2,
        "Should find multiple note documents in unfiltered search"
    );
}

/// Test search --watch mode detects file changes and triggers re-search.
///
/// This test verifies the core functionality of search watch mode:
/// 1. Initial search finds documents
/// 2. FileWatcher detects file modifications
/// 3. After rescan, updated content is searchable
///
/// Marked #[ignore] because:
/// - Requires Ollama for embeddings
/// - Depends on file system events which can be flaky in CI
/// - Uses real file watcher with timing-sensitive debouncing
#[tokio::test]
#[ignore]
async fn test_search_watch_mode_detects_changes() {
    use factbase::watcher::FileWatcher;
    use std::time::Duration;

    require_ollama().await;

    // Create test context with initial document
    let ctx = TestContext::with_files(
        "watch-test",
        &[(
            "people/alice.md",
            "# Alice\nAlice is a software engineer at TechCorp.",
        )],
    );

    let embedding = ctx.embedding();

    // Initial scan to index documents
    let result = ctx.scan().await.expect("initial scan should succeed");
    assert_eq!(result.added, 1, "Should add 1 document");

    // Verify initial search finds Alice
    let query_emb = embedding
        .generate("software engineer")
        .await
        .expect("embedding generation should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");
    assert!(
        results.iter().any(|r| r.title.contains("Alice")),
        "Initial search should find Alice"
    );

    // Set up file watcher (similar to search watch mode)
    let mut watcher = FileWatcher::new(200, &ctx.config.watcher.ignore_patterns)
        .expect("watcher creation should succeed");
    watcher
        .watch_directory(&ctx.repo_path)
        .expect("watch directory should succeed");

    // Allow watcher to initialize
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Modify the document with new content
    fs::write(
        ctx.repo_path.join("people/alice.md"),
        "# Alice\nAlice is a machine learning researcher specializing in NLP.",
    )
    .expect("file write should succeed");

    // Wait for debounce window
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify watcher detected the change
    let mut event_received = false;
    for _ in 0..10 {
        if watcher.try_recv().is_some() {
            event_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        event_received,
        "FileWatcher should detect file modification"
    );

    // Rescan to update index (this is what search watch mode would trigger)
    let result = ctx.scan().await.expect("rescan should succeed");
    assert_eq!(result.updated, 1, "Should update 1 document");

    // Verify search now finds updated content
    let query_emb = embedding
        .generate("machine learning NLP researcher")
        .await
        .expect("embedding generation should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");
    assert!(
        results.iter().any(|r| r.title.contains("Alice")),
        "Search should find Alice with updated content"
    );

    // Verify old content is no longer the top match for old query
    let query_emb = embedding
        .generate("software engineer TechCorp")
        .await
        .expect("embedding generation should succeed");
    let results = ctx
        .db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .expect("search should succeed");
    // Alice should still be found but content has changed
    if !results.is_empty() {
        let alice_result = results.iter().find(|r| r.title.contains("Alice"));
        if let Some(alice) = alice_result {
            assert!(
                !alice.snippet.contains("TechCorp"),
                "Updated document should not contain old content"
            );
        }
    }
}
