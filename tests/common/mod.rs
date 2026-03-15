//! Shared test utilities for integration tests.

pub mod fixtures;

use chrono::Utc;
use factbase::{
    config::Config,
    database::Database,
    embedding::OllamaEmbedding,
    mcp::McpServer,
    models::{Document, Perspective, Repository},
    processor::DocumentProcessor,
    scanner::{full_scan, ScanContext, ScanOptions, Scanner},
    LinkDetector, ScanResult,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::oneshot;

/// Create a `Repository` with standard test defaults.
/// Name is auto-capitalized from `id` (e.g., "test" → "Test").
#[allow(dead_code)]
pub fn test_repo(id: &str, path: PathBuf) -> Repository {
    let name = {
        let mut c = id.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().to_string() + c.as_str(),
        }
    };
    Repository {
        id: id.to_string(),
        name,
        path,
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    }
}

/// Standard `ScanOptions` for integration tests (all defaults).
#[allow(dead_code)]
pub fn test_scan_options() -> ScanOptions {
    ScanOptions::default()
}

/// `ScanOptions` with link detection skipped (faster tests).
#[allow(dead_code)]
pub fn test_scan_options_skip_links() -> ScanOptions {
    ScanOptions {
        skip_links: true,
        ..Default::default()
    }
}

/// Owns all scan components, provides `context()` to borrow them into a `ScanContext`.
#[allow(dead_code)]
pub struct TestScanSetup {
    pub config: Config,
    pub scanner: Scanner,
    pub processor: DocumentProcessor,
    pub embedding: OllamaEmbedding,
    pub link_detector: LinkDetector,
    pub opts: ScanOptions,
}

#[allow(dead_code)]
impl TestScanSetup {
    /// Create with default config and scan options.
    pub fn new() -> Self {
        Self::with_options(test_scan_options())
    }

    /// Create with custom scan options.
    pub fn with_options(opts: ScanOptions) -> Self {
        let config = Config::default();
        let scanner = Scanner::new(&config.watcher.ignore_patterns);
        let processor = DocumentProcessor::new();
        let embedding = OllamaEmbedding::new(
            &config.embedding.base_url,
            &config.embedding.model,
            config.embedding.dimension,
        );
        let link_detector = LinkDetector::new();
        Self {
            config,
            scanner,
            processor,
            embedding,
            link_detector,
            opts,
        }
    }

    /// Borrow all fields into a `ScanContext`.
    pub fn context(&self) -> ScanContext<'_> {
        ScanContext {
            scanner: &self.scanner,
            processor: &self.processor,
            embedding: &self.embedding,
            link_detector: &self.link_detector,
            opts: &self.opts,
            progress: &factbase::ProgressReporter::Silent,
        }
    }
}

/// Generate a random port in the range 30000-39999 for test servers.
#[allow(dead_code)]
pub fn random_port() -> u16 {
    let mut buf = [0u8; 2];
    getrandom::getrandom(&mut buf).expect("getrandom failed");
    30000 + (u16::from_le_bytes(buf) % 10000)
}

/// Compute SHA256 hash of content.
#[allow(dead_code)]
pub fn compute_hash(content: &str) -> String {
    factbase::content_hash(content)
}

/// Creates a test database in a temp directory.
/// Returns the database and temp dir (keep temp dir alive to preserve files).
#[allow(dead_code)]
pub fn create_test_db() -> (Database, TempDir) {
    let temp = TempDir::new().expect("create temp dir");
    let db_path = temp.path().join("test.db");
    let db = Database::new(&db_path).expect("create database");
    (db, temp)
}

/// Creates a test repository with markdown files.
/// `files` is a slice of (relative_path, content) tuples.
/// Returns the Repository and temp dir.
#[allow(dead_code)]
pub fn create_test_repo(id: &str, name: &str, files: &[(&str, &str)]) -> (Repository, TempDir) {
    let temp = TempDir::new().expect("create temp dir");

    for (path, content) in files {
        let file_path = temp.path().join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&file_path, content).expect("write file");
    }

    let mut repo = test_repo(id, temp.path().to_path_buf());
    repo.name = name.to_string();

    (repo, temp)
}

/// Test context with database, repository, and config.
/// Provides common setup for integration tests.
#[allow(dead_code)]
pub struct TestContext {
    pub db: Database,
    pub repo: Repository,
    pub config: Config,
    pub repo_path: PathBuf,
    _temp_dir: TempDir,
}

#[allow(dead_code)]
impl TestContext {
    /// Create a new test context with an empty repository.
    pub fn new(repo_id: &str) -> Self {
        Self::new_with_perspective(repo_id, None)
    }

    /// Create a new test context with a custom perspective.
    pub fn with_perspective(repo_id: &str, perspective: Perspective) -> Self {
        Self::new_with_perspective(repo_id, Some(perspective))
    }

    fn new_with_perspective(repo_id: &str, perspective: Option<Perspective>) -> Self {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_path = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_path).expect("create repo dir");

        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(&db_path).expect("create database");

        let mut repo = test_repo(repo_id, repo_path.clone());
        repo.perspective = perspective;
        db.upsert_repository(&repo).expect("add repository");

        Self {
            db,
            repo,
            config: Config::default(),
            repo_path,
            _temp_dir: temp_dir,
        }
    }

    /// Create a new test context with sample markdown files.
    pub fn with_files(repo_id: &str, files: &[(&str, &str)]) -> Self {
        let ctx = Self::new(repo_id);
        for (path, content) in files {
            let file_path = ctx.repo_path.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&file_path, content).expect("write file");
        }
        ctx
    }

    /// Create a new test context with sample markdown files and a custom perspective.
    pub fn with_files_and_perspective(
        repo_id: &str,
        files: &[(&str, &str)],
        perspective: Perspective,
    ) -> Self {
        let ctx = Self::with_perspective(repo_id, perspective);
        for (path, content) in files {
            let file_path = ctx.repo_path.join(path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&file_path, content).expect("write file");
        }
        ctx
    }

    /// Run a full scan on the repository.
    pub async fn scan(&self) -> anyhow::Result<ScanResult> {
        run_scan(&self.repo, &self.db, &self.config).await
    }

    /// Create an embedding provider using config settings.
    pub fn embedding(&self) -> OllamaEmbedding {
        OllamaEmbedding::new(
            &self.config.embedding.base_url,
            &self.config.embedding.model,
            self.config.embedding.dimension,
        )
    }
}

/// Run a full scan on a repository.
/// This is the common scan helper used across integration tests.
#[allow(dead_code)]
pub async fn run_scan(
    repo: &Repository,
    db: &Database,
    _config: &Config,
) -> anyhow::Result<ScanResult> {
    let setup = TestScanSetup::new();
    full_scan(repo, db, &setup.context()).await
}

/// Test MCP server with HTTP client for integration tests.
#[allow(dead_code)]
pub struct TestServer {
    pub client: Client,
    pub base_url: String,
    pub db: Database,
    pub repo_path: std::path::PathBuf,
    _shutdown_tx: oneshot::Sender<()>,
    _temp_dir: TempDir,
}

#[allow(dead_code)]
impl TestServer {
    /// Start a test server with empty database.
    pub async fn start() -> Self {
        Self::start_internal(false).await
    }

    /// Start a test server with sample documents.
    pub async fn start_with_data() -> Self {
        Self::start_internal(true).await
    }

    async fn start_internal(with_data: bool) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();

        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        // Add test repository
        let repo = Repository {
            id: "test-repo".into(),
            name: "Test Repo".into(),
            path: repo_path.clone(),
            perspective: Some(Perspective {
                type_name: "personal".into(),
                organization: None,
                focus: Some("testing".into()),
                allowed_types: None,
                review: None,
                format: None,
                link_match_mode: None,
            }),
            created_at: Utc::now(),
            last_indexed_at: None,
            last_lint_at: None,
        };
        db.upsert_repository(&repo).unwrap();

        if with_data {
            let docs = vec![
                (
                    "doc1",
                    "Alice Smith",
                    "person",
                    "Alice is a software engineer.",
                ),
                (
                    "doc2",
                    "Project Alpha",
                    "project",
                    "A project about testing.",
                ),
                ("doc3", "Bob Jones", "person", "Bob works with Alice."),
            ];

            for (id, title, doc_type, content) in docs {
                let doc = Document {
                    id: id.into(),
                    repo_id: "test-repo".into(),
                    title: title.into(),
                    doc_type: Some(doc_type.into()),
                    file_path: format!("{}/{}.md", doc_type, id),
                    content: content.into(),
                    file_hash: "abc123".into(),
                    file_modified_at: None,
                    indexed_at: Utc::now(),
                    is_deleted: false,
                };
                db.upsert_document(&doc).unwrap();
            }
        }

        let config = Config::default();
        let embedding = OllamaEmbedding::new(
            &config.embedding.base_url,
            &config.embedding.model,
            config.embedding.dimension,
        );

        let port = random_port();
        let server = McpServer::new(
            db.clone(),
            embedding,
            "127.0.0.1",
            port,
            config.rate_limit.clone(),
            &config.embedding.base_url,
        );
        let base_url = format!("http://127.0.0.1:{}", port);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        tokio::spawn(async move {
            server.start(shutdown_rx).await.ok();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            client,
            base_url,
            db,
            repo_path,
            _shutdown_tx: shutdown_tx,
            _temp_dir: temp_dir,
        }
    }

    /// Check server health.
    pub async fn health(&self) -> reqwest::Result<reqwest::Response> {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
    }

    /// Call an MCP tool.
    pub async fn call_tool(&self, tool: &str, args: Value) -> reqwest::Result<Value> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": args
            }
        });

        let resp = self
            .client
            .post(format!("{}/mcp", self.base_url))
            .json(&request)
            .send()
            .await?;

        resp.json().await
    }
}
