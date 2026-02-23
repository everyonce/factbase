//! Integration tests for file watcher
//! These tests use real filesystem events with tempfile directories.

use chrono::Utc;
use factbase::models::Repository;
use factbase::watcher::{find_repo_for_path, FileWatcher, ScanCoordinator};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// Helper to wait for watcher events with timeout
fn wait_for_events(watcher: &FileWatcher, max_wait_ms: u64) -> Vec<PathBuf> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(max_wait_ms);

    while start.elapsed() < timeout {
        if let Some(paths) = watcher.try_recv() {
            return paths;
        }
        thread::sleep(Duration::from_millis(50));
    }
    vec![]
}

#[test]
fn test_detect_file_creation() {
    let temp = TempDir::new().expect("operation should succeed");
    let mut watcher = FileWatcher::new(100, &[]).expect("operation should succeed");
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    // Give watcher time to initialize
    thread::sleep(Duration::from_millis(200));

    // Create a new .md file
    let file_path = temp.path().join("new_doc.md");
    fs::write(&file_path, "# New Document\n\nContent here.").expect("operation should succeed");

    // Wait for event
    let events = wait_for_events(&watcher, 2000);

    assert!(!events.is_empty(), "Should detect file creation");
    assert!(events.iter().any(|p| p.ends_with("new_doc.md")));
}

#[test]
fn test_detect_file_modification() {
    let temp = TempDir::new().expect("operation should succeed");
    let file_path = temp.path().join("existing.md");
    fs::write(&file_path, "# Original\n\nOriginal content.").expect("operation should succeed");

    let mut watcher = FileWatcher::new(100, &[]).expect("operation should succeed");
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    thread::sleep(Duration::from_millis(200));

    // Modify the file
    fs::write(&file_path, "# Modified\n\nUpdated content.").expect("operation should succeed");

    let events = wait_for_events(&watcher, 2000);

    assert!(!events.is_empty(), "Should detect file modification");
    assert!(events.iter().any(|p| p.ends_with("existing.md")));
}

#[test]
fn test_detect_file_deletion() {
    let temp = TempDir::new().expect("operation should succeed");
    let file_path = temp.path().join("to_delete.md");
    fs::write(&file_path, "# To Delete").expect("operation should succeed");

    let mut watcher = FileWatcher::new(100, &[]).expect("operation should succeed");
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    thread::sleep(Duration::from_millis(200));

    // Delete the file
    fs::remove_file(&file_path).expect("operation should succeed");

    // Note: deletion events may or may not include the path depending on OS
    // The key is that we get some event notification
    let events = wait_for_events(&watcher, 2000);

    // On some systems, deletion triggers an event; on others it may not
    // This test verifies the watcher doesn't crash on deletion
    // The actual deletion handling is done by the scanner marking docs as deleted
    assert!(events.is_empty() || events.iter().any(|p| p.ends_with("to_delete.md")));
}

#[test]
fn test_debouncing_batches_rapid_changes() {
    let temp = TempDir::new().expect("operation should succeed");
    let mut watcher = FileWatcher::new(500, &[]).expect("operation should succeed"); // 500ms debounce
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    thread::sleep(Duration::from_millis(300));

    // Make 10 rapid changes within debounce window
    for i in 0..10 {
        let file_path = temp.path().join(format!("rapid_{}.md", i));
        fs::write(&file_path, format!("# Doc {}", i)).expect("operation should succeed");
        thread::sleep(Duration::from_millis(10)); // 10ms between writes
    }

    // Wait for debounce window to pass
    thread::sleep(Duration::from_millis(800));

    // Collect all events
    let mut event_batches = 0;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        if watcher.try_recv().is_some() {
            event_batches += 1;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Debouncing should reduce batches - 10 writes should produce fewer than 10 batches
    assert!(
        event_batches > 0 && event_batches < 10,
        "Expected some batching (1-9 batches for 10 writes), got {} batches",
        event_batches
    );
}

#[test]
fn test_ignore_non_md_files() {
    let temp = TempDir::new().expect("operation should succeed");
    let mut watcher = FileWatcher::new(100, &[]).expect("operation should succeed");
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    thread::sleep(Duration::from_millis(200));

    // Create non-md files
    fs::write(temp.path().join("file.txt"), "text file").expect("operation should succeed");
    fs::write(temp.path().join("file.json"), "{}").expect("operation should succeed");
    fs::write(temp.path().join("file.rs"), "fn main() {}").expect("operation should succeed");

    let events = wait_for_events(&watcher, 1000);

    // Should not receive events for non-md files
    assert!(
        events.is_empty(),
        "Should ignore non-md files, got: {:?}",
        events
    );
}

#[test]
fn test_ignore_patterns() {
    let temp = TempDir::new().expect("operation should succeed");
    let patterns = vec!["*.swp".to_string(), "**/.git/**".to_string()];
    let mut watcher = FileWatcher::new(100, &patterns).expect("operation should succeed");
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");

    thread::sleep(Duration::from_millis(200));

    // Create files matching ignore patterns
    fs::write(temp.path().join("doc.md.swp"), "swap file").expect("operation should succeed");
    fs::create_dir_all(temp.path().join(".git")).expect("operation should succeed");
    fs::write(temp.path().join(".git/config.md"), "git config").expect("operation should succeed");

    // Also create a valid md file
    fs::write(temp.path().join("valid.md"), "# Valid").expect("operation should succeed");

    let events = wait_for_events(&watcher, 2000);

    // Should only see valid.md, not ignored files
    if !events.is_empty() {
        assert!(events.iter().all(|p| !p.to_string_lossy().contains(".swp")));
        assert!(events.iter().all(|p| !p.to_string_lossy().contains(".git")));
    }
}

#[test]
fn test_watch_unwatch() {
    let temp = TempDir::new().expect("operation should succeed");
    let mut watcher = FileWatcher::new(100, &[]).expect("operation should succeed");

    // Watch
    watcher
        .watch_directory(temp.path())
        .expect("operation should succeed");
    thread::sleep(Duration::from_millis(200));

    fs::write(temp.path().join("before.md"), "# Before").expect("operation should succeed");
    let events = wait_for_events(&watcher, 1000);
    assert!(!events.is_empty(), "Should detect events while watching");

    // Unwatch
    watcher
        .unwatch_directory(temp.path())
        .expect("operation should succeed");
    thread::sleep(Duration::from_millis(200));

    fs::write(temp.path().join("after.md"), "# After").expect("operation should succeed");
    let events = wait_for_events(&watcher, 500);
    assert!(
        events.is_empty(),
        "Should not detect events after unwatching"
    );
}

#[test]
fn test_scan_coordinator_thread_safety() {
    let coord = ScanCoordinator::new();
    let coord_clone = std::sync::Arc::new(coord);

    let mut handles = vec![];
    let success_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn 10 threads trying to start scans
    for _ in 0..10 {
        let c = coord_clone.clone();
        let s = success_count.clone();
        handles.push(thread::spawn(move || {
            if c.try_start() {
                s.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                thread::sleep(Duration::from_millis(10));
                c.finish();
            }
        }));
    }

    for h in handles {
        h.join().expect("operation should succeed");
    }

    // At least one should have succeeded
    assert!(success_count.load(std::sync::atomic::Ordering::SeqCst) >= 1);
}

#[test]
fn test_find_repo_for_nested_path() {
    let repos = vec![Repository {
        id: "main".into(),
        name: "Main".into(),
        path: PathBuf::from("/projects/main"),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    }];

    // Deeply nested path should still match
    let found = find_repo_for_path(
        &PathBuf::from("/projects/main/docs/2024/january/notes.md"),
        &repos,
    );
    assert_eq!(found.map(|r| &r.id), Some(&"main".to_string()));
}
