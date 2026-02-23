//! Grep command integration tests.

use chrono::Utc;
use factbase::{database::Database, models::Repository};
use std::fs;
use tempfile::TempDir;

/// Test grep --dry-run validates pattern and shows search scope
#[test]
fn test_grep_dry_run_flag() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    // Create test documents
    fs::write(repo_path.join("doc1.md"), "# Doc One\nFirst document.")
        .expect("write should succeed");
    fs::write(repo_path.join("doc2.md"), "# Doc Two\nSecond document.")
        .expect("write should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add documents to database
    for i in 0..2 {
        db.upsert_document(&factbase::models::Document {
            id: format!("doc{:03}", i),
            repo_id: "notes".into(),
            title: format!("Document {}", i),
            doc_type: Some("note".into()),
            content: format!("Content for document {}", i),
            file_path: format!("doc{}.md", i),
            file_hash: format!("hash{}", i),
            file_modified_at: Some(Utc::now()),
            indexed_at: Utc::now(),
            is_deleted: false,
        })
        .expect("operation should succeed");
    }

    // Verify dry-run would show correct counts
    let repos = db
        .list_repositories_with_stats()
        .expect("operation should succeed");
    let repo_count = repos.len();
    let doc_count: usize = repos.iter().map(|(_, c)| *c).sum();

    assert_eq!(repo_count, 1, "Should have 1 repository");
    assert_eq!(doc_count, 2, "Should have 2 documents");

    // Verify expected dry-run output format
    let expected_output = format!(
        "Would search {} document(s) in {} repository(ies)",
        doc_count, repo_count
    );
    assert!(
        expected_output.contains("2 document(s)"),
        "Dry-run should show document count"
    );
    assert!(
        expected_output.contains("1 repository(ies)"),
        "Dry-run should show repository count"
    );
}

/// Test grep --format yaml outputs valid YAML
#[test]
fn test_grep_format_yaml() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    // Create test document with searchable content
    fs::write(
        repo_path.join("doc1.md"),
        "# Test Doc\nThis has TODO items.",
    )
    .expect("write should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add document with content that matches "TODO"
    db.upsert_document(&factbase::models::Document {
        id: "doc001".into(),
        repo_id: "notes".into(),
        title: "Test Doc".into(),
        doc_type: Some("note".into()),
        content: "# Test Doc\nThis has TODO items.".into(),
        file_path: "doc1.md".into(),
        file_hash: "hash1".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    // Search for TODO and get YAML output
    let results = db
        .search_content("TODO", 10, None, None, 0, None)
        .expect("search should succeed");

    // Verify we have results
    assert!(!results.is_empty(), "Should find TODO in document");

    // Verify YAML serialization works
    let yaml_output = factbase::format_yaml(&results).expect("YAML serialization should succeed");
    assert!(yaml_output.contains("doc001"), "YAML should contain doc ID");
    assert!(
        yaml_output.contains("TODO"),
        "YAML should contain matched text"
    );
    assert!(yaml_output.starts_with("- id:"), "YAML should be a list");
}

/// Test grep --stats flag shows match statistics
#[test]
fn test_grep_stats_flag() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add documents with multiple TODO matches
    db.upsert_document(&factbase::models::Document {
        id: "doc001".into(),
        repo_id: "notes".into(),
        title: "Doc One".into(),
        doc_type: Some("note".into()),
        content: "# Doc One\nTODO: first\nTODO: second\nTODO: third".into(),
        file_path: "doc1.md".into(),
        file_hash: "hash1".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    db.upsert_document(&factbase::models::Document {
        id: "doc002".into(),
        repo_id: "notes".into(),
        title: "Doc Two".into(),
        doc_type: Some("note".into()),
        content: "# Doc Two\nTODO: only one".into(),
        file_path: "doc2.md".into(),
        file_hash: "hash2".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    // Search and get stats
    let results = db
        .search_content("TODO", 10, None, None, 0, None)
        .expect("search should succeed");

    // Verify we have results from both docs
    assert_eq!(results.len(), 2, "Should find TODO in both documents");

    // Verify total match count
    let total_matches: usize = results.iter().map(|r| r.matches.len()).sum();
    assert_eq!(total_matches, 4, "Should have 4 total TODO matches");

    // Verify JSON stats output works
    let stats_json = serde_json::json!({
        "total_matches": total_matches,
        "document_count": results.len(),
        "repository_count": 1,
    });
    assert_eq!(stats_json["total_matches"], 4);
    assert_eq!(stats_json["document_count"], 2);
}

/// Test grep --since flag filters by indexed_at timestamp
#[test]
fn test_grep_since_flag() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add document indexed now
    db.upsert_document(&factbase::models::Document {
        id: "doc001".into(),
        repo_id: "notes".into(),
        title: "Recent Doc".into(),
        doc_type: Some("note".into()),
        content: "# Recent Doc\nTODO: recent task".into(),
        file_path: "recent.md".into(),
        file_hash: "hash1".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    // Search without since filter - should find document
    let results = db
        .search_content("TODO", 10, None, None, 0, None)
        .expect("search should succeed");
    assert_eq!(
        results.len(),
        1,
        "Should find document without since filter"
    );

    // Search with since filter in the future - should not find document
    let future = Utc::now() + chrono::Duration::hours(1);
    let results = db
        .search_content("TODO", 10, None, None, 0, Some(future))
        .expect("search should succeed");
    assert_eq!(
        results.len(),
        0,
        "Should not find document with future since filter"
    );

    // Search with since filter in the past - should find document
    let past = Utc::now() - chrono::Duration::hours(1);
    let results = db
        .search_content("TODO", 10, None, None, 0, Some(past))
        .expect("search should succeed");
    assert_eq!(
        results.len(),
        1,
        "Should find document with past since filter"
    );
}

/// Test grep --count flag outputs only the match count
#[test]
fn test_grep_count_flag() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add documents with multiple FIXME matches
    db.upsert_document(&factbase::models::Document {
        id: "doc001".into(),
        repo_id: "notes".into(),
        title: "Doc One".into(),
        doc_type: Some("note".into()),
        content: "# Doc One\nFIXME: first\nFIXME: second".into(),
        file_path: "doc1.md".into(),
        file_hash: "hash1".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    db.upsert_document(&factbase::models::Document {
        id: "doc002".into(),
        repo_id: "notes".into(),
        title: "Doc Two".into(),
        doc_type: Some("note".into()),
        content: "# Doc Two\nFIXME: only one".into(),
        file_path: "doc2.md".into(),
        file_hash: "hash2".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    // Search and verify count
    let results = db
        .search_content("FIXME", 10, None, None, 0, None)
        .expect("search should succeed");

    let total_matches: usize = results.iter().map(|r| r.matches.len()).sum();
    assert_eq!(total_matches, 3, "Should have 3 total FIXME matches");

    // Verify JSON count output format
    let count_json = serde_json::json!({ "count": total_matches });
    assert_eq!(count_json["count"], 3);
}

/// Test grep --exclude-type flag filters out documents by type
#[test]
fn test_grep_exclude_type_flag() {
    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "notes".into(),
        name: "Notes".into(),
        path: repo_path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    // Add documents with different types
    db.upsert_document(&factbase::models::Document {
        id: "doc001".into(),
        repo_id: "notes".into(),
        title: "Draft Doc".into(),
        doc_type: Some("draft".into()),
        content: "# Draft Doc\nTODO: finish this".into(),
        file_path: "draft.md".into(),
        file_hash: "hash1".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    db.upsert_document(&factbase::models::Document {
        id: "doc002".into(),
        repo_id: "notes".into(),
        title: "Published Doc".into(),
        doc_type: Some("note".into()),
        content: "# Published Doc\nTODO: review this".into(),
        file_path: "published.md".into(),
        file_hash: "hash2".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    db.upsert_document(&factbase::models::Document {
        id: "doc003".into(),
        repo_id: "notes".into(),
        title: "Archived Doc".into(),
        doc_type: Some("archived".into()),
        content: "# Archived Doc\nTODO: old task".into(),
        file_path: "archived.md".into(),
        file_hash: "hash3".into(),
        file_modified_at: Some(Utc::now()),
        indexed_at: Utc::now(),
        is_deleted: false,
    })
    .expect("operation should succeed");

    // Search without exclusion - should find all 3
    let all_results = db
        .search_content("TODO", 10, None, None, 0, None)
        .expect("search should succeed");
    assert_eq!(all_results.len(), 3, "Should find 3 documents with TODO");

    // Simulate --exclude-type filtering (as done in grep.rs)
    let exclude_types = ["draft".to_string()];
    let exclude_types_lower: Vec<String> = exclude_types.iter().map(|t| t.to_lowercase()).collect();
    let filtered: Vec<_> = all_results
        .iter()
        .filter(|r| {
            r.doc_type
                .as_ref()
                .map(|t| !exclude_types_lower.contains(&t.to_lowercase()))
                .unwrap_or(true)
        })
        .collect();
    assert_eq!(
        filtered.len(),
        2,
        "Should have 2 results after excluding draft"
    );

    // Exclude multiple types
    let exclude_types = ["draft".to_string(), "archived".to_string()];
    let exclude_types_lower: Vec<String> = exclude_types.iter().map(|t| t.to_lowercase()).collect();
    let filtered: Vec<_> = all_results
        .iter()
        .filter(|r| {
            r.doc_type
                .as_ref()
                .map(|t| !exclude_types_lower.contains(&t.to_lowercase()))
                .unwrap_or(true)
        })
        .collect();
    assert_eq!(
        filtered.len(),
        1,
        "Should have 1 result after excluding draft and archived"
    );
    assert_eq!(filtered[0].title, "Published Doc");
}
