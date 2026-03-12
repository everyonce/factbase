//! Amazon Bedrock embedding provider.

use crate::error::FactbaseError;
use crate::BoxFuture;
use crate::EmbeddingProvider;
use aws_sdk_bedrockruntime::error::SdkError;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

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

/// Returns true if the error message indicates a transient connection-level failure.
fn is_connection_error(msg: &str) -> bool {
    msg.contains("GoAway")
        || msg.contains("dispatch failure")
        || msg.contains("DispatchFailure")
        || msg.contains("connection reset")
}

/// Extract a useful error message from an AWS SDK error.
/// `SdkError::ServiceError` Display just says "service error" — we need the inner message.
fn sdk_error_message<E: Display, R>(e: &SdkError<E, R>) -> String {
    match e {
        SdkError::ServiceError(se) => se.err().to_string(),
        other => format!("{other:#}"),
    }
}

/// Retry an async operation with exponential backoff.
/// Retries on throttling/timeout/connection errors, up to 3 attempts.
async fn retry_with_backoff<F, Fut, T, E>(mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = std::time::Duration::from_millis(500);
    for attempt in 0..3u32 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < 2 => {
                let msg = e.to_string();
                if is_connection_error(&msg)
                    || msg.contains("throttl")
                    || msg.contains("Throttl")
                    || msg.contains("TimedOut")
                    || msg.contains("ModelErrorException")
                {
                    tracing::warn!("Retrying after error (attempt {}): {}", attempt + 1, msg);
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

/// Maximum input characters for Bedrock embedding models.
const NOVA_EMBED_MAX_CHARS: usize = 50_000;
const TITAN_EMBED_MAX_CHARS: usize = 8_000;

/// Bedrock embedding provider supporting Titan and Nova models.
pub struct BedrockEmbedding {
    client: Client,
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
            client: build_client(region, profile, timeout_secs).await,
            model_id: model_id.to_string(),
            dim: dimension,
        }
    }

    fn is_nova(&self) -> bool {
        self.model_id.contains("nova")
    }

    fn max_input_chars(&self) -> usize {
        if self.is_nova() {
            NOVA_EMBED_MAX_CHARS
        } else {
            TITAN_EMBED_MAX_CHARS
        }
    }

    async fn invoke_embed(&self, text: &str) -> Result<Vec<f32>, FactbaseError> {
        // Clamp input to model's character limit to avoid ValidationException
        let max = self.max_input_chars();
        let clamped = if text.len() > max {
            tracing::debug!(
                "Clamping embedding input from {} to {} chars",
                text.len(),
                max
            );
            &text[..text.floor_char_boundary(max)]
        } else {
            text
        };

        let body = if self.is_nova() {
            let req = NovaEmbedRequest {
                task_type: "SINGLE_EMBEDDING",
                single_embedding_params: NovaEmbedParams {
                    embedding_purpose: "GENERIC_INDEX",
                    embedding_dimension: self.dim,
                    text: NovaTextInput {
                        truncation_mode: "END",
                        value: clamped,
                    },
                },
            };
            serde_json::to_vec(&req)
        } else {
            let req = TitanEmbedRequest {
                input_text: clamped,
                dimensions: self.dim,
                normalize: true,
            };
            serde_json::to_vec(&req)
        }
        .map_err(|e| embed_err("JSON serialize", e))?;

        let resp = self
            .client
            .invoke_model()
            .model_id(&self.model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(body))
            .send()
            .await
            .map_err(|e| embed_err("Bedrock invoke", sdk_error_message(&e)))?;

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
        Box::pin(async move { retry_with_backoff(|| self.invoke_embed(text)).await })
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
                results.push(retry_with_backoff(|| self.invoke_embed(text)).await?);
            }
            Ok(results)
        })
    }

    fn dimension(&self) -> usize {
        self.dim
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
    async fn test_retry_on_goaway() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 1 {
                    Err("GoAway { error_code: NO_ERROR }".to_string())
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_on_connection_reset() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 1 {
                    Err("connection reset by peer".to_string())
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_no_retry_on_unknown_error() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("access denied".to_string())
            }
        })
        .await;
        assert!(result.is_err());
        // Should not retry — only 1 attempt
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_no_retry_on_service_error() {
        // "service error" (e.g. ValidationException) should NOT be retried
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("Malformed input request: expected maxLength: 50000, actual: 79999".to_string())
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_on_throttle() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 1 {
                    Err("throttling: rate exceeded".to_string())
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_on_model_error_exception() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        let result: Result<u32, String> = retry_with_backoff(|| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 1 {
                    Err("ModelErrorException: The system encountered an unexpected error during processing. Try your request again.".to_string())
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_is_connection_error() {
        assert!(is_connection_error("GoAway { error_code: NO_ERROR }"));
        assert!(is_connection_error("dispatch failure"));
        assert!(is_connection_error("DispatchFailure"));
        assert!(is_connection_error("connection reset by peer"));
        assert!(!is_connection_error("service error"));
        assert!(!is_connection_error("throttling: rate exceeded"));
        assert!(!is_connection_error("access denied"));
    }

    #[test]
    fn test_max_input_chars() {
        assert_eq!(NOVA_EMBED_MAX_CHARS, 50_000);
        assert_eq!(TITAN_EMBED_MAX_CHARS, 8_000);
    }
}
