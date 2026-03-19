//! Scan command integration tests.

use super::common::ollama_helpers::require_ollama;
use super::common::run_scan;
use super::common::TestScanSetup;
use chrono::Utc;
use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    models::{Perspective, Repository},
    scanner::{full_scan, ScanOptions},
    EmbeddingProvider,
};
use std::fs;
use tempfile::TempDir;

/// Test 13.5: dry-run scan
#[tokio::test]
async fn test_dry_run_scan() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    fs::write(repo_path.join("doc.md"), "# Test\nContent.").unwrap();

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = super::common::test_repo("test", repo_path.clone());
    db.add_repository(&repo).unwrap();

    // Dry-run scan
    let setup = TestScanSetup::with_options(ScanOptions {
        dry_run: true,
        ..ScanOptions::default()
    });
    let result = full_scan(&repo, &db, &setup.context()).await.unwrap();

    assert_eq!(result.added, 1, "Dry-run should report 1 new document");

    // Verify no actual changes
    let docs = db.get_documents_for_repo("test").unwrap();
    assert!(
        docs.is_empty(),
        "Dry-run should not add documents to database"
    );

    // Verify file not modified (no header injected)
    let content = fs::read_to_string(repo_path.join("doc.md")).unwrap();
    assert!(
        !content.contains("<!-- factbase:"),
        "Dry-run should not inject header"
    );
}

/// Test: --timeout flag overrides config timeout
#[tokio::test]
#[ignore]
async fn test_timeout_flag_override() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).unwrap();

    fs::write(repo_path.join("doc.md"), "# Test\nSimple document.").unwrap();

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = super::common::test_repo("test", repo_path);
    db.add_repository(&repo).unwrap();

    let config = Config::default();

    // Create embedding with custom timeout (60 seconds instead of default 30)
    let custom_timeout = 60u64;
    let embedding = OllamaEmbedding::with_timeout(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
        custom_timeout,
    );

    // Verify embedding works with custom timeout
    let query_emb = embedding.generate("test query").await.unwrap();
    assert_eq!(
        query_emb.len(),
        config.embedding.dimension,
        "Embedding dimension should match config"
    );
}

/// Test scan --progress and --no-progress flags are accepted
#[test]
fn test_scan_progress_flags() {
    use std::process::Command;

    // Test that --progress is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "scan", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--progress"),
        "scan --help should show --progress flag"
    );
    assert!(
        stdout.contains("--no-progress"),
        "scan --help should show --no-progress flag"
    );
    assert!(
        stdout.contains("Force progress bars"),
        "scan --help should describe --progress"
    );
    assert!(
        stdout.contains("Disable progress bars"),
        "scan --help should describe --no-progress"
    );
}

/// Test scan --progress and --no-progress are mutually exclusive
#[test]
fn test_scan_progress_flags_mutually_exclusive() {
    use std::process::Command;

    // Test that using both flags together produces an error
    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--",
            "scan",
            "--progress",
            "--no-progress",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() || stderr.contains("mutually exclusive"),
        "Using --progress and --no-progress together should fail"
    );
}

/// Test 13.1: init -> scan -> search workflow
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_init_scan_search_workflow() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(repo_path.join("people")).unwrap();

    // Create test documents
    fs::write(
        repo_path.join("people/alice.md"),
        "# Alice\nAlice is a software engineer.",
    )
    .unwrap();
    fs::write(
        repo_path.join("readme.md"),
        "# My Notes\nPersonal knowledge base.",
    )
    .unwrap();

    // Simulate `factbase init` - create database
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    // Simulate `factbase repo add`
    let repo = Repository {
        id: "notes".into(),
        name: "My Notes".into(),
        path: repo_path,
        perspective: Some(Perspective {
            type_name: "personal".into(),
            focus: Some("knowledge management".into()),
            ..Default::default()
        }),
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).unwrap();

    let config = Config::default();

    // Simulate `factbase scan`
    let result = run_scan(&repo, &db, &config).await.unwrap();
    assert_eq!(result.added, 2, "Should add 2 documents");

    // Create embedding provider for search
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Simulate `factbase search "software engineer"`
    let query_embedding = embedding.generate("software engineer").await.unwrap();
    let results = db
        .search_semantic_with_query(&query_embedding, 10, None, None, None)
        .unwrap();

    assert!(!results.is_empty(), "Search should return results");
    // Alice document should be most relevant
    assert!(
        results[0].title.contains("Alice") || results[0].snippet.contains("software engineer"),
        "Top result should be Alice document"
    );
}

/// Test 13.2: repo add -> scan -> repo remove workflow
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_repo_add_scan_remove_workflow() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();

    // Create two repos
    let repo1_path = temp_dir.path().join("repo1");
    let repo2_path = temp_dir.path().join("repo2");
    fs::create_dir_all(&repo1_path).unwrap();
    fs::create_dir_all(&repo2_path).unwrap();

    fs::write(repo1_path.join("doc1.md"), "# Doc 1\nContent for repo 1.").unwrap();
    fs::write(repo2_path.join("doc2.md"), "# Doc 2\nContent for repo 2.").unwrap();

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    // Add both repos
    let repo1 = super::common::test_repo("repo1", repo1_path);
    let repo2 = super::common::test_repo("repo2", repo2_path);
    db.add_repository(&repo1).unwrap();
    db.add_repository(&repo2).unwrap();

    let config = Config::default();

    // Scan both
    run_scan(&repo1, &db, &config).await.unwrap();
    run_scan(&repo2, &db, &config).await.unwrap();

    // Verify both indexed
    let repos = db.list_repositories().unwrap();
    assert_eq!(repos.len(), 2, "Should have 2 repos");

    let docs1 = db.get_documents_for_repo("repo1").unwrap();
    let docs2 = db.get_documents_for_repo("repo2").unwrap();
    assert_eq!(docs1.len(), 1, "Repo 1 should have 1 doc");
    assert_eq!(docs2.len(), 1, "Repo 2 should have 1 doc");

    // Remove repo1
    db.remove_repository("repo1").unwrap();

    // Verify repo1 removed
    let repos = db.list_repositories().unwrap();
    assert_eq!(repos.len(), 1, "Should have 1 repo after removal");
    assert_eq!(repos[0].id, "repo2", "Remaining repo should be repo2");

    // Verify repo2 unaffected
    let docs2 = db.get_documents_for_repo("repo2").unwrap();
    assert_eq!(docs2.len(), 1, "Repo 2 should still have 1 doc");
}
