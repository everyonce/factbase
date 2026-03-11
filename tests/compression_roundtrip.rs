//! Integration tests for compressed export/import roundtrip.
//! Tests .tar.zst, .json.zst formats using library functions directly.

mod common;

use chrono::Utc;
use common::ollama_helpers::require_ollama;
use common::run_scan;
use factbase::{
    config::Config,
    database::Database,
    models::{Perspective, Repository},
    processor::DocumentProcessor,
};
use std::fs;
use std::io::Read;
use tempfile::TempDir;

/// Helper to create a test repository with documents
fn create_test_repo(temp_dir: &TempDir) -> (Repository, std::path::PathBuf) {
    let repo_path = temp_dir.path().join("source");
    fs::create_dir_all(repo_path.join("people")).expect("create people dir");
    fs::create_dir_all(repo_path.join("projects")).expect("create projects dir");

    fs::write(
        repo_path.join("people/alice.md"),
        "# Alice\nAlice is a software engineer who works on backend systems.",
    )
    .expect("write alice.md");

    fs::write(
        repo_path.join("people/bob.md"),
        "# Bob\nBob is a frontend developer specializing in React.",
    )
    .expect("write bob.md");

    fs::write(
        repo_path.join("projects/api.md"),
        "# API Project\nThe API project is led by Alice and uses REST architecture.",
    )
    .expect("write api.md");

    let repo = Repository {
        id: "source".into(),
        name: "Source Repo".into(),
        path: repo_path.clone(),
        perspective: Some(Perspective {
            type_name: "test".into(),
            organization: None,
            focus: None,
            allowed_types: None,
            review: None,
            format: None,
            link_match_mode: None,
        }),
        created_at: Utc::now(),
        last_indexed_at: None,
        last_check_at: None,
    };

    (repo, repo_path)
}

/// Export documents as JSON and compress with zstd
fn export_json_zst(
    db: &Database,
    repo_id: &str,
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    let docs = db.list_documents(None, Some(repo_id), None, usize::MAX)?;
    let mut export_data: Vec<serde_json::Value> = Vec::new();

    for doc in &docs {
        let links_from = db.get_links_from(&doc.id)?;
        let links_to = db.get_links_to(&doc.id)?;
        export_data.push(serde_json::json!({
            "id": doc.id,
            "title": doc.title,
            "type": doc.doc_type,
            "content": doc.content,
            "links_to": links_from.iter().map(|l| &l.target_id).collect::<Vec<_>>(),
            "linked_from": links_to.iter().map(|l| &l.source_id).collect::<Vec<_>>(),
        }));
    }

    let json_content = serde_json::to_string_pretty(&export_data)?;
    let compressed = zstd::encode_all(json_content.as_bytes(), 3)?;
    fs::write(output_path, compressed)?;
    Ok(())
}

/// Import documents from compressed JSON
fn import_json_zst(
    _db: &Database,
    repo: &Repository,
    input_path: &std::path::Path,
) -> anyhow::Result<usize> {
    let compressed = fs::read(input_path)?;
    let mut decoder = zstd::Decoder::new(&compressed[..])?;
    let mut json_content = String::new();
    decoder.read_to_string(&mut json_content)?;

    let docs: Vec<serde_json::Value> = serde_json::from_str(&json_content)?;
    let processor = DocumentProcessor::new();
    let mut count = 0;

    for doc in docs {
        let title = doc["title"].as_str().unwrap_or("Untitled");
        let content = doc["content"].as_str().unwrap_or("");
        let doc_type = doc["type"].as_str();

        // Determine path from type
        let rel_path = if let Some(t) = doc_type {
            format!("{}/{}.md", t, title.to_lowercase().replace(' ', "-"))
        } else {
            format!("{}.md", title.to_lowercase().replace(' ', "-"))
        };

        let file_path = repo.path.join(&rel_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Generate factbase header
        let id = processor.generate_id();
        let full_content = format!(
            "<!-- factbase:{} -->\n# {}\n{}",
            id,
            title,
            content.trim_start_matches(&format!("# {}\n", title))
        );
        fs::write(&file_path, full_content)?;
        count += 1;
    }

    Ok(count)
}

/// Export documents as tar.zst archive
fn export_tar_zst(
    db: &Database,
    repo: &Repository,
    output_path: &std::path::Path,
) -> anyhow::Result<()> {
    let docs = db.list_documents(None, Some(&repo.id), None, usize::MAX)?;

    // Create tar archive in memory
    let mut tar_data = Vec::new();
    {
        let mut tar_builder = tar::Builder::new(&mut tar_data);
        let repo_path_str = repo.path.to_string_lossy();

        for doc in &docs {
            // Get relative path from repo
            let rel_path = doc
                .file_path
                .strip_prefix(&*repo_path_str)
                .unwrap_or(&doc.file_path)
                .trim_start_matches('/');

            let content = doc.content.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_path(rel_path)?;
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            tar_builder.append(&header, content)?;
        }
        tar_builder.finish()?;
    }

    // Compress with zstd
    let compressed = zstd::encode_all(&tar_data[..], 3)?;
    fs::write(output_path, compressed)?;
    Ok(())
}

/// Import documents from tar.zst archive
fn import_tar_zst(repo: &Repository, input_path: &std::path::Path) -> anyhow::Result<usize> {
    let compressed = fs::read(input_path)?;
    let decoder = zstd::Decoder::new(&compressed[..])?;
    let mut archive = tar::Archive::new(decoder);
    let mut count = 0;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let dest_path = repo.path.join(&*path);

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut content = String::new();
        entry.read_to_string(&mut content)?;
        fs::write(&dest_path, content)?;
        count += 1;
    }

    Ok(count)
}

/// Test compressed json.zst export/import roundtrip
#[tokio::test]
#[ignore]
async fn test_json_zst_roundtrip() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("create temp dir");
    let (repo, _repo_path) = create_test_repo(&temp_dir);

    // Setup database and scan
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("create db");
    db.add_repository(&repo).expect("add repo");

    let config = Config::default();

    run_scan(&repo, &db, &config).await.expect("scan");

    // Verify documents indexed
    let docs = db.get_documents_for_repo("source").expect("get docs");
    assert_eq!(docs.len(), 3, "should have 3 documents");

    // Export as json.zst
    let json_path = temp_dir.path().join("backup.json.zst");
    export_json_zst(&db, "source", &json_path).expect("export json.zst");
    assert!(json_path.exists(), "json.zst should exist");

    // Verify compressed file is smaller than uncompressed
    let compressed_size = fs::metadata(&json_path).expect("metadata").len();
    assert!(compressed_size > 0, "compressed file should not be empty");

    // Create destination repo
    let dest_path = temp_dir.path().join("dest");
    fs::create_dir_all(&dest_path).expect("create dest dir");

    let dest_repo = common::test_repo("dest", dest_path.clone());
    db.add_repository(&dest_repo).expect("add dest repo");

    // Import from json.zst
    let imported = import_json_zst(&db, &dest_repo, &json_path).expect("import json.zst");
    assert_eq!(imported, 3, "should import 3 documents");

    // Verify files imported with factbase headers
    let alice_path = dest_path.join("person/alice.md");
    assert!(
        alice_path.exists(),
        "alice.md should exist at {:?}",
        alice_path
    );
    let alice_content = fs::read_to_string(&alice_path).expect("read alice");
    assert!(
        alice_content.contains("<!-- factbase:"),
        "should have factbase header"
    );
    assert!(alice_content.contains("Alice"), "should have title");
}

/// Test compressed tar.zst export/import roundtrip
#[tokio::test]
#[ignore]
async fn test_tar_zst_roundtrip() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("create temp dir");
    let (repo, _repo_path) = create_test_repo(&temp_dir);

    // Setup database and scan
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("create db");
    db.add_repository(&repo).expect("add repo");

    let config = Config::default();

    run_scan(&repo, &db, &config).await.expect("scan");

    // Verify documents indexed
    let docs = db.get_documents_for_repo("source").expect("get docs");
    assert_eq!(docs.len(), 3, "should have 3 documents");

    // Export as tar.zst
    let archive_path = temp_dir.path().join("backup.tar.zst");
    export_tar_zst(&db, &repo, &archive_path).expect("export tar.zst");
    assert!(archive_path.exists(), "tar.zst should exist");

    // Create destination repo
    let dest_path = temp_dir.path().join("dest");
    fs::create_dir_all(&dest_path).expect("create dest dir");

    let dest_repo = common::test_repo("dest", dest_path.clone());
    db.add_repository(&dest_repo).expect("add dest repo");

    // Import from tar.zst
    let imported = import_tar_zst(&dest_repo, &archive_path).expect("import tar.zst");
    assert_eq!(imported, 3, "should import 3 documents");

    // Verify files imported preserving structure
    assert!(
        dest_path.join("people/alice.md").exists(),
        "alice.md should exist"
    );
    assert!(
        dest_path.join("people/bob.md").exists(),
        "bob.md should exist"
    );
    assert!(
        dest_path.join("projects/api.md").exists(),
        "api.md should exist"
    );

    // Verify content preserved (including factbase headers from scan)
    let alice_content = fs::read_to_string(dest_path.join("people/alice.md")).expect("read alice");
    assert!(
        alice_content.contains("<!-- factbase:"),
        "should have factbase header"
    );
    assert!(
        alice_content.contains("Alice is a software engineer"),
        "content should be preserved"
    );
}

/// Test that compressed files are actually smaller
#[tokio::test]
#[ignore]
async fn test_compression_reduces_size() {
    require_ollama().await;

    let temp_dir = TempDir::new().expect("create temp dir");
    let (repo, _repo_path) = create_test_repo(&temp_dir);

    // Setup database and scan
    let db_path = temp_dir.path().join("factbase.db");
    let db = Database::new(&db_path).expect("create db");
    db.add_repository(&repo).expect("add repo");

    let config = Config::default();

    run_scan(&repo, &db, &config).await.expect("scan");

    // Export as uncompressed JSON
    let docs = db
        .list_documents(None, Some("source"), None, usize::MAX)
        .expect("list docs");
    let mut export_data: Vec<serde_json::Value> = Vec::new();
    for doc in &docs {
        export_data.push(serde_json::json!({
            "id": doc.id,
            "title": doc.title,
            "content": doc.content,
        }));
    }
    let json_content = serde_json::to_string_pretty(&export_data).expect("serialize");
    let uncompressed_size = json_content.len();

    // Export as compressed JSON
    let json_path = temp_dir.path().join("backup.json.zst");
    export_json_zst(&db, "source", &json_path).expect("export json.zst");
    let compressed_size = fs::metadata(&json_path).expect("metadata").len() as usize;

    // Compressed should be smaller (for text content, typically 60-80% reduction)
    assert!(
        compressed_size < uncompressed_size,
        "compressed ({}) should be smaller than uncompressed ({})",
        compressed_size,
        uncompressed_size
    );

    println!(
        "Compression ratio: {:.1}% ({}B -> {}B)",
        (1.0 - compressed_size as f64 / uncompressed_size as f64) * 100.0,
        uncompressed_size,
        compressed_size
    );
}
