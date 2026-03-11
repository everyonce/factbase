//! Auto-fix logic for doctor command.

use anyhow::{bail, Context};
use factbase::config::Config;
use std::fs;
use std::process::Command;

/// Pull an Ollama model using the ollama CLI.
pub fn pull_ollama_model(model: &str) -> anyhow::Result<()> {
    let status = Command::new("ollama").args(["pull", model]).status()?;

    if status.success() {
        Ok(())
    } else {
        bail!("ollama pull exited with status: {status}")
    }
}

/// Create default config file if it doesn't exist.
pub fn create_default_config() -> anyhow::Result<Config> {
    let config_path = std::path::PathBuf::from(".factbase/config.yaml");
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let default_config = Config::default();
    let yaml = serde_yaml_ng::to_string(&default_config)?;
    fs::write(&config_path, yaml)?;
    Config::load(None).with_context(|| "Failed to load created config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pull_ollama_model_command_format() {
        // We can't actually test pulling without Ollama running,
        // but we can verify the function signature and error handling
        let result = pull_ollama_model("nonexistent-model-12345");
        // Should fail because model doesn't exist or ollama isn't running
        assert!(result.is_err());
    }
}
