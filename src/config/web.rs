//! Web UI configuration.

use serde::{Deserialize, Serialize};

/// Web UI server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_enabled() -> bool {
    false
}

fn default_port() -> u16 {
    3001
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            port: default_port(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_config_defaults() {
        let config = WebConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.port, 3001);
    }

    #[test]
    fn test_web_config_serde_roundtrip() {
        let config = WebConfig {
            enabled: true,
            port: 8080,
        };
        let yaml = serde_yaml_ng::to_string(&config).unwrap();
        let parsed: WebConfig = serde_yaml_ng::from_str(&yaml).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.port, 8080);
    }

    #[test]
    fn test_web_config_deserialize_empty() {
        let yaml = "{}";
        let config: WebConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.port, 3001);
    }

    #[test]
    fn test_web_config_deserialize_partial() {
        let yaml = "enabled: true";
        let config: WebConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.port, 3001);
    }
}
