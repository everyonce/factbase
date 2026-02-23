//! Server, rate limiting, temporal, and review configuration.

use serde::{Deserialize, Serialize};

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_per_second")]
    pub per_second: u64,
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
}

fn default_per_second() -> u64 {
    10
}

fn default_burst_size() -> u32 {
    20
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: default_per_second(),
            burst_size: default_burst_size(),
        }
    }
}

/// Temporal tag configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalConfig {
    #[serde(default = "default_min_coverage")]
    pub min_coverage: f32,
    #[serde(default = "default_recency_window_days")]
    pub recency_window_days: u32,
    #[serde(default = "default_recency_boost_factor")]
    pub recency_boost_factor: f32,
}

fn default_min_coverage() -> f32 {
    0.8 // 80% coverage threshold
}

fn default_recency_window_days() -> u32 {
    180 // 6 months
}

fn default_recency_boost_factor() -> f32 {
    0.2 // 20% max boost for most recent facts
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            min_coverage: default_min_coverage(),
            recency_window_days: default_recency_window_days(),
            recency_boost_factor: default_recency_boost_factor(),
        }
    }
}

/// Review system configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewConfig {
    #[serde(default)]
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig::default();
        assert_eq!(config.per_second, 10);
        assert_eq!(config.burst_size, 20);
    }

    #[test]
    fn test_temporal_config_defaults() {
        let config = TemporalConfig::default();
        assert!((config.min_coverage - 0.8).abs() < f32::EPSILON);
        assert_eq!(config.recency_window_days, 180);
        assert!((config.recency_boost_factor - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_review_config_defaults() {
        let config = ReviewConfig::default();
        assert!(config.model.is_none());
    }
}
