use crate::database::Database;
use crate::embedding::OllamaEmbedding;
use crate::mcp::tools::{handle_tool_call, McpRequest, McpResponse};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::trace::TraceLayer;
use tracing::info;

pub struct AppState {
    pub db: Database,
    pub embedding: OllamaEmbedding,
}

pub struct McpServer {
    state: Arc<AppState>,
    host: String,
    port: u16,
}

impl McpServer {
    pub fn new(db: Database, embedding: OllamaEmbedding, host: &str, port: u16) -> Self {
        Self {
            state: Arc::new(AppState { db, embedding }),
            host: host.to_string(),
            port,
        }
    }

    pub async fn start(
        self,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = Router::new()
            .route("/health", get(health))
            .route("/mcp", post(mcp_handler))
            .layer(TraceLayer::new_for_http())
            .with_state(self.state);

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!("MCP server listening on http://{}", addr);

        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
                info!("MCP server shutting down");
            })
            .await?;

        Ok(())
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpRequest>,
) -> Result<Json<McpResponse>, StatusCode> {
    match handle_tool_call(&state.db, &state.embedding, request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!("MCP error: {}", e);
            Ok(Json(McpResponse::error(-32603, e.to_string())))
        }
    }
}
