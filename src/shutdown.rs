//! Graceful shutdown handling for long-running operations.
//!
//! This module provides a shared shutdown flag that can be checked by long-running
//! operations to exit gracefully when the user presses Ctrl+C.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Global shutdown flag
static SHUTDOWN_FLAG: OnceLock<AtomicBool> = OnceLock::new();

/// Initialize the shutdown handler.
/// Should be called once at program startup from within a tokio runtime.
pub fn init_shutdown_handler() {
    // Initialize the flag
    SHUTDOWN_FLAG.get_or_init(|| AtomicBool::new(false));

    // Spawn a background task that waits for Ctrl+C
    tokio::spawn(async {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            if let Some(flag) = SHUTDOWN_FLAG.get() {
                flag.store(true, Ordering::SeqCst);
            }
            eprintln!(
                "\n{}",
                crate::error::format_warning("Interrupted. Saving progress...")
            );
        }
    });
}

/// Check if shutdown has been requested.
/// Returns true if Ctrl+C was pressed.
pub(crate) fn is_shutdown_requested() -> bool {
    SHUTDOWN_FLAG
        .get()
        .is_some_and(|flag| flag.load(Ordering::SeqCst))
}

/// Request shutdown programmatically.
/// Useful for testing.
#[cfg(test)]
pub(crate) fn request_shutdown() {
    if let Some(flag) = SHUTDOWN_FLAG.get() {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Reset the shutdown flag.
/// Useful for testing.
#[cfg(test)]
pub fn reset_shutdown_flag() {
    if let Some(flag) = SHUTDOWN_FLAG.get() {
        flag.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_flag_default() {
        // Initialize if not already done
        let _ = SHUTDOWN_FLAG.get_or_init(|| AtomicBool::new(false));
        reset_shutdown_flag();
        assert!(!is_shutdown_requested());
    }

    #[test]
    fn test_request_shutdown() {
        // Initialize if not already done
        let _ = SHUTDOWN_FLAG.get_or_init(|| AtomicBool::new(false));
        reset_shutdown_flag();
        assert!(!is_shutdown_requested());
        request_shutdown();
        assert!(is_shutdown_requested());
        reset_shutdown_flag();
    }
}
