//! Ollama test helpers for Phase 5 E2E tests.
//! These helpers REQUIRE Ollama to be running - they panic if unavailable.

#![allow(dead_code)] // Functions will be used in Phase 5 tests

use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const OLLAMA_URL: &str = "http://localhost:11434";
const EMBEDDING_MODEL: &str = "qwen3-embedding:0.6b";
const LLM_MODEL: &str = "rnj-1-extended";

/// Panics with helpful message if Ollama is not available.
/// Use at the start of tests that require Ollama.
pub async fn require_ollama() {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client");

    match client.get(format!("{}/api/tags", OLLAMA_URL)).send().await {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => panic!(
            "Ollama returned error status: {}. Start Ollama with: ollama serve",
            resp.status()
        ),
        Err(e) => panic!(
            "Ollama not available at {}: {}. Start Ollama with: ollama serve",
            OLLAMA_URL, e
        ),
    }
}

/// Panics if required models are not available.
/// Checks both embedding model (nomic-embed-text) and LLM model (rnj-1-extended).
pub async fn require_models() {
    require_ollama().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client");

    let resp = client
        .get(format!("{}/api/tags", OLLAMA_URL))
        .send()
        .await
        .expect("Failed to get model list");

    let body: Value = resp.json().await.expect("Failed to parse model list");
    let models = body["models"]
        .as_array()
        .expect("Expected models array in response");

    let model_names: Vec<&str> = models.iter().filter_map(|m| m["name"].as_str()).collect();

    let has_embedding = model_names.iter().any(|n| n.starts_with(EMBEDDING_MODEL));
    let has_llm = model_names.iter().any(|n| n.starts_with(LLM_MODEL));

    if !has_embedding {
        panic!(
            "Embedding model '{}' not found. Pull with: ollama pull {}",
            EMBEDDING_MODEL, EMBEDDING_MODEL
        );
    }

    if !has_llm {
        panic!(
            "LLM model '{}' not found. Create with:\n\
            cat > /tmp/rnj-1-extended.modelfile << 'EOF'\n\
            FROM rnj-1:latest\n\
            PARAMETER num_ctx 49152\n\
            EOF\n\
            ollama create rnj-1-extended -f /tmp/rnj-1-extended.modelfile",
            LLM_MODEL
        );
    }
}

/// Waits for Ollama to become available, with retries.
/// Useful for CI environments where Ollama may take time to start.
pub async fn wait_for_ollama(max_attempts: u32, delay_secs: u64) {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client");

    for attempt in 1..=max_attempts {
        match client.get(format!("{}/api/tags", OLLAMA_URL)).send().await {
            Ok(resp) if resp.status().is_success() => return,
            _ => {
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                }
            }
        }
    }

    panic!(
        "Ollama not available after {} attempts. Start with: ollama serve",
        max_attempts
    );
}

/// Generates a test embedding to verify Ollama is working.
/// Returns the embedding vector.
pub async fn get_test_embedding(text: &str) -> Vec<f32> {
    require_ollama().await;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    let resp = client
        .post(format!("{}/api/embeddings", OLLAMA_URL))
        .json(&serde_json::json!({
            "model": EMBEDDING_MODEL,
            "prompt": text
        }))
        .send()
        .await
        .expect("Failed to generate embedding");

    let body: Value = resp
        .json()
        .await
        .expect("Failed to parse embedding response");
    body["embedding"]
        .as_array()
        .expect("Expected embedding array")
        .iter()
        .map(|v| v.as_f64().expect("Expected float") as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Ollama
    async fn test_require_ollama() {
        require_ollama().await;
    }

    #[tokio::test]
    #[ignore] // Requires Ollama with models
    async fn test_require_models() {
        require_models().await;
    }

    #[tokio::test]
    #[ignore] // Requires Ollama
    async fn test_get_embedding() {
        let embedding = get_test_embedding("test text").await;
        assert_eq!(embedding.len(), 1024);
    }
}
