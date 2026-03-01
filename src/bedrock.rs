//! Amazon Bedrock provider implementations for embedding and LLM.

use crate::error::FactbaseError;
use crate::BoxFuture;
use crate::EmbeddingProvider;
use crate::LlmProvider;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message};
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tracing::debug;

/// Build a Bedrock runtime client with optional region and profile override.
async fn build_client(region: Option<&str>, profile: Option<&str>, timeout_secs: u64) -> Client {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = region {
        loader = loader.region(aws_config::Region::new(r.to_string()));
    }
    if let Some(p) = profile {
        loader = loader.profile_name(p);
    }
    let config = loader.load().await;
    let timeout = aws_sdk_bedrockruntime::config::timeout::TimeoutConfig::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .operation_timeout(std::time::Duration::from_secs(timeout_secs))
        .operation_attempt_timeout(std::time::Duration::from_secs(timeout_secs))
        .build();
    Client::from_conf(
        aws_sdk_bedrockruntime::Config::from(&config)
            .to_builder()
            .timeout_config(timeout)
            .build(),
    )
}

fn embed_err(ctx: &str, e: impl Display) -> FactbaseError {
    FactbaseError::embedding(format!("{ctx}: {e}"))
}

fn llm_err(ctx: &str, e: impl Display) -> FactbaseError {
    FactbaseError::llm(format!("{ctx}: {e}"))
}

/// Returns true if the error message indicates a connection-level failure
/// (GoAway, dispatch failure, connection reset) that requires a fresh client.
fn is_connection_error(msg: &str) -> bool {
    msg.contains("GoAway")
        || msg.contains("dispatch failure")
        || msg.contains("DispatchFailure")
        || msg.contains("connection reset")
}

/// Retry an async operation with exponential backoff.
/// Retries on throttling/timeout/connection errors, up to 3 attempts.
/// Calls `on_retry` before each retry — use this to rebuild clients on connection errors.
async fn retry_with_backoff<F, Fut, T, E, R, RFut>(mut f: F, mut on_retry: R) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    R: FnMut(bool) -> RFut,
    RFut: std::future::Future<Output = ()>,
{
    let mut delay = std::time::Duration::from_millis(500);
    for attempt in 0..3u32 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < 2 => {
                let msg = e.to_string();
                let conn_err = is_connection_error(&msg);
                if conn_err
                    || msg.contains("throttl")
                    || msg.contains("Throttl")
                    || msg.contains("TimedOut")
                    || msg.contains("service error")
                {
                    tracing::warn!("Retrying after error (attempt {}): {}", attempt + 1, msg);
                    on_retry(conn_err).await;
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
                return Err(e);
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Bedrock embedding provider supporting Titan and Nova models.
pub struct BedrockEmbedding {
    client: tokio::sync::Mutex<Client>,
    region: Option<String>,
    profile: Option<String>,
    timeout_secs: u64,
    model_id: String,
    dim: usize,
}

// --- Titan request/response ---
#[derive(Serialize)]
struct TitanEmbedRequest<'a> {
    #[serde(rename = "inputText")]
    input_text: &'a str,
    dimensions: usize,
    normalize: bool,
}

#[derive(Deserialize)]
struct TitanEmbedResponse {
    embedding: Vec<f64>,
}

// --- Nova request/response ---
#[derive(Serialize)]
struct NovaEmbedRequest<'a> {
    #[serde(rename = "taskType")]
    task_type: &'a str,
    #[serde(rename = "singleEmbeddingParams")]
    single_embedding_params: NovaEmbedParams<'a>,
}

#[derive(Serialize)]
struct NovaEmbedParams<'a> {
    #[serde(rename = "embeddingPurpose")]
    embedding_purpose: &'a str,
    #[serde(rename = "embeddingDimension")]
    embedding_dimension: usize,
    text: NovaTextInput<'a>,
}

#[derive(Serialize)]
struct NovaTextInput<'a> {
    #[serde(rename = "truncationMode")]
    truncation_mode: &'a str,
    value: &'a str,
}

#[derive(Deserialize)]
struct NovaEmbedResponse {
    embeddings: Vec<NovaEmbeddingEntry>,
}

#[derive(Deserialize)]
struct NovaEmbeddingEntry {
    embedding: Vec<f64>,
}

impl BedrockEmbedding {
    pub async fn new(
        model_id: &str,
        dimension: usize,
        region: Option<&str>,
        profile: Option<&str>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            client: tokio::sync::Mutex::new(build_client(region, profile, timeout_secs).await),
            region: region.map(String::from),
            profile: profile.map(String::from),
            timeout_secs,
            model_id: model_id.to_string(),
            dim: dimension,
        }
    }

    async fn rebuild_client(&self) {
        tracing::info!("Rebuilding Bedrock client (connection reset)");
        let new_client = build_client(
            self.region.as_deref(),
            self.profile.as_deref(),
            self.timeout_secs,
        )
        .await;
        *self.client.lock().await = new_client;
    }

    fn is_nova(&self) -> bool {
        self.model_id.contains("nova")
    }

    async fn invoke_embed(&self, text: &str) -> Result<Vec<f32>, FactbaseError> {
        let body = if self.is_nova() {
            let req = NovaEmbedRequest {
                task_type: "SINGLE_EMBEDDING",
                single_embedding_params: NovaEmbedParams {
                    embedding_purpose: "GENERIC_INDEX",
                    embedding_dimension: self.dim,
                    text: NovaTextInput {
                        truncation_mode: "END",
                        value: text,
                    },
                },
            };
            serde_json::to_vec(&req)
        } else {
            let req = TitanEmbedRequest {
                input_text: text,
                dimensions: self.dim,
                normalize: true,
            };
            serde_json::to_vec(&req)
        }
        .map_err(|e| embed_err("JSON serialize", e))?;

        let resp = self
            .client
            .lock()
            .await
            .invoke_model()
            .model_id(&self.model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(body))
            .send()
            .await
            .map_err(|e| embed_err("Bedrock invoke", format!("{e:#}")))?;

        let raw = resp.body().as_ref();

        let embedding: Vec<f32> = if self.is_nova() {
            let parsed: NovaEmbedResponse =
                serde_json::from_slice(raw).map_err(|e| embed_err("JSON deserialize", e))?;
            parsed
                .embeddings
                .into_iter()
                .next()
                .ok_or_else(|| FactbaseError::embedding("No embeddings in response"))?
                .embedding
                .into_iter()
                .map(|v| v as f32)
                .collect()
        } else {
            let parsed: TitanEmbedResponse =
                serde_json::from_slice(raw).map_err(|e| embed_err("JSON deserialize", e))?;
            parsed.embedding.into_iter().map(|v| v as f32).collect()
        };

        if embedding.len() != self.dim {
            return Err(FactbaseError::embedding(format!(
                "Expected {} dimensions, got {}",
                self.dim,
                embedding.len()
            )));
        }
        Ok(embedding)
    }
}

impl EmbeddingProvider for BedrockEmbedding {
    fn generate<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>, FactbaseError>> {
        Box::pin(async move {
            retry_with_backoff(
                || self.invoke_embed(text),
                |conn_err| async move {
                    if conn_err {
                        self.rebuild_client().await;
                    }
                },
            )
            .await
        })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(texts.len());
            for (i, text) in texts.iter().enumerate() {
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                results.push(
                    retry_with_backoff(
                        || self.invoke_embed(text),
                        |conn_err| async move {
                            if conn_err {
                                self.rebuild_client().await;
                            }
                        },
                    )
                    .await?,
                );
            }
            Ok(results)
        })
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

/// Bedrock LLM provider using the Converse API (model-agnostic).
pub struct BedrockLlm {
    client: tokio::sync::Mutex<Client>,
    region: Option<String>,
    profile: Option<String>,
    timeout_secs: u64,
    model_id: String,
}

impl BedrockLlm {
    pub async fn new(model_id: &str, region: Option<&str>, profile: Option<&str>, timeout_secs: u64) -> Self {
        Self {
            client: tokio::sync::Mutex::new(build_client(region, profile, timeout_secs).await),
            region: region.map(String::from),
            profile: profile.map(String::from),
            timeout_secs,
            model_id: model_id.to_string(),
        }
    }

    async fn rebuild_client(&self) {
        tracing::info!("Rebuilding Bedrock LLM client (connection reset)");
        let new_client = build_client(
            self.region.as_deref(),
            self.profile.as_deref(),
            self.timeout_secs,
        )
        .await;
        *self.client.lock().await = new_client;
    }

    pub fn model(&self) -> &str {
        &self.model_id
    }

    async fn invoke_converse(&self, prompt: &str) -> Result<String, FactbaseError> {
        debug!("Bedrock converse: model={}", self.model_id);

        let msg = Message::builder()
            .role(ConversationRole::User)
            .content(ContentBlock::Text(prompt.to_string()))
            .build()
            .map_err(|e| llm_err("Build message", e))?;

        let resp = self
            .client
            .lock()
            .await
            .converse()
            .model_id(&self.model_id)
            .messages(msg)
            .send()
            .await
            .map_err(|e| llm_err("Bedrock converse", format!("{e:?}")))?;

        let text = resp
            .output()
            .ok_or_else(|| FactbaseError::llm("No output in response"))?
            .as_message()
            .map_err(|_| FactbaseError::llm("Output not a message"))?
            .content()
            .first()
            .ok_or_else(|| FactbaseError::llm("No content in message"))?
            .as_text()
            .map_err(|_| FactbaseError::llm("Content is not text"))?
            .to_string();

        Ok(text)
    }
}

impl LlmProvider for BedrockLlm {
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
        Box::pin(async move {
            retry_with_backoff(
                || self.invoke_converse(prompt),
                |conn_err| async move {
                    if conn_err {
                        self.rebuild_client().await;
                    }
                },
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_titan_embed_request_serialization() {
        let req = TitanEmbedRequest {
            input_text: "hello",
            dimensions: 1024,
            normalize: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"inputText\":\"hello\""));
        assert!(json.contains("\"dimensions\":1024"));
    }

    #[test]
    fn test_nova_embed_request_serialization() {
        let req = NovaEmbedRequest {
            task_type: "SINGLE_EMBEDDING",
            single_embedding_params: NovaEmbedParams {
                embedding_purpose: "GENERIC_INDEX",
                embedding_dimension: 1024,
                text: NovaTextInput {
                    truncation_mode: "END",
                    value: "hello",
                },
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"taskType\":\"SINGLE_EMBEDDING\""));
        assert!(json.contains("\"embeddingDimension\":1024"));
        assert!(json.contains("\"value\":\"hello\""));
    }

    #[test]
    fn test_nova_embed_response_deserialization() {
        let json = r#"{"embeddings":[{"embeddingType":"TEXT","embedding":[0.1,0.2,0.3]}]}"#;
        let resp: NovaEmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.embeddings[0].embedding.len(), 3);
    }

    #[test]
    fn test_titan_embed_response_deserialization() {
        let json = r#"{"embedding":[0.1,0.2,0.3],"inputTextTokenCount":5}"#;
        let resp: TitanEmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.embedding.len(), 3);
    }

    #[test]
    fn test_is_nova_detection() {
        // Can't construct without async, but we can test the logic
        assert!("amazon.nova-2-multimodal-embeddings-v1:0".contains("nova"));
        assert!(!"amazon.titan-embed-text-v2:0".contains("nova"));
    }

    #[tokio::test]
    async fn test_retry_on_service_error() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 1 {
                        Err("service error: GoAway".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |_| async {},
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_on_goaway() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 1 {
                        Err("GoAway { error_code: NO_ERROR }".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |_| async {},
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_on_connection_reset() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 1 {
                        Err("connection reset by peer".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |_| async {},
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_no_retry_on_unknown_error() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err("access denied".to_string())
                }
            },
            |_| async {},
        )
        .await;
        assert!(result.is_err());
        // Should not retry — only 1 attempt
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_on_retry_called_with_conn_error_flag() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let rebuild_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let rc = rebuild_count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 1 {
                        Err("GoAway received".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |conn_err| {
                let rc = rc.clone();
                async move {
                    if conn_err {
                        rc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(rebuild_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_on_retry_not_called_for_throttle() {
        let rebuild_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let rc = rebuild_count.clone();
        let result: Result<u32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 1 {
                        Err("throttling: rate exceeded".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            |conn_err| {
                let rc = rc.clone();
                async move {
                    if conn_err {
                        rc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            },
        )
        .await;
        assert_eq!(result.unwrap(), 42);
        // on_retry called but conn_err=false, so rebuild_count stays 0
        assert_eq!(rebuild_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_is_connection_error() {
        assert!(is_connection_error("GoAway { error_code: NO_ERROR }"));
        assert!(is_connection_error("dispatch failure"));
        assert!(is_connection_error("DispatchFailure"));
        assert!(is_connection_error("connection reset by peer"));
        assert!(!is_connection_error("throttling: rate exceeded"));
        assert!(!is_connection_error("access denied"));
    }
}
