//! Configuration validation utilities.

use crate::error::FactbaseError;
use anyhow::bail;
use std::fmt::Display;

/// Valid timeout range for Ollama operations (1-300 seconds)
pub const TIMEOUT_RANGE: std::ops::RangeInclusive<u64> = 1..=300;

/// Validate that a string value is not empty.
pub(crate) fn require_non_empty(value: &str, field: &str) -> Result<(), FactbaseError> {
    if value.is_empty() {
        return Err(FactbaseError::config(format!("{field} must not be empty")));
    }
    Ok(())
}

/// Validate that a numeric value is greater than zero.
pub(crate) fn require_positive(value: u64, field: &str) -> Result<(), FactbaseError> {
    if value == 0 {
        return Err(FactbaseError::config(format!(
            "{field} must be greater than 0"
        )));
    }
    Ok(())
}

/// Validate that a value falls within an inclusive range.
pub(crate) fn require_range<T: PartialOrd + Display>(
    value: T,
    min: T,
    max: T,
    field: &str,
) -> Result<(), FactbaseError> {
    if value < min || value > max {
        return Err(FactbaseError::config(format!(
            "{field} must be between {min} and {max}"
        )));
    }
    Ok(())
}

/// Validate that a timeout value is within the allowed range (1-300 seconds).
pub fn validate_timeout(timeout: u64) -> anyhow::Result<()> {
    if TIMEOUT_RANGE.contains(&timeout) {
        Ok(())
    } else {
        bail!(
            "--timeout must be between {} and {} seconds",
            TIMEOUT_RANGE.start(),
            TIMEOUT_RANGE.end()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_timeout() {
        assert!(validate_timeout(1).is_ok());
        assert!(validate_timeout(150).is_ok());
        assert!(validate_timeout(300).is_ok());
        let err = validate_timeout(0).unwrap_err().to_string();
        assert!(err.contains("1") && err.contains("300"));
        let err2 = validate_timeout(301).unwrap_err().to_string();
        assert!(err2.contains("1") && err2.contains("300"));
    }

    #[test]
    fn test_require_non_empty() {
        assert!(require_non_empty("hello", "field").is_ok());
        assert!(require_non_empty("", "my.field")
            .unwrap_err()
            .to_string()
            .contains("must not be empty"));
    }

    #[test]
    fn test_require_positive() {
        assert!(require_positive(1, "field").is_ok());
        assert!(require_positive(0, "my.field")
            .unwrap_err()
            .to_string()
            .contains("must be greater than 0"));
    }

    #[test]
    fn test_require_range() {
        assert!(require_range(1, 1, 10, "field").is_ok()); // at min
        assert!(require_range(5, 1, 10, "field").is_ok()); // middle
        assert!(require_range(10, 1, 10, "field").is_ok()); // at max
        assert!(require_range(0, 1, 10, "my.field")
            .unwrap_err()
            .to_string()
            .contains("must be between 1 and 10"));
        assert!(require_range(11, 1, 10, "my.field")
            .unwrap_err()
            .to_string()
            .contains("must be between 1 and 10"));
        // f64
        assert!(require_range(0.5, 0.0, 1.0, "field").is_ok());
        assert!(require_range(-0.1, 0.0, 1.0, "field").is_err());
    }
}
