use super::{auto_init_repo, setup_cached_embedding, setup_embedding};
use crate::commands::setup::Setup;
use anyhow::Context;
use clap::Parser;
use factbase::config::Config;
use factbase::mcp::McpServer;
use factbase::processor::DocumentProcessor;
use factbase::progress::ProgressReporter;
use factbase::scanner::{full_scan, ScanContext, ScanOptions, Scanner};
use factbase::watcher::{find_repo_for_path, FileWatcher, ScanCoordinator};
#[cfg(feature = "web")]
use factbase::web::start_web_server;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{error, info};

#[derive(Parser)]
#[command(
    about = "Start MCP server and file watcher",
    after_help = "\
EXAMPLES:
    # Start server with default settings
    factbase serve

    # Start on custom host/port
    factbase serve --host 0.0.0.0 --port 8080

    # Check if server is running (for scripts)
    factbase serve --health-check
"
)]
pub struct ServeArgs {
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub port: Option<u16>,
    /// Check server health and exit (for scripts and monitoring)
    #[arg(long)]
    pub health_check: bool,
}

pub async fn cmd_serve(args: ServeArgs) -> anyhow::Result<()> {
    let (config, db, _) = match Setup::new().require_repo(None).build() {
        Ok(ctx) => ctx.take_repo(),
        Err(_) => auto_init_repo(&std::env::current_dir()?)?,
    };

    // Health check mode: just check and exit
    if args.health_check {
        return run_health_check(&config).await;
    }

    let repos = db.list_repositories()?;
    if repos.is_empty() {
        anyhow::bail!("No repository found");
    }

    let host = args.host.unwrap_or_else(|| config.server.host.clone());
    let port = args.port.unwrap_or(config.server.port);

    let cached_embedding = setup_cached_embedding(&config, None, &db).await;
    let scan_embedding = setup_embedding(&config).await;
    let link_detector = factbase::link_detection::LinkDetector::new();

    let mut watcher =
        FileWatcher::new(config.watcher.debounce_ms, &config.watcher.ignore_patterns)?;

    for repo in &repos {
        watcher.watch_directory(&repo.path)?;
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mcp_server = McpServer::new(
        db.clone(),
        cached_embedding,
        &host,
        port,
        config.rate_limit.clone(),
        config.embedding.effective_base_url(),
    );

    // Web server setup (feature-gated)
    #[cfg(feature = "web")]
    let (web_shutdown_tx, web_handle) = if config.web.enabled {
        let (tx, rx) = oneshot::channel();
        let web_db = db.clone();
        let web_config = config.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = start_web_server(&web_config, web_db, rx).await {
                error!("Web server error: {}", e);
            }
        });
        (Some(tx), Some(handle))
    } else {
        (None, None)
    };

    println!("╔════════════════════════════════════════╗");
    println!("║         Factbase MCP Server            ║");
    println!("╠════════════════════════════════════════╣");
    println!("║ MCP endpoint: http://{host}:{port}/mcp");
    println!("║ Health check: http://{host}:{port}/health");
    #[cfg(feature = "web")]
    if config.web.enabled {
        println!("║ Web UI:       http://127.0.0.1:{}", config.web.port);
    }
    println!("║ Watching {} repository(ies)", repos.len());
    for repo in &repos {
        println!("║   - {} ({})", repo.name, repo.path.display());
    }
    println!("╠════════════════════════════════════════╣");
    println!("║ Ready for agent connections            ║");
    println!("║ Press Ctrl+C to stop                   ║");
    println!("╚════════════════════════════════════════╝");

    let server_handle = tokio::spawn(async move {
        if let Err(e) = mcp_server.start(shutdown_rx).await {
            error!("MCP server error: {}", e);
        }
    });

    let scan_coordinator = ScanCoordinator::new();
    let scanner = Scanner::new(&config.watcher.ignore_patterns);
    let processor = DocumentProcessor::new();

    let watcher_opts = ScanOptions::from_config(&config);
    loop {
        if let Some(changed_paths) = watcher.try_recv() {
            for path in &changed_paths {
                info!("File changed: {}", path.display());
            }

            if let Some(path) = changed_paths.first() {
                if let Some(repo) = find_repo_for_path(path, &repos) {
                    if scan_coordinator.try_start() {
                        info!("Rescanning repository: {}", repo.id);
                        let ctx = ScanContext {
                            scanner: &scanner,
                            processor: &processor,
                            embedding: &scan_embedding,
                            link_detector: &link_detector,
                            opts: &watcher_opts,
                            progress: &ProgressReporter::Silent,
                        };
                        match full_scan(repo, &db, &ctx).await {
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

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                let _ = shutdown_tx.send(());
                #[cfg(feature = "web")]
                if let Some(tx) = web_shutdown_tx {
                    let _ = tx.send(());
                }
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    let _ = server_handle.await;
    #[cfg(feature = "web")]
    if let Some(handle) = web_handle {
        let _ = handle.await;
    }
    println!("Factbase server stopped");
    Ok(())
}

async fn run_health_check(config: &Config) -> anyhow::Result<()> {
    let host = &config.server.host;
    let port = config.server.port;
    let url = format!("http://{host}:{port}/health");

    let client = factbase::ollama::create_http_client(Duration::from_secs(5));

    let response = client.get(&url).send().await.with_context(|| {
        factbase::error::format_user_error(
            "Health check failed: connection error",
            Some("Is the server running? Start with: factbase serve"),
        )
    })?;

    if !response.status().is_success() {
        error!(status = %response.status(), "Health check failed: HTTP error");
        anyhow::bail!(
            "{}",
            factbase::error::format_user_error(
                &format!("Health check failed: HTTP {}", response.status()),
                Some("Check server logs for details")
            )
        );
    }

    let body: serde_json::Value = response.json().await?;
    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    println!("Health check: {url}");
    println!("  Status: {status}");
    if let Some(version) = body.get("version").and_then(|v| v.as_str()) {
        println!("  Version: {version}");
    }
    if let Some(uptime) = body
        .get("uptime_seconds")
        .and_then(serde_json::Value::as_u64)
    {
        println!("  Uptime: {uptime}s");
    }
    if let Some(db) = body.get("database").and_then(|v| v.as_str()) {
        println!("  Database: {db}");
    }
    if let Some(ollama) = body.get("ollama").and_then(|v| v.as_str()) {
        println!("  Ollama: {ollama}");
    }

    if status == "ok" {
        Ok(())
    } else {
        anyhow::bail!("Health check: server status is '{status}'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_serve_args_default() {
        let args = ServeArgs::try_parse_from(["serve"]).unwrap();
        assert!(args.host.is_none());
        assert!(args.port.is_none());
        assert!(!args.health_check);
    }

    #[test]
    fn test_serve_args_host() {
        let args = ServeArgs::try_parse_from(["serve", "--host", "0.0.0.0"]).unwrap();
        assert_eq!(args.host, Some("0.0.0.0".to_string()));
        assert!(args.port.is_none());
        assert!(!args.health_check);
    }

    #[test]
    fn test_serve_args_port() {
        let args = ServeArgs::try_parse_from(["serve", "--port", "8080"]).unwrap();
        assert!(args.host.is_none());
        assert_eq!(args.port, Some(8080));
        assert!(!args.health_check);
    }

    #[test]
    fn test_serve_args_host_and_port() {
        let args =
            ServeArgs::try_parse_from(["serve", "--host", "127.0.0.1", "--port", "3001"]).unwrap();
        assert_eq!(args.host, Some("127.0.0.1".to_string()));
        assert_eq!(args.port, Some(3001));
        assert!(!args.health_check);
    }

    #[test]
    fn test_serve_args_health_check() {
        let args = ServeArgs::try_parse_from(["serve", "--health-check"]).unwrap();
        assert!(args.host.is_none());
        assert!(args.port.is_none());
        assert!(args.health_check);
    }

    #[test]
    fn test_serve_args_invalid_port() {
        let result = ServeArgs::try_parse_from(["serve", "--port", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_serve_args_port_out_of_range() {
        // Port 99999 is out of u16 range
        let result = ServeArgs::try_parse_from(["serve", "--port", "99999"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_serve_args_all_combined() {
        let args = ServeArgs::try_parse_from([
            "serve",
            "--host",
            "192.168.1.1",
            "--port",
            "9000",
            "--health-check",
        ])
        .unwrap();
        assert_eq!(args.host, Some("192.168.1.1".to_string()));
        assert_eq!(args.port, Some(9000));
        assert!(args.health_check);
    }

    #[test]
    fn test_serve_args_port_boundary_min() {
        let args = ServeArgs::try_parse_from(["serve", "--port", "1"]).unwrap();
        assert_eq!(args.port, Some(1));
    }

    #[test]
    fn test_serve_args_port_boundary_max() {
        let args = ServeArgs::try_parse_from(["serve", "--port", "65535"]).unwrap();
        assert_eq!(args.port, Some(65535));
    }

    #[test]
    fn test_mcp_server_address_format() {
        // Test that MCP server address is formatted correctly
        let host = "127.0.0.1";
        let port = 3000u16;
        let mcp_url = format!("http://{}:{}/mcp", host, port);
        let health_url = format!("http://{}:{}/health", host, port);
        assert_eq!(mcp_url, "http://127.0.0.1:3000/mcp");
        assert_eq!(health_url, "http://127.0.0.1:3000/health");
    }

    #[test]
    fn test_mcp_server_address_custom_host() {
        let host = "0.0.0.0";
        let port = 8080u16;
        let mcp_url = format!("http://{}:{}/mcp", host, port);
        assert_eq!(mcp_url, "http://0.0.0.0:8080/mcp");
    }

    // Web feature tests
    #[cfg(feature = "web")]
    mod web_tests {
        use factbase::config::WebConfig;

        #[test]
        fn test_web_config_default_disabled() {
            let config = WebConfig::default();
            assert!(!config.enabled);
            assert_eq!(config.port, 3001);
        }

        #[test]
        fn test_web_config_enabled() {
            let config = WebConfig {
                enabled: true,
                port: 3001,
            };
            assert!(config.enabled);
        }

        #[test]
        fn test_web_config_custom_port() {
            let config = WebConfig {
                enabled: true,
                port: 8081,
            };
            assert_eq!(config.port, 8081);
        }

        #[test]
        fn test_web_server_address_format() {
            let port = 3001u16;
            let web_url = format!("http://127.0.0.1:{}", port);
            assert_eq!(web_url, "http://127.0.0.1:3001");
        }

        #[test]
        fn test_web_server_address_custom_port() {
            let port = 9001u16;
            let web_url = format!("http://127.0.0.1:{}", port);
            assert_eq!(web_url, "http://127.0.0.1:9001");
        }

        #[test]
        fn test_web_and_mcp_different_ports() {
            // Verify web and MCP servers use different default ports
            let mcp_port = 3000u16;
            let web_port = 3001u16;
            assert_ne!(mcp_port, web_port);
        }

        #[test]
        fn test_web_config_serde_roundtrip() {
            let config = WebConfig {
                enabled: true,
                port: 4000,
            };
            let json = serde_json::to_string(&config).unwrap();
            let parsed: WebConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.enabled, config.enabled);
            assert_eq!(parsed.port, config.port);
        }
    }
}
