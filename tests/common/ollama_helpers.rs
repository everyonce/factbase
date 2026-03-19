//! Ollama test helpers for integration tests.
//! These helpers REQUIRE Ollama to be running - they panic if unavailable.

#![allow(dead_code)]

use reqwest::Client;
use std::time::Duration;

const OLLAMA_URL: &str = "http://localhost:11434";

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
