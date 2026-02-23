use crate::error::FactbaseError;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

#[cfg(test)]
const DEFAULT_MAX_RETRIES: u32 = 3;
#[cfg(test)]
const DEFAULT_RETRY_DELAY_MS: u64 = 1000;
#[cfg(test)]
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Create an HTTP client with the given timeout.
///
/// Falls back to a default client if the builder fails (e.g., TLS backend issue).
pub fn create_http_client(timeout: Duration) -> Client {
    Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// Error context for Ollama failures
#[derive(Debug, Clone, PartialEq)]
enum OllamaErrorKind {
    ConnectionRefused,
    Timeout,
    ModelNotFound,
    ClientError(u16),
    ServerError(u16),
    ParseError,
    Other,
}

impl OllamaErrorKind {
    /// Returns true if this error type should trigger a retry
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            OllamaErrorKind::ConnectionRefused
                | OllamaErrorKind::Timeout
                | OllamaErrorKind::ServerError(_)
                | OllamaErrorKind::ParseError
                | OllamaErrorKind::Other
        )
    }
}

/// Shared HTTP client for Ollama API calls
pub(crate) struct OllamaClient {
    client: Client,
    base_url: String,
    timeout_secs: u64,
    max_retries: u32,
    retry_delay_ms: u64,
}

impl OllamaClient {
    pub(crate) fn with_config(
        base_url: &str,
        timeout_secs: u64,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> Self {
        let client = create_http_client(Duration::from_secs(timeout_secs));
        Self {
            client,
            base_url: base_url.to_string(),
            timeout_secs,
            max_retries,
            retry_delay_ms,
        }
    }

    #[cfg(test)]
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Make a POST request to Ollama with retry logic for transient errors
    pub(crate) async fn post<Req, Resp>(
        &self,
        endpoint: &str,
        request: &Req,
        model: &str,
    ) -> Result<Resp, FactbaseError>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut last_error = String::new();
        let mut error_kind = OllamaErrorKind::Other;
        let max_attempts = self.max_retries + 1; // +1 for initial attempt

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let delay = self.retry_delay_ms * (1 << (attempt - 1)); // exponential: 1x, 2x, 4x...
                debug!(
                    attempt = attempt + 1,
                    max_attempts = max_attempts,
                    delay_ms = delay,
                    "Retrying Ollama request"
                );
                sleep(Duration::from_millis(delay)).await;
            }

            let resp = match self.client.post(&url).json(request).send().await {
                Ok(r) => r,
                Err(e) => {
                    (last_error, error_kind) = classify_reqwest_error(&e);
                    warn!(
                        base_url = %self.base_url,
                        error = %e,
                        "Failed to connect to Ollama"
                    );
                    continue;
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let status_code = status.as_u16();
                last_error = format!("status {status}");
                error_kind = if status_code == 404 {
                    OllamaErrorKind::ModelNotFound
                } else if status.is_client_error() {
                    OllamaErrorKind::ClientError(status_code)
                } else {
                    OllamaErrorKind::ServerError(status_code)
                };
                warn!(
                    status = %status,
                    model = %model,
                    retryable = error_kind.is_retryable(),
                    "Ollama returned error status"
                );
                // Don't retry client errors (4xx) - they're permanent failures
                if !error_kind.is_retryable() {
                    break;
                }
                continue;
            }

            match resp.json().await {
                Ok(b) => return Ok(b),
                Err(e) => {
                    last_error = format!("parse error: {e}");
                    error_kind = OllamaErrorKind::ParseError;
                    warn!(error = %e, "Failed to parse Ollama response");
                    continue;
                }
            }
        }

        Err(format_ollama_error(
            &error_kind,
            &last_error,
            model,
            &self.base_url,
            self.timeout_secs,
            max_attempts,
        ))
    }
}

/// Classify a reqwest error into a specific error kind
fn classify_reqwest_error(e: &reqwest::Error) -> (String, OllamaErrorKind) {
    if e.is_timeout() {
        (format!("request timed out: {e}"), OllamaErrorKind::Timeout)
    } else if e.is_connect() {
        let msg = e.to_string();
        // Check for common connection refused patterns
        if msg.contains("Connection refused")
            || msg.contains("connection refused")
            || msg.contains("ConnectError")
        {
            (
                format!("connection refused: {e}"),
                OllamaErrorKind::ConnectionRefused,
            )
        } else {
            (format!("connection failed: {e}"), OllamaErrorKind::Other)
        }
    } else {
        (format!("request failed: {e}"), OllamaErrorKind::Other)
    }
}

/// Format a user-friendly error message with specific remediation steps
fn format_ollama_error(
    kind: &OllamaErrorKind,
    last_error: &str,
    model: &str,
    base_url: &str,
    timeout_secs: u64,
    max_attempts: u32,
) -> FactbaseError {
    let (summary, suggestion) = match kind {
        OllamaErrorKind::ConnectionRefused => (
            format!("Cannot connect to Ollama at {base_url}"),
            "Start Ollama with: ollama serve".to_string(),
        ),
        OllamaErrorKind::Timeout => (
            format!("Request timed out after {timeout_secs}s"),
            format!(
                "Increase timeout in config: embedding.timeout_secs or llm.timeout_secs (current: {timeout_secs})"
            ),
        ),
        OllamaErrorKind::ModelNotFound => (
            format!("Model '{model}' not found"),
            format!("Pull the model with: ollama pull {model}"),
        ),
        OllamaErrorKind::ClientError(status) => (
            format!("Ollama returned client error {status}"),
            "Check request parameters. Run 'factbase doctor' to diagnose.".to_string(),
        ),
        OllamaErrorKind::ServerError(status) => (
            format!("Ollama returned server error {status} after {max_attempts} attempts"),
            "Check Ollama logs for details. Run 'factbase doctor' to diagnose.".to_string(),
        ),
        OllamaErrorKind::ParseError => (
            "Failed to parse Ollama response".to_string(),
            "This may indicate a version mismatch. Try updating Ollama.".to_string(),
        ),
        OllamaErrorKind::Other => (
            format!("Ollama request failed after {max_attempts} attempts"),
            format!(
                "Ensure Ollama is running (ollama serve) and model '{model}' is available (ollama pull {model})"
            ),
        ),
    };

    FactbaseError::ollama(format!("{summary} ({last_error})\nhint: {suggestion}"))
}

#[cfg(test)]
impl OllamaClient {
    pub(crate) fn new(base_url: &str) -> Self {
        Self::with_config(
            base_url,
            DEFAULT_TIMEOUT_SECS,
            DEFAULT_MAX_RETRIES,
            DEFAULT_RETRY_DELAY_MS,
        )
    }
}

impl Clone for OllamaClient {
    fn clone(&self) -> Self {
        Self::with_config(
            &self.base_url,
            self.timeout_secs,
            self.max_retries,
            self.retry_delay_ms,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_client_new() {
        let client = OllamaClient::new("http://localhost:11434");
        assert_eq!(client.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_client_with_config() {
        let client = OllamaClient::with_config("http://test:1234", 60, 5, 2000);
        assert_eq!(client.base_url(), "http://test:1234");
        assert_eq!(client.timeout_secs, 60);
        assert_eq!(client.max_retries, 5);
        assert_eq!(client.retry_delay_ms, 2000);
    }

    #[test]
    fn test_ollama_client_clone() {
        let client = OllamaClient::with_config("http://test:1234", 45, 2, 500);
        let cloned = client.clone();
        assert_eq!(cloned.base_url(), "http://test:1234");
        assert_eq!(cloned.timeout_secs, 45);
        assert_eq!(cloned.max_retries, 2);
        assert_eq!(cloned.retry_delay_ms, 500);
    }

    #[test]
    fn test_format_ollama_error_connection_refused() {
        let err = format_ollama_error(
            &OllamaErrorKind::ConnectionRefused,
            "connection refused",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("Cannot connect to Ollama"));
        assert!(msg.contains("ollama serve"));
    }

    #[test]
    fn test_format_ollama_error_timeout() {
        let err = format_ollama_error(
            &OllamaErrorKind::Timeout,
            "timed out",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("timed out after 30s"));
        assert!(msg.contains("timeout_secs"));
    }

    #[test]
    fn test_format_ollama_error_model_not_found() {
        let err = format_ollama_error(
            &OllamaErrorKind::ModelNotFound,
            "status 404",
            "missing-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("Model 'missing-model' not found"));
        assert!(msg.contains("ollama pull missing-model"));
    }

    #[test]
    fn test_format_ollama_error_client_error() {
        let err = format_ollama_error(
            &OllamaErrorKind::ClientError(400),
            "bad request",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("client error 400"));
        assert!(msg.contains("factbase doctor"));
    }

    #[test]
    fn test_format_ollama_error_server_error() {
        let err = format_ollama_error(
            &OllamaErrorKind::ServerError(500),
            "internal error",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("server error 500"));
        assert!(msg.contains("4 attempts"));
        assert!(msg.contains("factbase doctor"));
    }

    #[test]
    fn test_format_ollama_error_parse_error() {
        let err = format_ollama_error(
            &OllamaErrorKind::ParseError,
            "invalid json",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("parse Ollama response"));
        assert!(msg.contains("updating Ollama"));
    }

    #[test]
    fn test_format_ollama_error_other() {
        let err = format_ollama_error(
            &OllamaErrorKind::Other,
            "unknown error",
            "test-model",
            "http://localhost:11434",
            30,
            4,
        );
        let msg = err.to_string();
        assert!(msg.contains("4 attempts"));
        assert!(msg.contains("ollama serve"));
        assert!(msg.contains("ollama pull test-model"));
    }

    #[test]
    fn test_error_kind_equality() {
        assert_eq!(OllamaErrorKind::Timeout, OllamaErrorKind::Timeout);
        assert_eq!(
            OllamaErrorKind::ServerError(500),
            OllamaErrorKind::ServerError(500)
        );
        assert_ne!(
            OllamaErrorKind::ServerError(404),
            OllamaErrorKind::ServerError(500)
        );
        assert_eq!(
            OllamaErrorKind::ClientError(400),
            OllamaErrorKind::ClientError(400)
        );
    }

    #[test]
    fn test_error_kind_is_retryable() {
        // Retryable errors
        assert!(OllamaErrorKind::ConnectionRefused.is_retryable());
        assert!(OllamaErrorKind::Timeout.is_retryable());
        assert!(OllamaErrorKind::ServerError(500).is_retryable());
        assert!(OllamaErrorKind::ServerError(503).is_retryable());
        assert!(OllamaErrorKind::ParseError.is_retryable());
        assert!(OllamaErrorKind::Other.is_retryable());

        // Non-retryable errors (4xx client errors)
        assert!(!OllamaErrorKind::ModelNotFound.is_retryable());
        assert!(!OllamaErrorKind::ClientError(400).is_retryable());
        assert!(!OllamaErrorKind::ClientError(401).is_retryable());
        assert!(!OllamaErrorKind::ClientError(403).is_retryable());
    }
}
