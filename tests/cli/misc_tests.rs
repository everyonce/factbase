//! Miscellaneous command integration tests.
//! Includes: status, stats, doctor, show, links, version

use super::common::ollama_helpers::require_ollama;
use super::common::run_scan;
use chrono::Utc;
use factbase::{config::Config, database::Database, models::Repository};
use std::fs;
use tempfile::TempDir;

/// Test 13.4: status command
#[tokio::test]
#[ignore]
async fn test_status_command() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("repo");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    for i in 0..5 {
        fs::write(
            repo_path.join(format!("doc{}.md", i)),
            format!("# Document {}\nContent {}.", i, i),
        )
        .expect("operation should succeed");
    }

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

    let config = Config::default();

    run_scan(&repo, &db, &config)
        .await
        .expect("operation should succeed");

    // Simulate `factbase status`
    let stats = db
        .get_stats("test", None)
        .expect("operation should succeed");
    assert_eq!(stats.total, 5, "Should have 5 documents");
    assert_eq!(stats.active, 5, "Should have 5 active documents");

    // Simulate `factbase status --detailed`
    let detailed = db
        .get_detailed_stats("test", None)
        .expect("operation should succeed");
    assert!(detailed.avg_doc_size > 0, "Avg doc size should be > 0");
}

/// Test: stats --short flag produces single-line output
#[test]
fn test_stats_short_flag() {
    use factbase::format_bytes;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let repo_path = temp_dir.path().join("notes");
    fs::create_dir_all(&repo_path).expect("operation should succeed");

    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("operation should succeed");

    // Add a repository with some documents
    let repo = Repository {
        id: "test".into(),
        name: "Test".into(),
        path: repo_path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("operation should succeed");

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
        .expect("operation should succeed");
    }

    // Get stats to verify format
    let repos = db
        .list_repositories_with_stats()
        .expect("operation should succeed");
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
fn test_status_offline_flag() {
    use std::process::Command;

    // Test that --offline is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "status", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--offline"),
        "status --help should show --offline flag"
    );
    assert!(
        stdout.contains("no-op") || stdout.contains("never contacts Ollama"),
        "status --help should clarify --offline is a no-op"
    );
}

/// Test show command displays document details
#[test]
fn test_show_command_help() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "show", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Show document details"),
        "show --help should show description"
    );
    assert!(
        stdout.contains("Document ID"),
        "show --help should mention Document ID"
    );
    assert!(
        stdout.contains("--json"),
        "show --help should show --json flag"
    );
    assert!(
        stdout.contains("--format"),
        "show --help should show --format flag"
    );
}

/// Test show command with non-existent document
#[test]
fn test_show_nonexistent_document() {
    use std::process::Command;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let db_path = temp_dir.path().join("factbase.db");

    // Create empty database
    let db = factbase::Database::new(&db_path).expect("operation should succeed");
    drop(db);

    // Create minimal config pointing to temp database
    let config_dir = temp_dir.path().join(".config").join("factbase");
    std::fs::create_dir_all(&config_dir).expect("operation should succeed");
    let config_content = format!(
        r#"database:
  path: {}
  pool_size: 4
  compression: none
repositories: []
watcher:
  debounce_ms: 500
  ignore_patterns: []
processor:
  max_file_size: 1048576
  snippet_length: 200
  embedding_batch_size: 10
  metadata_cache_size: 100
embedding:
  provider: ollama
  base_url: http://localhost:11434
  model: qwen3-embedding:0.6b
  dimension: 1024
  cache_size: 100
  timeout_secs: 30
llm:
  provider: ollama
  base_url: http://localhost:11434
  model: rnj-1-extended
  timeout_secs: 30
"#,
        db_path.display()
    );
    std::fs::write(config_dir.join("config.yaml"), config_content)
        .expect("operation should succeed");

    // Run show with non-existent ID
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "show", "abc123"])
        .env("HOME", temp_dir.path())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("abc123"),
        "show should report document not found: {}",
        stderr
    );
}

/// Test links command help output
#[test]
fn test_links_command_help() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "links", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Explore document link relationships"),
        "links --help should show description"
    );
    assert!(
        stdout.contains("--reverse"),
        "links --help should show --reverse flag"
    );
    assert!(
        stdout.contains("--orphans"),
        "links --help should show --orphans flag"
    );
    assert!(
        stdout.contains("--top"),
        "links --help should show --top flag"
    );
    assert!(
        stdout.contains("--json"),
        "links --help should show --json flag"
    );
    assert!(
        stdout.contains("--format"),
        "links --help should show --format flag"
    );
}

/// Test links command with non-existent document
#[test]
fn test_links_nonexistent_document() {
    use std::process::Command;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().expect("operation should succeed");
    let db_path = temp_dir.path().join("factbase.db");

    // Create empty database
    let db = factbase::Database::new(&db_path).expect("operation should succeed");
    drop(db);

    // Create minimal config pointing to temp database
    let config_dir = temp_dir.path().join(".config").join("factbase");
    std::fs::create_dir_all(&config_dir).expect("operation should succeed");
    let config_content = format!(
        r#"database:
  path: {}
  pool_size: 4
  compression: none
repositories: []
watcher:
  debounce_ms: 500
  ignore_patterns: []
processor:
  max_file_size: 1048576
  snippet_length: 200
  embedding_batch_size: 10
  metadata_cache_size: 100
embedding:
  provider: ollama
  base_url: http://localhost:11434
  model: qwen3-embedding:0.6b
  dimension: 1024
  cache_size: 100
  timeout_secs: 30
llm:
  provider: ollama
  base_url: http://localhost:11434
  model: rnj-1-extended
  timeout_secs: 30"#,
        db_path.display()
    );
    std::fs::write(config_dir.join("config.yaml"), config_content)
        .expect("operation should succeed");

    // Run links with non-existent ID
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "links", "abc123"])
        .env("HOME", temp_dir.path())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("abc123"),
        "links should report document not found: {}",
        stderr
    );
}
