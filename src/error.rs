use thiserror::Error;

#[derive(Error, Debug)]
pub enum FactbaseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Watcher error: {0}")]
    Watcher(String),
}

impl From<serde_yaml::Error> for FactbaseError {
    fn from(e: serde_yaml::Error) -> Self {
        FactbaseError::Config(e.to_string())
    }
}
