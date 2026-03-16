//! Temporal tag parsing and validation.
//!
//! This module handles parsing `@t[...]` temporal tags from document content
//! and validating their format and consistency.
//!
//! # Module Organization
//!
//! - `parser` - Temporal tag parsing from content
//! - `date` - Date validation and utility functions
//! - `range` - Date range operations and overlap detection
//! - `validation` - Tag validation and conflict detection
//!
//! # Public API
//!
//! ## Functions
//! - [`parse_temporal_tags`] - Parse all temporal tags from content
//! - [`validate_date`] - Validate a single date string
//! - [`validate_temporal_tags`] - Validate all tags in content
//! - [`overlaps_point`] - Check if tag overlaps a point in time
//! - [`overlaps_range`] - Check if tag overlaps a date range
//! - [`calculate_recency_boost`] - Calculate boost for recent LastSeen tags
//!
//! ## Structs
//! - [`TemporalValidationError`] - Error in temporal tag format

mod date;
mod parser;
mod range;
mod validation;

// Re-export parser functions
pub use parser::{find_malformed_tags, parse_temporal_tags};
pub(crate) use parser::{line_has_temporal_tag, normalize_temporal_tags};

// Re-export date functions
pub use date::validate_date;

// Re-export range functions
pub(crate) use range::ranges_overlap;
pub use range::{calculate_recency_boost, overlaps_point, overlaps_range};

// Re-export validation types and functions
pub use validation::{validate_temporal_tags, TemporalValidationError};
