use crate::error::FactbaseError;
use crate::models::Repository;
use glob::Pattern;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<Vec<PathBuf>>,
    ignore_patterns: Vec<Pattern>,
}

impl FileWatcher {
    pub fn new(debounce_ms: u64, ignore_patterns: &[String]) -> Result<Self, FactbaseError> {
        let (raw_tx, raw_rx) = channel::<Event>();
        let (debounced_tx, debounced_rx) = channel::<Vec<PathBuf>>();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    let _ = raw_tx.send(event);
                }
                Err(e) => warn!("Watcher error: {}", e),
            },
            notify::Config::default(),
        )?;

        let debounce = Duration::from_millis(debounce_ms);
        std::thread::spawn(move || debounce_loop(raw_rx, debounced_tx, debounce));

        let patterns = ignore_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        Ok(Self {
            watcher,
            receiver: debounced_rx,
            ignore_patterns: patterns,
        })
    }

    pub fn watch_directory(&mut self, path: &Path) -> Result<(), FactbaseError> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;
        info!("Watching directory: {}", path.display());
        Ok(())
    }

    pub fn unwatch_directory(&mut self, path: &Path) -> Result<(), FactbaseError> {
        self.watcher.unwatch(path)?;
        info!("Stopped watching: {}", path.display());
        Ok(())
    }

    pub fn try_recv(&self) -> Option<Vec<PathBuf>> {
        match self.receiver.try_recv() {
            Ok(paths) => {
                let filtered: Vec<PathBuf> = paths
                    .into_iter()
                    .filter(|p| self.should_process(p))
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(filtered)
                }
            }
            Err(_) => None,
        }
    }

    fn should_process(&self, path: &Path) -> bool {
        if path.extension().map(|e| e != "md").unwrap_or(true) {
            debug!("Ignoring non-markdown file: {}", path.display());
            return false;
        }

        let path_str = path.to_string_lossy();
        for pattern in &self.ignore_patterns {
            if pattern.matches(&path_str) {
                debug!("Ignoring file matching pattern: {}", path.display());
                return false;
            }
        }
        true
    }
}

/// Collects raw notify events over a debounce window and sends batched paths.
fn debounce_loop(raw_rx: Receiver<Event>, debounced_tx: Sender<Vec<PathBuf>>, debounce: Duration) {
    use std::collections::HashSet;

    loop {
        // Block until first event (or channel closed)
        let first = match raw_rx.recv() {
            Ok(event) => event,
            Err(_) => return, // channel closed
        };

        let mut paths = HashSet::new();
        for p in first.paths {
            paths.insert(p);
        }

        // Collect additional events within the debounce window
        let deadline = std::time::Instant::now() + debounce;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match raw_rx.recv_timeout(remaining) {
                Ok(event) => {
                    for p in event.paths {
                        paths.insert(p);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Send what we have, then exit
                    if !paths.is_empty() {
                        let _ = debounced_tx.send(paths.into_iter().collect());
                    }
                    return;
                }
            }
        }

        if !paths.is_empty() && debounced_tx.send(paths.into_iter().collect()).is_err() {
            return; // receiver dropped
        }
    }
}

/// Tracks scan state to prevent concurrent scans
pub struct ScanCoordinator {
    scanning: Arc<AtomicBool>,
}

impl ScanCoordinator {
    pub fn new() -> Self {
        Self {
            scanning: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Try to start a scan. Returns true if scan can proceed, false if already scanning.
    pub fn try_start(&self) -> bool {
        self.scanning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Mark scan as complete.
    pub fn finish(&self) {
        self.scanning.store(false, Ordering::SeqCst);
    }

    /// Check if a scan is in progress.
    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::SeqCst)
    }
}

impl Default for ScanCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Find which repository contains the given path.
pub fn find_repo_for_path<'a>(path: &Path, repos: &'a [Repository]) -> Option<&'a Repository> {
    repos
        .iter()
        .find(|&repo| path.starts_with(&repo.path))
        .map(|v| v as _)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_should_process_md_files() {
        let watcher = FileWatcher::new(500, &[]).unwrap();
        assert!(watcher.should_process(Path::new("/test/doc.md")));
        assert!(!watcher.should_process(Path::new("/test/doc.txt")));
        assert!(!watcher.should_process(Path::new("/test/doc")));
    }

    #[test]
    fn test_should_process_ignores_patterns() {
        let patterns = vec!["*.swp".to_string(), "**/.git/**".to_string()];
        let watcher = FileWatcher::new(500, &patterns).unwrap();
        assert!(!watcher.should_process(Path::new("/test/.git/config.md")));
        assert!(!watcher.should_process(Path::new("/test/doc.swp")));
        assert!(watcher.should_process(Path::new("/test/doc.md")));
    }

    #[test]
    fn test_scan_coordinator_prevents_concurrent() {
        let coord = ScanCoordinator::new();
        assert!(coord.try_start());
        assert!(coord.is_scanning());
        assert!(!coord.try_start());
        coord.finish();
        assert!(!coord.is_scanning());
        assert!(coord.try_start());
    }

    #[test]
    fn test_find_repo_for_path() {
        let repos = vec![
            Repository {
                id: "repo1".into(),
                name: "Repo 1".into(),
                path: PathBuf::from("/home/user/repo1"),
                perspective: None,
                created_at: Utc::now(),
                last_indexed_at: None,
                last_lint_at: None,
            },
            Repository {
                id: "repo2".into(),
                name: "Repo 2".into(),
                path: PathBuf::from("/home/user/repo2"),
                perspective: None,
                created_at: Utc::now(),
                last_indexed_at: None,
                last_lint_at: None,
            },
        ];

        let found = find_repo_for_path(Path::new("/home/user/repo1/docs/test.md"), &repos);
        assert_eq!(found.map(|r| &r.id), Some(&"repo1".to_string()));

        let found = find_repo_for_path(Path::new("/home/user/repo2/notes.md"), &repos);
        assert_eq!(found.map(|r| &r.id), Some(&"repo2".to_string()));

        let found = find_repo_for_path(Path::new("/other/path/file.md"), &repos);
        assert!(found.is_none());
    }
}
