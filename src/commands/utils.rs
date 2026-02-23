//! Utility functions for CLI commands

use chrono::{DateTime, Duration, NaiveDate, Utc};
use factbase::{format_json, format_yaml, Repository};
use serde::Serialize;
use std::io::{self, Write};
use std::path::Path;

use super::OutputFormat;

/// Case-insensitive file extension check.
pub fn ends_with_ext(path: &str, ext: &str) -> bool {
    path.len() >= ext.len() && path[path.len() - ext.len()..].eq_ignore_ascii_case(ext)
}

/// Print data in the specified output format.
///
/// For JSON and YAML formats, serializes the data directly.
/// For Table format, calls the provided closure to render custom output.
///
/// # Example
/// ```ignore
/// print_output(format, &data, || {
///     println!("Custom table output");
/// })?;
/// ```
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

/// Filter repositories by optional repo ID, bailing if none remain.
///
/// If `repo_id` is `Some`, keeps only the matching repository.
/// If `repo_id` is `None`, returns all repositories.
/// Returns an error if the resulting list is empty.
pub fn resolve_repos(
    repos: Vec<Repository>,
    repo_id: Option<&str>,
) -> anyhow::Result<Vec<Repository>> {
    let filtered: Vec<_> = if let Some(id) = repo_id {
        repos.into_iter().filter(|r| r.id == id).collect()
    } else {
        repos
    };
    if filtered.is_empty() {
        anyhow::bail!("No repositories found");
    }
    Ok(filtered)
}

/// Open the database and resolve repositories in one step.
///
/// Combines the common 3-line pattern:
/// ```ignore
/// let db = setup_database_only()?;
/// let repos = db.list_repositories()?;
/// let repos = resolve_repos(repos, repo_filter)?;
/// ```
pub fn setup_db_and_resolve_repos(
    repo_filter: Option<&str>,
) -> anyhow::Result<(factbase::Database, Vec<Repository>)> {
    let db = super::setup_database_only()?;
    let repos = db.list_repositories()?;
    let repos = resolve_repos(repos, repo_filter)?;
    Ok((db, repos))
}

/// Filter items by excluded types.
///
/// Removes items whose type (extracted via `get_type`) matches any of the excluded types.
/// Items without a type are kept. Comparison is case-insensitive.
///
/// # Example
/// ```ignore
/// let results = filter_by_excluded_types(results, &exclude_types, |r| r.doc_type.as_deref());
/// ```
pub fn filter_by_excluded_types<T>(
    items: Vec<T>,
    exclude_types: &[String],
    get_type: impl Fn(&T) -> Option<&str>,
) -> Vec<T> {
    let exclude_lower: Vec<String> = exclude_types.iter().map(|t| t.to_lowercase()).collect();
    items
        .into_iter()
        .filter(|item| get_type(item).is_none_or(|t| !exclude_lower.contains(&t.to_lowercase())))
        .collect()
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
    use std::path::Path;

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

    #[test]
    fn test_filter_by_excluded_types_removes_matching() {
        struct Item {
            doc_type: Option<String>,
        }
        let items = vec![
            Item {
                doc_type: Some("draft".to_string()),
            },
            Item {
                doc_type: Some("person".to_string()),
            },
            Item {
                doc_type: Some("Draft".to_string()),
            }, // case-insensitive
        ];
        let exclude = vec!["draft".to_string()];
        let result = filter_by_excluded_types(items, &exclude, |i| i.doc_type.as_deref());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].doc_type.as_deref(), Some("person"));
    }

    #[test]
    fn test_filter_by_excluded_types_keeps_none() {
        struct Item {
            doc_type: Option<String>,
        }
        let items = vec![
            Item { doc_type: None },
            Item {
                doc_type: Some("draft".to_string()),
            },
        ];
        let exclude = vec!["draft".to_string()];
        let result = filter_by_excluded_types(items, &exclude, |i| i.doc_type.as_deref());
        assert_eq!(result.len(), 1);
        assert!(result[0].doc_type.is_none());
    }

    #[test]
    fn test_filter_by_excluded_types_empty_exclude() {
        struct Item {
            doc_type: Option<String>,
        }
        let items = vec![
            Item {
                doc_type: Some("draft".to_string()),
            },
            Item {
                doc_type: Some("person".to_string()),
            },
        ];
        let exclude: Vec<String> = vec![];
        let result = filter_by_excluded_types(items, &exclude, |i| i.doc_type.as_deref());
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_repos_no_filter() {
        let repos = vec![
            create_repository("r1", "Repo 1", Path::new("/r1")),
            create_repository("r2", "Repo 2", Path::new("/r2")),
        ];
        let result = resolve_repos(repos, None).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_resolve_repos_with_filter() {
        let repos = vec![
            create_repository("r1", "Repo 1", Path::new("/r1")),
            create_repository("r2", "Repo 2", Path::new("/r2")),
        ];
        let result = resolve_repos(repos, Some("r2")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "r2");
    }

    #[test]
    fn test_resolve_repos_empty_bails() {
        let repos: Vec<Repository> = vec![];
        let result = resolve_repos(repos, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No repositories"));
    }

    #[test]
    fn test_resolve_repos_filter_no_match_bails() {
        let repos = vec![create_repository("r1", "Repo 1", Path::new("/r1"))];
        let result = resolve_repos(repos, Some("nonexistent"));
        assert!(result.is_err());
    }
}
