//! Web API endpoints.
//!
//! JSON API wrapping existing CLI/MCP functions.
//! All business logic lives in existing modules - these are thin wrappers.

pub mod documents;
pub mod errors;
pub mod organize;
pub mod review;
pub mod stats;

pub use documents::*;
pub use errors::{handle_error, ApiError};
pub use organize::*;
pub use review::*;
pub use stats::*;

use crate::error::FactbaseError;
use axum::{http::StatusCode, Json};

/// Run blocking operation in spawn_blocking context with consistent error handling.
///
/// Delegates to the shared [`crate::async_helpers::run_blocking`] and maps
/// `FactbaseError` to the web API error response format.
pub(crate) async fn run_blocking_web<F, T>(f: F) -> Result<T, (StatusCode, Json<ApiError>)>
where
    F: FnOnce() -> Result<T, FactbaseError> + Send + 'static,
    T: Send + 'static,
{
    crate::async_helpers::run_blocking(f)
        .await
        .map_err(errors::handle_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_blocking_web_success() {
        let result = run_blocking_web(|| Ok::<_, FactbaseError>(42)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_run_blocking_web_error() {
        let result = run_blocking_web(|| Err::<i32, _>(FactbaseError::not_found("test"))).await;
        assert!(result.is_err());
        let (status, json) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json.code, "NOT_FOUND");
    }

    #[tokio::test]
    async fn test_run_blocking_web_internal_error() {
        let result = run_blocking_web(|| Err::<i32, _>(FactbaseError::internal("internal"))).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
