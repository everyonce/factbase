use crate::config::RateLimitConfig;
use crate::database::Database;
use crate::embedding::EmbeddingProvider;
use crate::mcp::initialize_result;
use crate::mcp::tools::{handle_tool_call, McpRequest, McpResponse};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{info, Level};

pub struct AppState<E: EmbeddingProvider> {
    pub db: Database,
    pub embedding: E,
    pub start_time: Instant,
    pub ollama_base_url: String,
    pub session_id: Mutex<Option<String>>,
}

pub struct McpServer<E: EmbeddingProvider> {
    state: Arc<AppState<E>>,
    host: String,
    port: u16,
    rate_limit: RateLimitConfig,
}

impl<E: EmbeddingProvider + 'static> McpServer<E> {
    pub fn new(
        db: Database,
        embedding: E,
        host: &str,
        port: u16,
        rate_limit: RateLimitConfig,
        ollama_base_url: &str,
    ) -> Self {
        Self {
            state: Arc::new(AppState {
                db,
                embedding,
                start_time: Instant::now(),
                ollama_base_url: ollama_base_url.to_string(),
                session_id: Mutex::new(None),
            }),
            host: host.to_string(),
            port,
            rate_limit,
        }
    }

    pub async fn start(
        self,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let rate_limiter = RateLimiter::new(self.rate_limit.per_second, self.rate_limit.burst_size);

        let app = Router::new()
            .route("/health", get(health::<E>))
            .route(
                "/mcp",
                get(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(mcp_handler::<E>),
            )
            .layer(axum::middleware::from_fn_with_state(
                Arc::new(rate_limiter),
                rate_limit_middleware,
            ))
            .layer(
                TraceLayer::new_for_http().on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .with_state(self.state);

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!("MCP server listening on http://{}", addr);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
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

async fn health<E: EmbeddingProvider>(
    State(state): State<Arc<AppState<E>>>,
) -> Json<serde_json::Value> {
    let db_ok = state.db.health_check().is_ok();
    let uptime_secs = state.start_time.elapsed().as_secs();

    // Check Ollama connectivity with short timeout
    let ollama_ok = check_ollama(&state.ollama_base_url).await;

    // Get connection pool stats
    let pool_stats = state.db.pool_stats();

    let status = if db_ok && ollama_ok {
        "ok"
    } else if db_ok {
        "degraded"
    } else {
        "error"
    };

    Json(serde_json::json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_secs,
        "database": if db_ok { "ok" } else { "error" },
        "ollama": if ollama_ok { "ok" } else { "unavailable" },
        "pool": {
            "connections": pool_stats.connections,
            "idle_connections": pool_stats.idle_connections,
            "max_size": pool_stats.max_size
        }
    }))
}

async fn check_ollama(base_url: &str) -> bool {
    let client = crate::ollama::create_http_client(std::time::Duration::from_secs(2));
    client
        .get(format!("{base_url}/api/tags"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Generate a UUID v4 string using `getrandom`.
fn generate_session_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    // Set version (4) and variant (RFC 4122) bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        // Last 6 bytes as a single hex string
        ((bytes[10] as u64) << 40)
            | ((bytes[11] as u64) << 32)
            | ((bytes[12] as u64) << 24)
            | ((bytes[13] as u64) << 16)
            | ((bytes[14] as u64) << 8)
            | (bytes[15] as u64)
    )
}

async fn mcp_handler<E: EmbeddingProvider>(
    State(state): State<Arc<AppState<E>>>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Result<Response, StatusCode> {
    // Validate Mcp-Session-Id if client sends one
    if let Some(client_sid) = headers.get("mcp-session-id") {
        let stored = state.session_id.lock().expect("session lock poisoned");
        if let Some(ref server_sid) = *stored {
            if client_sid.as_bytes() != server_sid.as_bytes() {
                return Err(StatusCode::CONFLICT);
            }
        }
    }

    // Handle initialize before routing to tool dispatch
    if request.method == "initialize" {
        let id = request.id.unwrap_or(Value::Null);
        let sid = generate_session_id();
        *state.session_id.lock().expect("session lock poisoned") = Some(sid.clone());
        let mut headers = HeaderMap::new();
        headers.insert(
            "mcp-session-id",
            sid.parse().expect("session id is valid header"),
        );
        return Ok((headers, Json(McpResponse::success(id, initialize_result()))).into_response());
    }

    // Handle ping before routing to tool dispatch
    if request.method == "ping" {
        let id = request.id.unwrap_or(Value::Null);
        return Ok(Json(McpResponse::success(id, serde_json::json!({}))).into_response());
    }

    // Notifications have no id — return 202 Accepted with no body
    if request.is_notification() {
        return Err(StatusCode::ACCEPTED);
    }

    let method = request.method.clone();
    let tool_name = request.params.name.clone().unwrap_or_default();
    let start = Instant::now();

    let result = handle_tool_call(&state.db, &state.embedding, request, None).await;
    let duration = start.elapsed();

    match &result {
        Ok(_) => {
            if method == "tools/call" {
                info!(tool = %tool_name, duration_ms = %duration.as_millis(), "MCP tool call");
            }
        }
        Err(e) => {
            tracing::error!(tool = %tool_name, error = %e, "MCP error");
        }
    }

    match result {
        Ok(Some(response)) => Ok(Json(response).into_response()),
        Ok(None) => Err(StatusCode::ACCEPTED),
        Err(e) => Ok(Json(McpResponse::error(-32603, e.to_string())).into_response()),
    }
}

/// Simple sliding-window rate limiter.
struct RateLimiter {
    window: Mutex<VecDeque<Instant>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    fn new(per_second: u64, burst_size: u32) -> Self {
        Self {
            window: Mutex::new(VecDeque::with_capacity(burst_size as usize)),
            max_requests: burst_size,
            window_secs: per_second.max(1),
        }
    }

    fn check(&self) -> bool {
        let mut window = self.window.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(self.window_secs);
        while window.front().is_some_and(|&t| t < cutoff) {
            window.pop_front();
        }
        if window.len() < self.max_requests as usize {
            window.push_back(now);
            true
        } else {
            false
        }
    }
}

async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    if limiter.check() {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;
    use crate::error::FactbaseError;
    use std::pin::Pin;

    /// Stub embedding provider for HTTP transport tests (no Ollama needed).
    struct StubEmbedding;

    impl EmbeddingProvider for StubEmbedding {
        fn generate<'a>(
            &'a self,
            _text: &'a str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<f32>, FactbaseError>> + Send + 'a>>
        {
            Box::pin(async { Ok(vec![0.0; 1024]) })
        }
        fn dimension(&self) -> usize {
            1024
        }
    }

    /// Start a test server on a random port, returning (base_url, shutdown_sender).
    async fn start_test_server() -> (String, oneshot::Sender<()>, tempfile::TempDir) {
        let (db, tmp) = test_db();
        let mut buf = [0u8; 2];
        getrandom::getrandom(&mut buf).expect("getrandom failed");
        let port = 30000 + (u16::from_le_bytes(buf) % 10000);
        let server = McpServer::new(
            db,
            StubEmbedding,
            "127.0.0.1",
            port,
            RateLimitConfig::default(),
            "http://localhost:11434",
        );
        let base_url = format!("http://127.0.0.1:{}", port);
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            server.start(rx).await.ok();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        (base_url, tx, tmp)
    }

    #[tokio::test]
    async fn test_http_initialize_returns_result_and_session_id() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}
            }))
            .send()
            .await
            .expect("request should succeed");

        // Mcp-Session-Id header present
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .expect("session id header");
        assert!(!sid.is_empty());

        let body: Value = resp.json().await.expect("json parse");
        assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
        assert!(body["result"]["serverInfo"]["name"]
            .as_str()
            .unwrap()
            .contains("factbase"));
        assert_eq!(body["id"], 1);
    }

    #[tokio::test]
    async fn test_http_tools_list_returns_tools() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();
        let body: Value = client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))
            .send()
            .await
            .expect("request should succeed")
            .json()
            .await
            .expect("json parse");

        let tools = body["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        assert_eq!(body["id"], 2);
    }

    #[tokio::test]
    async fn test_http_get_mcp_returns_405() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/mcp", base_url))
            .send()
            .await
            .expect("request should succeed");
        assert_eq!(resp.status(), 405);
    }

    #[tokio::test]
    async fn test_http_notification_returns_202() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .send()
            .await
            .expect("request should succeed");
        assert_eq!(resp.status(), 202);
    }

    #[tokio::test]
    async fn test_http_ping_returns_empty_result() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();
        let body: Value = client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({"jsonrpc":"2.0","id":3,"method":"ping"}))
            .send()
            .await
            .expect("request should succeed")
            .json()
            .await
            .expect("json parse");

        assert_eq!(body["result"], serde_json::json!({}));
        assert_eq!(body["id"], 3);
    }

    #[tokio::test]
    async fn test_http_session_id_mismatch_returns_409() {
        let (base_url, _tx, _tmp) = start_test_server().await;
        let client = reqwest::Client::new();

        // First, initialize to establish a session
        client
            .post(format!("{}/mcp", base_url))
            .json(&serde_json::json!({
                "jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}
            }))
            .send()
            .await
            .expect("initialize should succeed");

        // Send request with wrong session ID
        let resp = client
            .post(format!("{}/mcp", base_url))
            .header("mcp-session-id", "wrong-session-id")
            .json(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"ping"}))
            .send()
            .await
            .expect("request should succeed");
        assert_eq!(resp.status(), 409);
    }

    #[test]
    fn test_generate_session_id_is_valid_uuid_v4() {
        let sid = generate_session_id();
        // UUID format: 8-4-4-4-12 hex chars
        let parts: Vec<&str> = sid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // Version nibble is '4'
        assert!(parts[2].starts_with('4'));
        // Variant nibble is 8, 9, a, or b
        let variant = u8::from_str_radix(&parts[3][..1], 16).unwrap();
        assert!((8..=11).contains(&variant));
    }

    #[test]
    fn test_generate_session_id_unique() {
        let a = generate_session_id();
        let b = generate_session_id();
        assert_ne!(a, b);
    }

    #[test]
    fn test_json_responses_have_content_type() {
        use axum::http::header::CONTENT_TYPE;

        let resp = McpResponse::success(Value::from(1), serde_json::json!({}));

        // Json(T).into_response() — used by ping, tool success, tool error paths
        let r = Json(resp).into_response();
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), "application/json");

        // (HeaderMap, Json(T)).into_response() — used by initialize path
        let mut hdrs = HeaderMap::new();
        hdrs.insert("mcp-session-id", "test".parse().unwrap());
        let resp2 = McpResponse::success(Value::from(1), initialize_result());
        let r2 = (hdrs, Json(resp2)).into_response();
        assert_eq!(r2.headers().get(CONTENT_TYPE).unwrap(), "application/json");
    }
}
