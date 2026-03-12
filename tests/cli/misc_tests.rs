//! Miscellaneous command integration tests.
//! Includes: status, stats, doctor, show, links, version

use super::common::ollama_helpers::require_ollama;
use super::common::run_scan;
use chrono::Utc;
use factbase::{config::Config, database::Database};
use std::fs;
use tempfile::TempDir;

/// Test 13.4: status command
#[tokio::test]
#[ignore]
async fn test_status_command() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    for i in 0..5 {
        fs::write(
            repo_path.join(format!("doc{}.md", i)),
            format!("# Document {}\nContent {}.", i, i),
        )
        .unwrap();
    }

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    let repo = super::common::test_repo("test", repo_path);
    db.add_repository(&repo).unwrap();

    let config = Config::default();

    run_scan(&repo, &db, &config).await.unwrap();

    // Simulate `factbase status`
    let stats = db.get_stats("test", None).unwrap();
    assert_eq!(stats.total, 5, "Should have 5 documents");
    assert_eq!(stats.active, 5, "Should have 5 active documents");

    // Simulate `factbase status --detailed`
    let detailed = db.get_detailed_stats("test", None).unwrap();
    assert!(detailed.avg_doc_size > 0, "Avg doc size should be > 0");
}

/// Test: stats --short flag produces single-line output
#[test]
fn test_stats_short_flag() {
    use factbase::format_bytes;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).unwrap();

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).unwrap();

    // Add a repository with some documents
    let repo = super::common::test_repo("test", repo_path);
    db.add_repository(&repo).unwrap();

    // Add some test documents directly to database
    for i in 0..3 {
        db.upsert_document(&factbase::models::Document {
            id: format!("doc{:03}", i),
            repo_id: "test".into(),
            title: format!("Document {}", i),
            doc_type: Some("note".into()),
            content: format!("Content for document {}", i),
            file_path: format!("doc{}.md", i),
            file_hash: format!("hash{}", i),
            file_modified_at: Some(Utc::now()),
            indexed_at: Utc::now(),
            is_deleted: false,
        })
        .unwrap();
    }

    // Get stats to verify format
    let repos = db.list_repositories_with_stats().unwrap();
    let total_repos = repos.len();
    let total_docs: usize = repos.iter().map(|(_, c)| c).sum();
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    // Verify short format would produce expected output
    let short_output = format!(
        "{} repos, {} docs, {}",
        total_repos,
        total_docs,
        format_bytes(db_size)
    );

    assert_eq!(total_repos, 1, "Should have 1 repository");
    assert_eq!(total_docs, 3, "Should have 3 documents");
    assert!(
        short_output.contains("1 repos, 3 docs"),
        "Short output should contain repo and doc counts"
    );
}

/// Test doctor --quiet flag suppresses output on success
#[test]
fn test_doctor_quiet_flag() {
    // Verify --quiet flag is accepted by clap parser
    // The actual behavior is tested by running the binary
    use std::process::Command;

    // Test that --quiet is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "doctor", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--quiet") || stdout.contains("-q"),
        "doctor --help should show --quiet flag"
    );
}

/// Test doctor --timeout flag is accepted
#[test]
fn test_doctor_timeout_flag() {
    use std::process::Command;

    // Test that --timeout is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "doctor", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--timeout"),
        "doctor --help should show --timeout flag"
    );
    assert!(
        stdout.contains("SECONDS"),
        "doctor --help should show SECONDS value name"
    );
}

/// Test --version shows build info (date and rustc version)
#[test]
fn test_version_shows_build_info() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "--version"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Long version should show build date and rustc version
    assert!(
        stdout.contains("built") && stdout.contains("rustc"),
        "--version should show build info: got '{}'",
        stdout.trim()
    );
}

/// Test status --offline flag is accepted
#[test]
#[ignore]
fn test_status_offline_flag() {
    // `status` command was removed from CLI; this test is kept as a placeholder.
}

/// Test show command displays document details
#[test]
#[ignore]
fn test_show_command_help() {
    // `show` command was removed from CLI; this test is kept as a placeholder.
}

/// Test show command with non-existent document
#[test]
#[ignore]
fn test_show_nonexistent_document() {
    // `show` command was removed from CLI; this test is kept as a placeholder.
}

/// Test links command help output
#[test]
#[ignore]
fn test_links_command_help() {
    // `links` command was removed from CLI; this test is kept as a placeholder.
}

/// Test links command with non-existent document
#[test]
#[ignore]
fn test_links_nonexistent_document() {
    // `links` command was removed from CLI; this test is kept as a placeholder.
}
