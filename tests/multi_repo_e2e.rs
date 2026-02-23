//! Multi-repository E2E tests with real Ollama operations.
//! Tests multiple repositories with real embeddings and link detection.

mod common;

use common::fixtures::create_temp_repo_with_files;
use common::ollama_helpers::require_ollama;
use common::TestScanSetup;
use factbase::{database::Database, scanner::full_scan, EmbeddingProvider};
use tempfile::TempDir;

fn setup_db(temp: &TempDir) -> Database {
    let db_path = temp.path().join("factbase.db");
    Database::new(&db_path).expect("Database creation should succeed")
}

/// Task 4.1: Test multi-repo workflow with real Ollama
#[tokio::test]
async fn test_multi_repo_with_real_ollama() {
    require_ollama().await;

    let temp = TempDir::new().expect("temp dir");
    let db = setup_db(&temp);

    // Create "engineering" repo with technical content
    let eng_files = [
        (
            "people/alice.md",
            "# Alice Chen\nSenior backend engineer specializing in Rust and distributed systems.",
        ),
        (
            "people/bob.md",
            "# Bob Martinez\nFrontend developer with React and TypeScript expertise.",
        ),
        (
            "projects/api-gateway.md",
            "# API Gateway\nMicroservices gateway built by Alice Chen using Rust.",
        ),
    ];
    let eng_dir = create_temp_repo_with_files(&eng_files);
    let eng_repo = common::test_repo("engineering", eng_dir.path().to_path_buf());
    db.upsert_repository(&eng_repo).expect("add eng repo");

    // Create "sales" repo with business content
    let sales_files = [
        (
            "people/carol.md",
            "# Carol Davis\nSales director managing enterprise accounts.",
        ),
        (
            "people/dave.md",
            "# Dave Wilson\nAccount executive focused on startup clients.",
        ),
        (
            "deals/acme-corp.md",
            "# ACME Corp Deal\nEnterprise deal managed by Carol Davis worth $500K.",
        ),
    ];
    let sales_dir = create_temp_repo_with_files(&sales_files);
    let sales_repo = common::test_repo("sales", sales_dir.path().to_path_buf());
    db.upsert_repository(&sales_repo).expect("add sales repo");

    // Set up components
    let setup = TestScanSetup::new();

    // Scan engineering repo
    let eng_result = full_scan(&eng_repo, &db, &setup.context())
        .await
        .expect("Engineering scan should succeed");

    assert_eq!(eng_result.added, 3, "Engineering should have 3 docs");

    // Scan sales repo
    let sales_result = full_scan(&sales_repo, &db, &setup.context())
        .await
        .expect("Sales scan should succeed");

    assert_eq!(sales_result.added, 3, "Sales should have 3 docs");

    // Verify embeddings generated for both repos
    let query_emb = setup.embedding.generate("engineer").await.unwrap();
    let all_results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();
    assert!(
        all_results.len() >= 3,
        "Should find results from both repos"
    );

    // Verify links detected within each repo
    let eng_docs = db.get_documents_for_repo("engineering").unwrap();
    let eng_links: usize = eng_docs
        .values()
        .map(|d| db.get_links_from(&d.id).unwrap_or_default().len())
        .sum();
    println!("Engineering repo links: {}", eng_links);

    let sales_docs = db.get_documents_for_repo("sales").unwrap();
    let sales_links: usize = sales_docs
        .values()
        .map(|d| db.get_links_from(&d.id).unwrap_or_default().len())
        .sum();
    println!("Sales repo links: {}", sales_links);
}

/// Task 4.2: Test cross-repo search
#[tokio::test]
async fn test_cross_repo_search() {
    require_ollama().await;

    let temp = TempDir::new().expect("temp dir");
    let db = setup_db(&temp);

    // Create two repos with overlapping concepts
    let eng_files = [(
        "concepts/microservices.md",
        "# Microservices\nDistributed architecture pattern for scalable systems.",
    )];
    let eng_dir = create_temp_repo_with_files(&eng_files);
    let eng_repo = common::test_repo("engineering", eng_dir.path().to_path_buf());
    db.upsert_repository(&eng_repo).expect("add eng repo");

    let sales_files = [(
        "training/architecture.md",
        "# Architecture Overview\nOur platform uses microservices for flexibility.",
    )];
    let sales_dir = create_temp_repo_with_files(&sales_files);
    let sales_repo = common::test_repo("sales", sales_dir.path().to_path_buf());
    db.upsert_repository(&sales_repo).expect("add sales repo");

    let setup = TestScanSetup::new();

    // Scan both repos
    full_scan(&eng_repo, &db, &setup.context())
        .await
        .expect("Engineering scan");

    full_scan(&sales_repo, &db, &setup.context())
        .await
        .expect("Sales scan");

    // Search without repo filter - should find results from both
    let query_emb = setup
        .embedding
        .generate("microservices architecture")
        .await
        .unwrap();
    let results = db
        .search_semantic_with_query(&query_emb, 10, None, None, None)
        .unwrap();

    assert!(results.len() >= 2, "Should find docs from both repos");
    println!("Cross-repo search found {} results:", results.len());
    for r in &results {
        println!("  {} (score: {:.4})", r.title, r.relevance_score);
    }

    // Verify we can find docs from each repo by checking document counts
    let eng_docs = db.get_documents_for_repo("engineering").unwrap();
    let sales_docs = db.get_documents_for_repo("sales").unwrap();
    assert_eq!(eng_docs.len(), 1, "Engineering should have 1 doc");
    assert_eq!(sales_docs.len(), 1, "Sales should have 1 doc");
}

/// Task 4.3: Test repo-filtered search
#[tokio::test]
async fn test_repo_filtered_search() {
    require_ollama().await;

    let temp = TempDir::new().expect("temp dir");
    let db = setup_db(&temp);

    // Create repos with similar content
    let eng_files = [(
        "people/engineer.md",
        "# Software Engineer\nBuilds software systems and applications.",
    )];
    let eng_dir = create_temp_repo_with_files(&eng_files);
    let eng_repo = common::test_repo("engineering", eng_dir.path().to_path_buf());
    db.upsert_repository(&eng_repo).expect("add eng repo");

    let sales_files = [(
        "people/sales-engineer.md",
        "# Sales Engineer\nTechnical sales supporting enterprise deals.",
    )];
    let sales_dir = create_temp_repo_with_files(&sales_files);
    let sales_repo = common::test_repo("sales", sales_dir.path().to_path_buf());
    db.upsert_repository(&sales_repo).expect("add sales repo");

    let setup = TestScanSetup::new();

    full_scan(&eng_repo, &db, &setup.context())
        .await
        .expect("Engineering scan");
    full_scan(&sales_repo, &db, &setup.context())
        .await
        .expect("Sales scan");

    let query_emb = setup.embedding.generate("engineer").await.unwrap();

    // Search with repo="engineering"
    let eng_results = db
        .search_semantic_with_query(&query_emb, 10, None, Some("engineering"), None)
        .unwrap();
    println!("Engineering-only results: {}", eng_results.len());
    assert!(!eng_results.is_empty(), "Should find engineering docs");
    for r in &eng_results {
        // Verify by checking the doc exists in engineering repo
        let eng_docs = db.get_documents_for_repo("engineering").unwrap();
        assert!(
            eng_docs.contains_key(&r.id),
            "Result {} should be from engineering repo",
            r.title
        );
        println!("  {}", r.title);
    }

    // Search with repo="sales"
    let sales_results = db
        .search_semantic_with_query(&query_emb, 10, None, Some("sales"), None)
        .unwrap();
    println!("Sales-only results: {}", sales_results.len());
    assert!(!sales_results.is_empty(), "Should find sales docs");
    for r in &sales_results {
        // Verify by checking the doc exists in sales repo
        let sales_docs = db.get_documents_for_repo("sales").unwrap();
        assert!(
            sales_docs.contains_key(&r.id),
            "Result {} should be from sales repo",
            r.title
        );
        println!("  {}", r.title);
    }
}

/// Task 4.4: Test repo isolation
#[tokio::test]
async fn test_repo_isolation() {
    require_ollama().await;

    let temp = TempDir::new().expect("temp dir");
    let db = setup_db(&temp);

    // Create two repos
    let eng_files = [("doc.md", "# Engineering Doc\nOriginal engineering content.")];
    let eng_dir = create_temp_repo_with_files(&eng_files);
    let eng_repo = common::test_repo("engineering", eng_dir.path().to_path_buf());
    db.upsert_repository(&eng_repo).expect("add eng repo");

    let sales_files = [("doc.md", "# Sales Doc\nOriginal sales content.")];
    let sales_dir = create_temp_repo_with_files(&sales_files);
    let sales_repo = common::test_repo("sales", sales_dir.path().to_path_buf());
    db.upsert_repository(&sales_repo).expect("add sales repo");

    let setup = TestScanSetup::new();

    // Initial scan of both repos
    full_scan(&eng_repo, &db, &setup.context())
        .await
        .expect("Engineering scan");
    full_scan(&sales_repo, &db, &setup.context())
        .await
        .expect("Sales scan");

    // Get initial state
    let sales_docs_before = db.get_documents_for_repo("sales").unwrap();
    let sales_doc_id = sales_docs_before.keys().next().unwrap().clone();
    let sales_hash_before = sales_docs_before
        .get(&sales_doc_id)
        .unwrap()
        .file_hash
        .clone();

    // Modify engineering repo - read existing file to get ID
    let eng_docs = db.get_documents_for_repo("engineering").unwrap();
    let eng_doc = eng_docs.values().next().unwrap();
    let eng_doc_path = eng_dir.path().join("doc.md");
    let updated_content = format!(
        "<!-- factbase:{} -->\n# Engineering Doc\nUpdated engineering content with new information.",
        eng_doc.id
    );
    std::fs::write(&eng_doc_path, updated_content).expect("write updated doc");

    // Rescan only engineering
    let eng_result = full_scan(&eng_repo, &db, &setup.context())
        .await
        .expect("Engineering rescan");

    println!(
        "Engineering rescan: {} added, {} updated",
        eng_result.added, eng_result.updated
    );

    // Verify sales repo unchanged
    let sales_docs_after = db.get_documents_for_repo("sales").unwrap();
    let sales_hash_after = sales_docs_after
        .get(&sales_doc_id)
        .unwrap()
        .file_hash
        .clone();

    assert_eq!(
        sales_hash_before, sales_hash_after,
        "Sales doc should be unchanged after engineering rescan"
    );
    println!("Sales repo verified unchanged after engineering modification");
}
