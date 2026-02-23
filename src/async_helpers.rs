//! Shared async utility helpers.

use crate::error::FactbaseError;
use tokio::task::spawn_blocking;

/// Run a blocking closure on a dedicated thread via `spawn_blocking`.
///
/// Converts the `JoinError` from tokio into `FactbaseError::Internal`.
/// Used by both MCP and Web modules to safely run synchronous database
/// operations from async handlers.
pub(crate) async fn run_blocking<F, T>(f: F) -> Result<T, FactbaseError>
where
    F: FnOnce() -> Result<T, FactbaseError> + Send + 'static,
    T: Send + 'static,
{
    spawn_blocking(f)
        .await
        .map_err(|e| FactbaseError::internal(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_blocking_success() {
        let result = run_blocking(|| Ok::<_, FactbaseError>(42)).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_run_blocking_error() {
        let result = run_blocking(|| Err::<i32, _>(FactbaseError::internal("fail"))).await;
        assert!(result.unwrap_err().to_string().contains("fail"));
    }
}
