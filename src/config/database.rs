//! Database configuration.

use serde::{Deserialize, Serialize};

/// Database configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "default_compression")]
    pub compression: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            pool_size: default_pool_size(),
            compression: default_compression(),
        }
    }
}

impl DatabaseConfig {
    /// Returns true if zstd compression is enabled for document content.
    pub fn is_compression_enabled(&self) -> bool {
        self.compression == "zstd"
    }
}

fn default_db_path() -> String {
    ".factbase/factbase.db".to_string()
}

pub(crate) fn default_compression() -> String {
    "none".to_string()
}

pub(crate) fn default_pool_size() -> u32 {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pool_size() {
        assert_eq!(default_pool_size(), 4);
    }

    #[test]
    fn test_default_compression() {
        assert_eq!(default_compression(), "none");
    }
}
