//! Web UI HTTP server.
//!
//! Axum-based server for the web UI, running on a separate port from the MCP server.

use super::api::documents::{get_document, get_document_links, get_document_preview, list_repos};
use super::api::organize::{
    approve_suggestion, assign_orphan, dismiss_suggestion, get_document_suggestions, list_orphans,
    list_suggestions,
};
use super::api::review::{
    get_document_questions, get_review_status, list_review_queue, post_answer, post_apply,
    post_approve_bulk, post_bulk_answer, post_check, post_scan,
};
use super::api::stats::{get_organize_stats, get_review_stats, get_stats};
use super::assets::{index_handler, static_handler};
use crate::config::Config;
use crate::database::Database;
use axum::{http::StatusCode, routing::get, routing::post, Json, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::info;

/// Shared state for web server handlers.
pub struct WebAppState {
    pub db: Database,
    pub start_time: Instant,
}

/// Web UI server.
pub struct WebServer {
    state: Arc<WebAppState>,
    port: u16,
}

impl WebServer {
    /// Create a new web server instance.
    pub fn new(db: Database, port: u16) -> Self {
        Self {
            state: Arc::new(WebAppState {
                db,
                start_time: Instant::now(),
            }),
            port,
        }
    }

    /// Start the web server with graceful shutdown support.
    pub async fn start(
        self,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = Router::new()
            // API routes
            .route("/api/health", get(health))
            // Stats API routes
            .route("/api/stats", get(get_stats))
            .route("/api/stats/review", get(get_review_stats))
            .route("/api/stats/organize", get(get_organize_stats))
            // Review API routes
            .route("/api/review/queue", get(list_review_queue))
            .route("/api/review/queue/{doc_id}", get(get_document_questions))
            .route("/api/review/answer/{doc_id}", post(post_answer))
            .route("/api/review/bulk-answer", post(post_bulk_answer))
            .route("/api/review/status", get(get_review_status))
            // Action API routes
            .route("/api/apply", post(post_apply))
            .route("/api/approve-bulk", post(post_approve_bulk))
            .route("/api/scan", post(post_scan))
            .route("/api/check", post(post_check))
            // Organize API routes
            .route("/api/organize/suggestions", get(list_suggestions))
            .route(
                "/api/organize/suggestions/{doc_id}",
                get(get_document_suggestions),
            )
            .route("/api/organize/approve", post(approve_suggestion))
            .route("/api/organize/dismiss", post(dismiss_suggestion))
            .route("/api/organize/orphans", get(list_orphans))
            .route("/api/organize/assign-orphan", post(assign_orphan))
            // Document API routes (read-only)
            .route("/api/documents/{id}", get(get_document))
            .route("/api/documents/{id}/links", get(get_document_links))
            .route("/api/documents/{id}/preview", get(get_document_preview))
            .route("/api/repos", get(list_repos))
            // Static asset routes - fallback after all API routes
            .route("/", get(index_handler))
            .fallback(static_handler)
            .with_state(self.state);

        let addr: SocketAddr = format!("127.0.0.1:{}", self.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!("Web UI server listening on http://{}", addr);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
            info!("Web UI server shutting down");
        })
        .await?;

        Ok(())
    }

    /// Get the server address.
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

/// Start the web server (convenience function).
///
/// # Arguments
/// * `config` - Application configuration
/// * `db` - Database connection
/// * `llm` - Optional LLM provider for apply endpoint
/// * `shutdown_rx` - Shutdown signal receiver
pub async fn start_web_server(
    config: &Config,
    db: Database,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = WebServer::new(db, config.web.port);
    server.start(shutdown_rx).await
}

/// Health check endpoint.
async fn health(
    axum::extract::State(state): axum::extract::State<Arc<WebAppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db_ok = state.db.health_check().is_ok();
    let uptime_secs = state.start_time.elapsed().as_secs();

    let status = if db_ok { "ok" } else { "error" };

    Ok(Json(serde_json::json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_secs,
        "database": if db_ok { "ok" } else { "error" }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;

    #[test]
    fn test_web_server_address() {
        let (db, _tmp) = test_db();
        let server = WebServer::new(db, 3001);
        assert_eq!(server.address(), "127.0.0.1:3001");
    }

    #[test]
    fn test_web_server_custom_port() {
        let (db, _tmp) = test_db();
        let server = WebServer::new(db, 8080);
        assert_eq!(server.address(), "127.0.0.1:8080");
    }
}
