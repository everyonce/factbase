//! Health check functions for doctor command.

use factbase::Config;
use reqwest::Client;
use serde::Serialize;
use std::path::Path;

/// Status of a single health check.
#[derive(Serialize)]
pub struct CheckStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CheckStatus {
    pub fn ok() -> Self {
        Self {
            available: true,
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            available: false,
            error: Some(msg.into()),
        }
    }
}

/// Combined output of all health checks.
#[derive(Serialize)]
pub struct DoctorOutput {
    pub database: CheckStatus,
    pub ollama_server: CheckStatus,
    pub embedding_model: CheckStatus,
    pub overall_healthy: bool,
}

/// Check database connectivity and return status.
pub fn check_database(config: &Config) -> (bool, CheckStatus, String) {
    let db_path = shellexpand::tilde(&config.database.path).to_string();

    match config.open_database(Path::new(&db_path)) {
        Ok(db) => match db.health_check() {
            Ok(()) => {
                let repos = db.list_repositories().unwrap_or_default();
                (
                    true,
                    CheckStatus::ok(),
                    format!("{} ({} repos)", db_path, repos.len()),
                )
            }
            Err(e) => (
                false,
                CheckStatus::err(format!("health check failed: {e}")),
                db_path,
            ),
        },
        Err(e) => (
            false,
            CheckStatus::err(format!("cannot open: {e}")),
            db_path,
        ),
    }
}

/// Check Ollama server connectivity.
pub async fn check_ollama_server(client: &Client, base_url: &str) -> (bool, CheckStatus) {
    match client.get(base_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                (true, CheckStatus::ok())
            } else {
                (false, CheckStatus::err(format!("status {}", resp.status())))
            }
        }
        Err(_) => (false, CheckStatus::err("not reachable")),
    }
}

/// Fetch list of available models from Ollama.
pub async fn fetch_available_models(client: &Client, base_url: &str) -> Vec<String> {
    let tags_url = format!("{base_url}/api/tags");
    match client.get(&tags_url).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                json["models"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                m["name"].as_str().map(std::string::ToString::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            }
        }
        Err(_) => vec![],
    }
}

/// Check if a model is available in the list.
pub fn model_available(models: &[String], model_name: &str) -> bool {
    models.iter().any(|m| m.starts_with(model_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_status_ok() {
        let status = CheckStatus::ok();
        assert!(status.available);
        assert!(status.error.is_none());
    }

    #[test]
    fn test_check_status_err() {
        let status = CheckStatus::err("test error");
        assert!(!status.available);
        assert_eq!(status.error, Some("test error".to_string()));
    }

    #[test]
    fn test_doctor_output_serialization() {
        let output = DoctorOutput {
            database: CheckStatus::ok(),
            ollama_server: CheckStatus::ok(),
            embedding_model: CheckStatus::ok(),
            overall_healthy: true,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"overall_healthy\":true"));
        assert!(json.contains("\"database\""));
        assert!(json.contains("\"ollama_server\""));
        assert!(json.contains("\"embedding_model\""));
        // error field should be omitted when None
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_doctor_output_with_errors() {
        let output = DoctorOutput {
            database: CheckStatus::err("cannot open"),
            ollama_server: CheckStatus::err("not reachable"),
            embedding_model: CheckStatus::err("not found"),
            overall_healthy: false,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"overall_healthy\":false"));
        assert!(json.contains("\"error\":\"cannot open\""));
        assert!(json.contains("\"error\":\"not reachable\""));
        assert!(json.contains("\"error\":\"not found\""));
    }

    #[test]
    fn test_model_available() {
        let models = vec![
            "qwen3-embedding:0.6b".to_string(),
            "rnj-1-extended".to_string(),
        ];
        assert!(model_available(&models, "qwen3-embedding"));
        assert!(model_available(&models, "rnj-1-extended"));
        assert!(!model_available(&models, "nonexistent"));
    }

    #[test]
    fn test_model_available_empty() {
        let models: Vec<String> = vec![];
        assert!(!model_available(&models, "any-model"));
    }
}
