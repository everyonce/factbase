//! Cross-validation configuration.

use serde::{Deserialize, Serialize};

/// Configuration for cross-document fact validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossValidateConfig {
    /// Minimum cosine similarity for fact pairs (0.0 to 1.0).
    #[serde(default = "default_fact_similarity_threshold")]
    pub fact_similarity_threshold: f32,
}

impl Default for CrossValidateConfig {
    fn default() -> Self {
        Self {
            fact_similarity_threshold: default_fact_similarity_threshold(),
        }
    }
}

pub(crate) fn default_fact_similarity_threshold() -> f32 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cross_validate_config() {
        let config = CrossValidateConfig::default();
        assert!((config.fact_similarity_threshold - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_deserialize_cross_validate_config() {
        let yaml = "fact_similarity_threshold: 0.7";
        let config: CrossValidateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!((config.fact_similarity_threshold - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_deserialize_empty_uses_defaults() {
        let yaml = "{}";
        let config: CrossValidateConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!((config.fact_similarity_threshold - 0.5).abs() < f32::EPSILON);
    }
}
