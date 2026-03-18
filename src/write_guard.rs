//! Global write guard preventing concurrent destructive operations.
//!
//! On HTTP transport, axum dispatches requests concurrently — this guard
//! serialises operations that write to disk and DB (scan, check, apply,
//! organize merge/split).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::FactbaseError;

static WRITE_LOCK: AtomicBool = AtomicBool::new(false);

/// RAII guard that releases [`WRITE_LOCK`] on drop.
#[derive(Debug)]
pub(crate) struct WriteGuard;

impl WriteGuard {
    /// Try to acquire the write lock. Returns an error if another destructive
    /// operation is already in progress.
    pub fn try_acquire() -> Result<Self, FactbaseError> {
        if WRITE_LOCK
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(FactbaseError::internal(
                "Another write operation (scan/check/apply/merge/split) is already in progress. \
                 Wait for it to complete before starting another.",
            ));
        }
        Ok(Self)
    }

    /// Force-release the write lock. Only for use in tests where a prior
    /// test may have leaked the lock due to a panic or async cancellation.
    #[cfg(test)]
    pub fn force_release() {
        WRITE_LOCK.store(false, Ordering::SeqCst);
    }
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        WRITE_LOCK.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_write_guard_prevents_concurrent() {
        let guard1 = WriteGuard::try_acquire();
        assert!(guard1.is_ok());
        let guard2 = WriteGuard::try_acquire();
        assert!(guard2.is_err());
        drop(guard1);
        let guard3 = WriteGuard::try_acquire();
        assert!(guard3.is_ok());
    }
}
