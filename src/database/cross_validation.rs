//! Cross-validation progress state stored server-side.
//!
//! Replaces the client-side `checked_pair_ids` cursor that grew too large
//! for MCP responses (~220KB for medium repos). The server stores a simple
//! integer offset into the deterministically-sorted pair list, plus a
//! `fact_count` to detect when a rescan invalidates the cursor.
//!
//! Includes a lock/lease mechanism to prevent concurrent cross-validation
//! on the same scope from clobbering each other's progress.

use super::Database;
use crate::error::FactbaseError;

/// Default lock timeout in seconds (10 minutes).
pub const DEFAULT_LOCK_TIMEOUT_SECS: u64 = 600;

/// Result of attempting to acquire a cross-validation lock.
#[derive(Debug, PartialEq)]
pub enum CvLockResult {
    /// Lock acquired (or re-acquired by same token).
    Acquired,
    /// Lock held by another session.
    AlreadyLocked {
        locked_by: String,
        pair_offset: usize,
        fact_count: usize,
    },
}

impl Database {
    /// Get the stored cross-validation offset for a scope key.
    /// Returns `Some((offset, fact_count))` if state exists, `None` otherwise.
    pub fn get_cross_validation_state(
        &self,
        scope_key: &str,
    ) -> Result<Option<(usize, usize)>, FactbaseError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT pair_offset, fact_count FROM cross_validation_state WHERE scope_key = ?1",
        )?;
        let result = stmt
            .query_row([scope_key], |row| {
                Ok((
                    row.get::<_, i64>(0)? as usize,
                    row.get::<_, i64>(1)? as usize,
                ))
            })
            .optional()?;
        Ok(result)
    }

    /// Save cross-validation progress (also extends lease if locked).
    pub fn set_cross_validation_state(
        &self,
        scope_key: &str,
        pair_offset: usize,
        fact_count: usize,
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO cross_validation_state (scope_key, pair_offset, fact_count, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(scope_key) DO UPDATE SET pair_offset = ?2, fact_count = ?3, updated_at = datetime('now'),
             locked_at = CASE WHEN locked_by IS NOT NULL THEN datetime('now') ELSE locked_at END",
            rusqlite::params![scope_key, pair_offset as i64, fact_count as i64],
        )?;
        Ok(())
    }

    /// Clear cross-validation state for a scope (or all scopes).
    pub fn clear_cross_validation_state(
        &self,
        scope_key: Option<&str>,
    ) -> Result<(), FactbaseError> {
        let conn = self.get_conn()?;
        if let Some(key) = scope_key {
            conn.execute(
                "DELETE FROM cross_validation_state WHERE scope_key = ?1",
                [key],
            )?;
        } else {
            conn.execute("DELETE FROM cross_validation_state", [])?;
        }
        Ok(())
    }

    /// Try to acquire a lock for cross-validation on a scope.
    ///
    /// - If no lock exists or the lock is expired, acquires it.
    /// - If the same token already holds the lock, re-acquires (idempotent).
    /// - If another token holds a non-expired lock, returns `AlreadyLocked`.
    pub fn try_acquire_cv_lock(
        &self,
        scope_key: &str,
        token: &str,
        lock_timeout_secs: u64,
    ) -> Result<CvLockResult, FactbaseError> {
        let conn = self.get_conn()?;

        // Check current state
        let row: Option<(Option<String>, Option<String>, i64, i64)> = conn
            .prepare_cached(
                "SELECT locked_by, locked_at, pair_offset, fact_count
                 FROM cross_validation_state WHERE scope_key = ?1",
            )?
            .query_row([scope_key], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?;

        if let Some((Some(holder), Some(locked_at), offset, fact_count)) = &row {
            // Lock exists — check if it's ours or expired
            if holder != token {
                let expired = conn
                    .prepare_cached(
                        "SELECT datetime(?1, '+' || ?2 || ' seconds') < datetime('now')",
                    )?
                    .query_row(rusqlite::params![locked_at, lock_timeout_secs], |r| {
                        r.get::<_, bool>(0)
                    })?;
                if !expired {
                    return Ok(CvLockResult::AlreadyLocked {
                        locked_by: holder.clone(),
                        pair_offset: *offset as usize,
                        fact_count: *fact_count as usize,
                    });
                }
            }
        }

        // Acquire: upsert with our token
        conn.execute(
            "INSERT INTO cross_validation_state (scope_key, pair_offset, fact_count, updated_at, locked_by, locked_at)
             VALUES (?1, 0, 0, datetime('now'), ?2, datetime('now'))
             ON CONFLICT(scope_key) DO UPDATE SET locked_by = ?2, locked_at = datetime('now'), updated_at = datetime('now')",
            rusqlite::params![scope_key, token],
        )?;

        Ok(CvLockResult::Acquired)
    }

    /// Extend the lease on a held lock (refresh locked_at).
    pub fn extend_cv_lease(&self, scope_key: &str, token: &str) -> Result<bool, FactbaseError> {
        let conn = self.get_conn()?;
        let updated = conn.execute(
            "UPDATE cross_validation_state SET locked_at = datetime('now'), updated_at = datetime('now')
             WHERE scope_key = ?1 AND locked_by = ?2",
            rusqlite::params![scope_key, token],
        )?;
        Ok(updated > 0)
    }

    /// Release the lock (clear lock fields and delete state).
    pub fn release_cv_lock(&self, scope_key: &str, token: &str) -> Result<bool, FactbaseError> {
        let conn = self.get_conn()?;
        let deleted = conn.execute(
            "DELETE FROM cross_validation_state WHERE scope_key = ?1 AND locked_by = ?2",
            rusqlite::params![scope_key, token],
        )?;
        Ok(deleted > 0)
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::tests::test_db;

    #[test]
    fn test_cross_validation_state_roundtrip() {
        let (db, _tmp) = test_db();
        // Initially empty
        assert_eq!(db.get_cross_validation_state("test-repo").unwrap(), None);

        // Set state
        db.set_cross_validation_state("test-repo", 42, 100).unwrap();
        assert_eq!(
            db.get_cross_validation_state("test-repo").unwrap(),
            Some((42, 100))
        );

        // Update state
        db.set_cross_validation_state("test-repo", 80, 100).unwrap();
        assert_eq!(
            db.get_cross_validation_state("test-repo").unwrap(),
            Some((80, 100))
        );

        // Clear specific
        db.clear_cross_validation_state(Some("test-repo")).unwrap();
        assert_eq!(db.get_cross_validation_state("test-repo").unwrap(), None);
    }

    #[test]
    fn test_cross_validation_state_multiple_scopes() {
        let (db, _tmp) = test_db();
        db.set_cross_validation_state("repo-a", 10, 50).unwrap();
        db.set_cross_validation_state("repo-b", 20, 60).unwrap();

        assert_eq!(
            db.get_cross_validation_state("repo-a").unwrap(),
            Some((10, 50))
        );
        assert_eq!(
            db.get_cross_validation_state("repo-b").unwrap(),
            Some((20, 60))
        );

        // Clear all
        db.clear_cross_validation_state(None).unwrap();
        assert_eq!(db.get_cross_validation_state("repo-a").unwrap(), None);
        assert_eq!(db.get_cross_validation_state("repo-b").unwrap(), None);
    }

    #[test]
    fn test_lock_acquire_and_release() {
        let (db, _tmp) = test_db();
        let result = db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();
        assert_eq!(result, CvLockResult::Acquired);

        // Same token re-acquires
        let result = db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();
        assert_eq!(result, CvLockResult::Acquired);

        // Release
        assert!(db.release_cv_lock("repo-a", "token-1").unwrap());
    }

    #[test]
    fn test_lock_contention() {
        let (db, _tmp) = test_db();
        // Token 1 acquires
        let r = db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();
        assert_eq!(r, CvLockResult::Acquired);

        // Set some progress so we can verify it's returned
        db.set_cross_validation_state("repo-a", 42, 100).unwrap();

        // Token 2 is blocked
        let r = db.try_acquire_cv_lock("repo-a", "token-2", 600).unwrap();
        match r {
            CvLockResult::AlreadyLocked {
                locked_by,
                pair_offset,
                fact_count,
            } => {
                assert_eq!(locked_by, "token-1");
                assert_eq!(pair_offset, 42);
                assert_eq!(fact_count, 100);
            }
            _ => panic!("expected AlreadyLocked"),
        }
    }

    #[test]
    fn test_lock_expiry() {
        let (db, _tmp) = test_db();
        // Acquire lock
        db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();

        // Manually backdate locked_at to simulate expiry
        let conn = db.get_conn().unwrap();
        conn.execute(
            "UPDATE cross_validation_state SET locked_at = datetime('now', '-700 seconds') WHERE scope_key = 'repo-a'",
            [],
        ).unwrap();

        // Token 2 can now acquire (lock expired)
        let r = db.try_acquire_cv_lock("repo-a", "token-2", 600).unwrap();
        assert_eq!(r, CvLockResult::Acquired);
    }

    #[test]
    fn test_extend_lease() {
        let (db, _tmp) = test_db();
        db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();

        // Extend succeeds for correct token
        assert!(db.extend_cv_lease("repo-a", "token-1").unwrap());

        // Extend fails for wrong token
        assert!(!db.extend_cv_lease("repo-a", "token-2").unwrap());
    }

    #[test]
    fn test_release_wrong_token() {
        let (db, _tmp) = test_db();
        db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();

        // Wrong token can't release
        assert!(!db.release_cv_lock("repo-a", "token-2").unwrap());

        // Correct token can release
        assert!(db.release_cv_lock("repo-a", "token-1").unwrap());
    }

    #[test]
    fn test_set_state_extends_lease() {
        let (db, _tmp) = test_db();
        db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();

        // Backdate the lock
        let conn = db.get_conn().unwrap();
        conn.execute(
            "UPDATE cross_validation_state SET locked_at = datetime('now', '-500 seconds') WHERE scope_key = 'repo-a'",
            [],
        ).unwrap();

        // set_cross_validation_state should refresh locked_at
        db.set_cross_validation_state("repo-a", 50, 100).unwrap();

        // Lock should not be expired now (was refreshed)
        let r = db.try_acquire_cv_lock("repo-a", "token-2", 600).unwrap();
        match r {
            CvLockResult::AlreadyLocked { .. } => {} // expected
            _ => panic!("lock should still be held after set_state refreshed it"),
        }
    }

    #[test]
    fn test_lock_different_scopes_independent() {
        let (db, _tmp) = test_db();
        let r1 = db.try_acquire_cv_lock("repo-a", "token-1", 600).unwrap();
        assert_eq!(r1, CvLockResult::Acquired);

        // Different scope — independent lock
        let r2 = db.try_acquire_cv_lock("repo-b", "token-2", 600).unwrap();
        assert_eq!(r2, CvLockResult::Acquired);
    }
}
