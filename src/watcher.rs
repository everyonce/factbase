use crate::error::FactbaseError;
use crate::models::Repository;
use glob::Pattern;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind, Debouncer};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct FileWatcher {
    debouncer: Debouncer<RecommendedWatcher>,
    receiver: Receiver<Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>>,
    ignore_patterns: Vec<Pattern>,
}

impl FileWatcher {
    pub fn new(debounce_ms: u64, ignore_patterns: &[String]) -> Result<Self, FactbaseError> {
        let (tx, rx) = channel();
        let debouncer = new_debouncer(Duration::from_millis(debounce_ms), tx)
            .map_err(|e| FactbaseError::Watcher(e.to_string()))?;

        let patterns = ignore_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        Ok(Self {
            debouncer,
            receiver: rx,
            ignore_patterns: patterns,
        })
    }

    pub fn watch_directory(&mut self, path: &Path) -> Result<(), FactbaseError> {
        self.debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| FactbaseError::Watcher(e.to_string()))?;
        info!("Watching directory: {}", path.display());
        Ok(())
    }

    pub fn unwatch_directory(&mut self, path: &Path) -> Result<(), FactbaseError> {
        self.debouncer
            .watcher()
            .unwatch(path)
            .map_err(|e| FactbaseError::Watcher(e.to_string()))?;
        info!("Stopped watching: {}", path.display());
        Ok(())
    }

    pub fn try_recv(&self) -> Option<Vec<PathBuf>> {
        match self.receiver.try_recv() {
            Ok(Ok(events)) => {
                let paths: Vec<PathBuf> = events
                    .into_iter()
                    .filter(|e| e.kind == DebouncedEventKind::Any)
                    .map(|e| e.path)
                    .filter(|p| self.should_process(p))
                    .collect();
                if paths.is_empty() {
                    None
                } else {
                    Some(paths)
                }
            }
            Ok(Err(e)) => {
                warn!("Watcher error: {}", e);
                None
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
    for repo in repos {
        if path.starts_with(&repo.path) {
            return Some(repo);
        }
    }
    None
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
            },
            Repository {
                id: "repo2".into(),
                name: "Repo 2".into(),
                path: PathBuf::from("/home/user/repo2"),
                perspective: None,
                created_at: Utc::now(),
                last_indexed_at: None,
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
