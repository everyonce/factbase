use chrono::Utc;
use clap::{Parser, Subcommand};
use factbase::{
    find_repo_for_path, Config, Database, DocumentProcessor, EmbeddingProvider, FileWatcher,
    LinkDetector, McpServer, OllamaEmbedding, OllamaLlm, Repository, ScanCoordinator, ScanResult,
    Scanner,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "factbase", about = "Filesystem-based knowledge management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init(InitArgs),
    Scan(ScanArgs),
    Status(StatusArgs),
    Search(SearchArgs),
    Serve(ServeArgs),
}

#[derive(Parser)]
struct InitArgs {
    path: PathBuf,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    id: Option<String>,
}

#[derive(Parser)]
struct ScanArgs {
    #[arg(long)]
    repo: Option<String>,
}

#[derive(Parser)]
struct StatusArgs {
    #[arg(long)]
    repo: Option<String>,
}

#[derive(Parser)]
struct SearchArgs {
    query: String,
    #[arg(long, short = 't')]
    doc_type: Option<String>,
    #[arg(long, short = 'r')]
    repo: Option<String>,
    #[arg(long, short = 'l', default_value = "10")]
    limit: usize,
}

#[derive(Parser)]
struct ServeArgs {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value = "3000")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Init(args) => cmd_init(args)?,
        Commands::Scan(args) => cmd_scan(args).await?,
        Commands::Status(args) => cmd_status(args)?,
        Commands::Search(args) => cmd_search(args).await?,
        Commands::Serve(args) => cmd_serve(args).await?,
    }
    Ok(())
}

fn cmd_init(args: InitArgs) -> anyhow::Result<()> {
    let path = args.path.canonicalize().unwrap_or_else(|_| {
        std::fs::create_dir_all(&args.path).ok();
        args.path.clone()
    });
    let factbase_dir = path.join(".factbase");
    if factbase_dir.exists() {
        anyhow::bail!("Already initialized: {}", path.display());
    }
    std::fs::create_dir_all(&factbase_dir)?;

    // Create perspective.yaml
    let perspective_path = path.join("perspective.yaml");
    if !perspective_path.exists() {
        std::fs::write(&perspective_path, "# Factbase perspective\ntype: knowledge-base\n# organization: Your Org\n# focus: Your focus area\n")?;
    }

    // Initialize database
    let db_path = factbase_dir.join("factbase.db");
    let db = Database::new(&db_path)?;

    let repo_id = args.id.unwrap_or_else(|| "main".into());
    let repo_name = args.name.unwrap_or_else(|| {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "main".into())
    });
    let repo = Repository {
        id: repo_id,
        name: repo_name,
        path: path.clone(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
    };
    db.upsert_repository(&repo)?;

    println!("Initialized factbase at {}", path.display());
    println!("Next: Add markdown files and run `factbase scan`");
    Ok(())
}

async fn cmd_scan(args: ScanArgs) -> anyhow::Result<()> {
    let (db, repo) = find_repo(args.repo.as_deref())?;
    let config = Config::load(None)?;
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    let result = full_scan(&repo, &db, &scanner, &processor, &embedding, &link_detector).await?;
    println!("{}", result);
    if result.links_detected > 0 {
        println!("{} links detected", result.links_detected);
    }
    Ok(())
}

fn cmd_status(args: StatusArgs) -> anyhow::Result<()> {
    let (db, repo) = find_repo(args.repo.as_deref())?;
    let stats = db.get_stats(&repo.id)?;
    let db_path = repo.path.join(".factbase/factbase.db");
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    println!("Repository: {} ({})", repo.name, repo.id);
    println!("Path: {}", repo.path.display());
    println!(
        "Documents: {} active, {} deleted",
        stats.active, stats.deleted
    );
    if !stats.by_type.is_empty() {
        println!("By type:");
        for (t, c) in &stats.by_type {
            println!("  {}: {}", t, c);
        }
    }
    println!("Database: {} KB", db_size / 1024);
    Ok(())
}

async fn cmd_search(args: SearchArgs) -> anyhow::Result<()> {
    let (db, _repo) = find_repo(args.repo.as_deref())?;
    let config = Config::load(None)?;

    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    let query_embedding = embedding.generate(&args.query).await?;
    let results = db.search_semantic(
        &query_embedding,
        args.limit,
        args.doc_type.as_deref(),
        args.repo.as_deref(),
    )?;

    if results.is_empty() {
        println!("No results found for: {}", args.query);
        return Ok(());
    }

    for (i, r) in results.iter().enumerate() {
        println!("{}. {} ({:.0}%)", i + 1, r.title, r.relevance_score * 100.0);
        if let Some(t) = &r.doc_type {
            println!("   Type: {}", t);
        }
        println!("   Path: {}", r.file_path);
        println!("   ID: {}", r.id);
        if !r.snippet.is_empty() {
            println!("   {}", r.snippet.replace('\n', " "));
        }
        println!();
    }

    Ok(())
}

fn find_repo(repo_id: Option<&str>) -> anyhow::Result<(Database, Repository)> {
    // Look for .factbase in current directory or parents
    let mut dir = std::env::current_dir()?;
    loop {
        let factbase_dir = dir.join(".factbase");
        if factbase_dir.exists() {
            let db_path = factbase_dir.join("factbase.db");
            let db = Database::new(&db_path)?;
            let repos = db.list_repositories()?;
            let repo = if let Some(id) = repo_id {
                repos.into_iter().find(|r| r.id == id)
            } else {
                repos.into_iter().next()
            };
            if let Some(r) = repo {
                return Ok((db, r));
            }
            anyhow::bail!("No repository found");
        }
        if !dir.pop() {
            break;
        }
    }
    anyhow::bail!("Not in a factbase repository. Run `factbase init <path>` first.")
}

async fn full_scan(
    repo: &Repository,
    db: &Database,
    scanner: &Scanner,
    processor: &DocumentProcessor,
    embedding: &OllamaEmbedding,
    link_detector: &LinkDetector,
) -> anyhow::Result<ScanResult> {
    let files = scanner.find_markdown_files(&repo.path);
    let known = db.get_documents_for_repo(&repo.id)?;
    let mut seen = HashSet::new();
    let mut result = ScanResult::default();

    info!("Scanning {} files in {}", files.len(), repo.path.display());

    // Pass 1: Index documents and generate embeddings
    for path in files {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Skip {}: {}", path.display(), e);
                continue;
            }
        };

        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        let (id, content) = if let Some(id) = processor.extract_id(&content) {
            (id, content)
        } else {
            let id = processor.generate_unique_id(db);
            let new_content = processor.inject_header(&content, &id);
            std::fs::write(&path, &new_content)?;
            (id, new_content)
        };

        seen.insert(id.clone());
        let relative = path
            .strip_prefix(&repo.path)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let title = processor.extract_title(&content, &path);
        let doc_type = processor.derive_type(&path, &repo.path);

        let is_new = !known.contains_key(&id);
        let is_modified = known.get(&id).map(|d| d.file_hash != hash).unwrap_or(false);

        if is_new {
            result.added += 1;
        } else if is_modified {
            result.updated += 1;
        } else {
            result.unchanged += 1;
        }

        if is_new || is_modified {
            // Generate embedding
            let emb = embedding.generate(&content).await?;
            db.upsert_embedding(&id, &emb)?;

            let doc = factbase::Document {
                id,
                repo_id: repo.id.clone(),
                file_path: relative,
                file_hash: hash,
                title,
                doc_type: Some(doc_type),
                content,
                file_modified_at: std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(chrono::DateTime::from),
                indexed_at: Utc::now(),
                is_deleted: false,
            };
            db.upsert_document(&doc)?;
        }
    }

    // Mark deleted documents
    for (id, doc) in &known {
        if !seen.contains(id) && !doc.is_deleted {
            db.mark_deleted(id)?;
            result.deleted += 1;
        }
    }

    // Pass 2: Detect links using LLM
    let known_entities = db.get_all_document_titles()?;
    let all_docs = db.get_documents_for_repo(&repo.id)?;

    for (id, doc) in &all_docs {
        if doc.is_deleted {
            continue;
        }
        let links = link_detector
            .detect_links(&doc.content, id, &known_entities)
            .await?;
        result.links_detected += links.len();
        db.update_links(id, &links)?;
    }

    Ok(result)
}

async fn cmd_serve(args: ServeArgs) -> anyhow::Result<()> {
    let (db, _) = find_repo(None)?;
    let config = Config::load(None)?;
    let repos = db.list_repositories()?;

    if repos.is_empty() {
        anyhow::bail!("No repositories found. Run `factbase init <path>` first.");
    }

    // Create services
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    // Create file watcher
    let mut watcher =
        FileWatcher::new(config.watcher.debounce_ms, &config.watcher.ignore_patterns)?;

    // Watch all repository directories
    for repo in &repos {
        watcher.watch_directory(&repo.path)?;
    }

    // Create MCP server
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mcp_server = McpServer::new(db.clone(), embedding.clone(), &args.host, args.port);

    // Print startup banner
    println!("╔════════════════════════════════════════╗");
    println!("║         Factbase MCP Server            ║");
    println!("╠════════════════════════════════════════╣");
    println!("║ MCP endpoint: http://{}:{}/mcp", args.host, args.port);
    println!("║ Watching {} repository(ies)", repos.len());
    for repo in &repos {
        println!("║   - {} ({})", repo.name, repo.path.display());
    }
    println!("╠════════════════════════════════════════╣");
    println!("║ Ready for agent connections            ║");
    println!("║ Press Ctrl+C to stop                   ║");
    println!("╚════════════════════════════════════════╝");

    // Spawn MCP server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = mcp_server.start(shutdown_rx).await {
            error!("MCP server error: {}", e);
        }
    });

    // Create scan coordinator
    let scan_coordinator = ScanCoordinator::new();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();
    let llm = OllamaLlm::new(&config.llm.base_url, &config.llm.model);
    let link_detector = LinkDetector::new(Box::new(llm));

    // Event loop
    loop {
        // Check for file changes
        if let Some(changed_paths) = watcher.try_recv() {
            for path in &changed_paths {
                info!("File changed: {}", path.display());
            }

            // Find affected repository
            if let Some(path) = changed_paths.first() {
                if let Some(repo) = find_repo_for_path(path, &repos) {
                    if scan_coordinator.try_start() {
                        info!("Rescanning repository: {}", repo.id);
                        match full_scan(
                            &repo,
                            &db,
                            &scanner,
                            &processor,
                            &embedding,
                            &link_detector,
                        )
                        .await
                        {
                            Ok(result) => info!("Scan complete: {}", result),
                            Err(e) => error!("Scan error: {}", e),
                        }
                        scan_coordinator.finish();
                    } else {
                        info!("Scan already in progress, skipping");
                    }
                }
            }
        }

        // Check for shutdown signal (Ctrl+C)
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                let _ = shutdown_tx.send(());
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    // Wait for server to shut down
    let _ = server_handle.await;
    println!("Factbase server stopped");
    Ok(())
}
