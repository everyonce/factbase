//! Document processing module.
//!
//! This module handles all document processing operations including:
//! - Core document identity (ID extraction/injection, title, type, hash)
//! - Temporal tag parsing and validation
//! - Source reference parsing
//! - Review question parsing
//! - Document chunking for embeddings
//! - Fact statistics calculation
//!
//! # Module Organization
//!
//! - `core` - Document identity: ID extraction/injection, title, type, hash
//! - `temporal` - Temporal tag parsing (`@t[...]`) and validation
//! - `sources` - Source reference parsing (footnotes)
//! - `review` - Review question parsing (`@q[...]`)
//! - `chunks` - Document chunking for large documents
//! - `stats` - Fact statistics calculation
//!
//! # Public API (22 items)
//!
//! ## Structs
//! - [`DocumentProcessor`] - Main processor for document operations
//! - [`DocumentChunk`] - Chunk of a large document for embedding
//! - [`TemporalValidationError`] - Error in temporal tag format
//! - [`TemporalSequenceError`] - Illogical date sequence
//! - [`TemporalConflict`] - Conflicting temporal ranges
//!
//! ## Functions
//! - Core: [`DocumentProcessor::new`], [`DocumentProcessor::compute_hash`], etc.
//! - Temporal: [`parse_temporal_tags`], [`validate_temporal_tags`], [`detect_temporal_conflicts`]
//! - Sources: [`parse_source_references`], [`parse_source_definitions`]
//! - Review: [`parse_review_queue`], [`append_review_questions`]
//! - Chunks: [`chunk_document`]
//! - Stats: [`calculate_fact_stats`], [`count_facts`], [`count_facts_with_temporal_tags`]

// Submodules
mod chunks;
mod core;
mod review;
mod sources;
mod stats;
mod temporal;

// Re-export core types and functions
pub(crate) use core::normalize_type;
pub use core::DocumentProcessor;

// Re-export temporal types and functions
pub use temporal::{
    calculate_recency_boost, detect_illogical_sequences, detect_temporal_conflicts, overlaps_point,
    overlaps_range, parse_temporal_tags, validate_date, validate_temporal_tags, TemporalConflict,
    TemporalSequenceError, TemporalValidationError,
};

// Re-export source types and functions
pub use sources::{count_facts_with_sources, parse_source_definitions, parse_source_references};

// Re-export review types and functions
pub use review::{append_review_questions, parse_review_queue};

// Re-export chunking types and functions
pub use chunks::{chunk_document, DocumentChunk};

// Re-export stats types and functions
pub use stats::{calculate_fact_stats, count_facts, count_facts_with_temporal_tags};

// Note: Unit tests are distributed to their respective submodules:
// - core.rs: 16 tests (ID, title, type, hash)
// - temporal.rs: 24 tests (temporal tag parsing)
// - sources.rs: 31 tests (source reference parsing)
// - review.rs: 17 tests (review queue parsing)
// - chunks.rs: 5 tests (document chunking)
// - stats.rs: 15 tests (fact statistics)
