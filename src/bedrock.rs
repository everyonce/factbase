//! Amazon Bedrock provider implementations for embedding and LLM.

use crate::error::FactbaseError;
use crate::EmbeddingProvider;
use crate::LlmProvider;
use crate::BoxFuture;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message};
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use tracing::debug;

/// Build a Bedrock runtime client with optional region override.
async fn build_client(region: Option<&str>) -> Client {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = region {
        loader = loader.region(aws_config::Region::new(r.to_string()));
    }
    Client::new(&loader.load().await)
}

fn embed_err(ctx: &str, e: impl Display) -> FactbaseError {
    FactbaseError::embedding(format!("{ctx}: {e}"))
}

fn llm_err(ctx: &str, e: impl Display) -> FactbaseError {
    FactbaseError::llm(format!("{ctx}: {e}"))
}

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
    pub async fn new(model_id: &str, dimension: usize, region: Option<&str>) -> Self {
        Self {
            client: build_client(region).await,
            model_id: model_id.to_string(),
            dim: dimension,
        }
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
        Box::pin(async move { self.invoke_embed(text).await })
    }

    fn generate_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, Result<Vec<Vec<f32>>, FactbaseError>> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(texts.len());
            for text in texts {
                results.push(self.invoke_embed(text).await?);
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
    client: Client,
    model_id: String,
}

impl BedrockLlm {
    pub async fn new(model_id: &str, region: Option<&str>) -> Self {
        Self {
            client: build_client(region).await,
            model_id: model_id.to_string(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model_id
    }
}

impl LlmProvider for BedrockLlm {
    fn complete<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String, FactbaseError>> {
        Box::pin(async move {
            debug!("Bedrock converse: model={}", self.model_id);

            let msg = Message::builder()
                .role(ConversationRole::User)
                .content(ContentBlock::Text(prompt.to_string()))
                .build()
                .map_err(|e| llm_err("Build message", e))?;

            let resp = self
                .client
                .converse()
                .model_id(&self.model_id)
                .messages(msg)
                .send()
                .await
                .map_err(|e| llm_err("Bedrock converse", format!("{e:#}")))?;

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
}
