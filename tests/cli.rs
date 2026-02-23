//! CLI command integration tests.
//!
//! Tests CLI commands end-to-end with real operations.
//! Note: These tests use the library directly rather than spawning CLI processes.
//!
//! ## Module Organization
//!
//! Tests are organized by command/feature:
//! - `scan_tests` - Scan command tests
//! - `search_tests` - Search command tests
//! - `grep_tests` - Grep command tests
//! - `lint_tests` - Lint command tests
//! - `export_import_tests` - Export/import workflow tests
//! - `misc_tests` - Status, stats, doctor, show, links, version tests

mod common;

#[path = "cli/export_import_tests.rs"]
mod export_import_tests;
#[path = "cli/grep_tests.rs"]
mod grep_tests;
#[path = "cli/lint_tests.rs"]
mod lint_tests;
#[path = "cli/misc_tests.rs"]
mod misc_tests;
#[path = "cli/scan_tests.rs"]
mod scan_tests;
#[path = "cli/search_tests.rs"]
mod search_tests;
