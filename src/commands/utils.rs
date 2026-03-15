//! Utility functions for CLI commands

use chrono::{DateTime, Duration, NaiveDate, Utc};
use factbase::models::Repository;
use factbase::output::{format_json, format_yaml};
use serde::Serialize;
use std::io::{self, Write};
use std::path::Path;

use super::OutputFormat;

/// Print data in the specified output format.
///
/// For JSON and YAML formats, serializes the data directly.
/// For Table format, calls the provided closure to render custom output.
pub fn print_output<T: Serialize>(
    format: OutputFormat,
    data: &T,
    table_fn: impl FnOnce(),
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => println!("{}", format_json(data)?),
        OutputFormat::Yaml => println!("{}", format_yaml(data)?),
        OutputFormat::Table => table_fn(),
    }
    Ok(())
}

/// Prompt user for y/N confirmation. Returns `true` if user enters "y".
pub fn confirm_prompt(message: &str) -> anyhow::Result<bool> {
    print!("\n{message} [y/N] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Create a Repository struct with standard defaults
pub fn create_repository(id: &str, name: &str, path: &Path) -> Repository {
    Repository {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_path_buf(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    }
}

/// Parse an optional --since argument into an optional DateTime.
/// Convenience wrapper around `parse_since` for the common pattern at call sites.
pub fn parse_since_filter(since: &Option<String>) -> anyhow::Result<Option<DateTime<Utc>>> {
    since.as_ref().map(|s| parse_since(s)).transpose()
}

/// Parse --since argument into DateTime
/// Supports relative formats (1h, 1d, 1w) and ISO 8601 / date-only formats
pub fn parse_since(since: &str) -> anyhow::Result<DateTime<Utc>> {
    // Try relative format first (1h, 1d, 1w)
    if let Some(num_str) = since.strip_suffix('h') {
        let hours: i64 = num_str.parse()?;
        return Ok(Utc::now() - Duration::hours(hours));
    }
    if let Some(num_str) = since.strip_suffix('d') {
        let days: i64 = num_str.parse()?;
        return Ok(Utc::now() - Duration::days(days));
    }
    if let Some(num_str) = since.strip_suffix('w') {
        let weeks: i64 = num_str.parse()?;
        return Ok(Utc::now() - Duration::weeks(weeks));
    }

    // Try ISO 8601 format
    if let Ok(dt) = DateTime::parse_from_rfc3339(since) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try date-only format (YYYY-MM-DD)
    if let Ok(date) = NaiveDate::parse_from_str(since, "%Y-%m-%d") {
        let dt = date.and_hms_opt(0, 0, 0).expect("valid time");
        return Ok(DateTime::from_naive_utc_and_offset(dt, Utc));
    }

    anyhow::bail!("Invalid --since format. Use ISO 8601 (2024-01-01) or relative (1h, 1d, 1w)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn test_print_output_json() {
        #[derive(Serialize)]
        struct TestData {
            name: String,
        }
        let data = TestData {
            name: "test".to_string(),
        };
        let called = RefCell::new(false);
        let result = print_output(OutputFormat::Json, &data, || {
            *called.borrow_mut() = true;
        });
        assert!(result.is_ok());
        assert!(
            !*called.borrow(),
            "Table closure should not be called for JSON"
        );
    }

    #[test]
    fn test_print_output_yaml() {
        #[derive(Serialize)]
        struct TestData {
            value: i32,
        }
        let data = TestData { value: 42 };
        let called = RefCell::new(false);
        let result = print_output(OutputFormat::Yaml, &data, || {
            *called.borrow_mut() = true;
        });
        assert!(result.is_ok());
        assert!(
            !*called.borrow(),
            "Table closure should not be called for YAML"
        );
    }

    #[test]
    fn test_print_output_table() {
        #[derive(Serialize)]
        struct TestData {
            count: usize,
        }
        let data = TestData { count: 10 };
        let called = RefCell::new(false);
        let result = print_output(OutputFormat::Table, &data, || {
            *called.borrow_mut() = true;
        });
        assert!(result.is_ok());
        assert!(
            *called.borrow(),
            "Table closure should be called for Table format"
        );
    }
}
