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

mod export_import_tests;
mod grep_tests;
mod lint_tests;
mod misc_tests;
mod scan_tests;
mod search_tests;
