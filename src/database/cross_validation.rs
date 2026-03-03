//! Cross-validation progress state stored server-side.
//!
//! Replaces the client-side `checked_pair_ids` cursor that grew too large
//! for MCP responses (~220KB for medium repos). The server stores a simple
//! integer offset into the deterministically-sorted pair list, plus a
//! `fact_count` to detect when a rescan invalidates the cursor.

use super::Database;
use crate::error::FactbaseError;

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
                Ok((row.get::<_, i64>(0)? as usize, row.get::<_, i64>(1)? as usize))
            })
            .optional()?;
        Ok(result)
    }

    /// Save cross-validation progress.
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
             ON CONFLICT(scope_key) DO UPDATE SET pair_offset = ?2, fact_count = ?3, updated_at = datetime('now')",
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
}
