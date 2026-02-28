//! Error recovery and resilience tests.
//! These tests verify the system handles errors gracefully.

mod common;

use common::ollama_helpers::require_ollama;
use common::TestContext;
use std::fs;
use std::io::Write;

/// Test 8.1: Corrupted file handling (invalid UTF-8)
#[tokio::test]
#[ignore]
async fn test_corrupted_file_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create valid document
    fs::write(
        ctx.repo_path.join("valid.md"),
        "# Valid Document\nThis is valid content.",
    )
    .unwrap();

    // Create corrupted file with invalid UTF-8
    let corrupted_path = ctx.repo_path.join("corrupted.md");
    let mut file = fs::File::create(&corrupted_path).unwrap();
    file.write_all(b"# Corrupted\n\xFF\xFE Invalid UTF-8 bytes")
        .unwrap();

    // Scan should continue despite corrupted file
    let result = ctx.scan().await.unwrap();

    // Valid file should be indexed
    assert_eq!(result.added, 1, "Valid document should be indexed");

    // Verify valid document is searchable
    let docs = ctx.db.get_documents_for_repo("test").unwrap();
    assert_eq!(docs.len(), 1, "Only valid document should be in database");
    assert!(
        docs.values().any(|d| d.title.contains("Valid")),
        "Valid document should be indexed"
    );
}

/// Test 8.2: Very large file handling
#[tokio::test]
#[ignore]
async fn test_very_large_file_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create normal document
    fs::write(
        ctx.repo_path.join("normal.md"),
        "# Normal Document\nNormal sized content.",
    )
    .unwrap();

    // Create very large file (2MB - exceeds default max_file_size of 1MB)
    let large_content = format!("# Large Document\n{}", "x".repeat(2 * 1024 * 1024));
    fs::write(ctx.repo_path.join("large.md"), &large_content).unwrap();

    // Scan should process both files (large file will be truncated for embedding)
    let result = ctx.scan().await.unwrap();

    // Both files should be indexed (large file content truncated for embedding)
    assert_eq!(result.added, 2, "Both documents should be indexed");

    let docs = ctx.db.get_documents_for_repo("test").unwrap();
    assert_eq!(docs.len(), 2, "Both documents should be in database");
}

/// Test 8.3: Invalid factbase header handling
#[tokio::test]
#[ignore]
async fn test_invalid_factbase_header_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create file with malformed header
    fs::write(
        ctx.repo_path.join("malformed.md"),
        "<!-- factbase:INVALID -->\n# Malformed Header\nContent here.",
    )
    .unwrap();

    // Create file with partial header
    fs::write(
        ctx.repo_path.join("partial.md"),
        "<!-- factbase: -->\n# Partial Header\nContent here.",
    )
    .unwrap();

    // Create file with wrong format
    fs::write(
        ctx.repo_path.join("wrong.md"),
        "<!-- factbase:toolong123 -->\n# Wrong Format\nContent here.",
    )
    .unwrap();

    // Scan should handle invalid headers by generating new IDs
    let result = ctx.scan().await.unwrap();

    assert_eq!(result.added, 3, "All documents should be indexed");

    // Verify files now have valid headers
    let malformed_content = fs::read_to_string(ctx.repo_path.join("malformed.md")).unwrap();
    assert!(
        malformed_content.contains("<!-- factbase:"),
        "Malformed file should have header"
    );

    // Check that IDs are valid 6-char hex
    let docs = ctx.db.get_documents_for_repo("test").unwrap();
    for id in docs.keys() {
        assert_eq!(id.len(), 6, "ID should be 6 characters");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "ID should be hex"
        );
    }
}

/// Test 8.4: Empty file handling
#[tokio::test]
#[ignore]
async fn test_empty_file_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create empty file
    fs::write(ctx.repo_path.join("empty.md"), "").unwrap();

    // Create file with only whitespace
    fs::write(ctx.repo_path.join("whitespace.md"), "   \n\n   \n").unwrap();

    // Create normal file
    fs::write(ctx.repo_path.join("normal.md"), "# Normal\nNormal content.").unwrap();

    // Scan should handle empty files gracefully
    let result = ctx.scan().await.unwrap();

    // All files should be processed (empty files get headers added)
    assert!(result.added >= 1, "At least normal file should be indexed");

    let docs = ctx.db.get_documents_for_repo("test").unwrap();
    assert!(
        docs.values().any(|d| d.title.contains("Normal")),
        "Normal document should be indexed"
    );
}

/// Test 8.5: Permission denied handling
#[tokio::test]
#[ignore]
#[cfg(unix)]
async fn test_permission_denied_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create readable file
    fs::write(
        ctx.repo_path.join("readable.md"),
        "# Readable\nReadable content.",
    )
    .unwrap();

    // Create unreadable file
    let unreadable_path = ctx.repo_path.join("unreadable.md");
    fs::write(&unreadable_path, "# Unreadable\nUnreadable content.").unwrap();

    // Make file unreadable (Unix only)
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&unreadable_path).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&unreadable_path, perms).unwrap();

    // Scan should continue despite permission error
    let result = ctx.scan().await.unwrap();

    // Readable file should be indexed
    assert_eq!(result.added, 1, "Readable document should be indexed");

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&unreadable_path).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&unreadable_path, perms).unwrap();
}

/// Test 8.6: Symlink handling
#[tokio::test]
#[ignore]
#[cfg(unix)]
async fn test_symlink_handling() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create real file
    fs::write(
        ctx.repo_path.join("real.md"),
        "# Real Document\nReal content.",
    )
    .unwrap();

    // Create symlink to real file
    std::os::unix::fs::symlink(ctx.repo_path.join("real.md"), ctx.repo_path.join("link.md"))
        .unwrap();

    // Create broken symlink
    std::os::unix::fs::symlink(
        ctx.repo_path.join("nonexistent.md"),
        ctx.repo_path.join("broken.md"),
    )
    .unwrap();

    // Scan should handle symlinks gracefully
    let result = ctx.scan().await.unwrap();

    // At least the real file should be indexed
    assert!(result.added >= 1, "At least real file should be indexed");
}

/// Test database integrity after errors
#[tokio::test]
#[ignore]
async fn test_database_integrity_after_errors() {
    require_ollama().await;

    let ctx = TestContext::new("test");

    // Create mix of valid and problematic files
    fs::write(
        ctx.repo_path.join("valid1.md"),
        "# Valid One\nFirst valid document.",
    )
    .unwrap();

    // Invalid UTF-8
    let mut file = fs::File::create(ctx.repo_path.join("invalid.md")).unwrap();
    file.write_all(b"# Invalid\n\xFF\xFE bytes").unwrap();

    fs::write(
        ctx.repo_path.join("valid2.md"),
        "# Valid Two\nSecond valid document.",
    )
    .unwrap();

    // Run scan
    ctx.scan().await.unwrap();

    // Verify database integrity
    let docs = ctx.db.get_documents_for_repo("test").unwrap();

    // All valid documents should be indexed
    assert_eq!(docs.len(), 2, "Both valid documents should be indexed");

    // Verify no orphaned embeddings
    for id in docs.keys() {
        let search_results = ctx
            .db
            .search_semantic_with_query(
                &vec![0.0; ctx.config.embedding.dimension],
                10,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(
            search_results.iter().any(|r| &r.id == id),
            "Document {} should have embedding",
            id
        );
    }
}
