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
//! - `review` - Review question parsing (`@q[...]`), normalization, and management
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
//! - Core: [`DocumentProcessor::new`], [`content_hash`], etc.
//! - Temporal: [`parse_temporal_tags`], [`validate_temporal_tags`], [`detect_temporal_conflicts`]
//! - Temporal: [`line_has_temporal_tag`], [`normalize_temporal_tags`]
//! - Sources: [`parse_source_references`], [`parse_source_definitions`]
//! - Review: [`parse_review_queue`], [`append_review_questions`]
//! - Chunks: [`chunk_document`]
//! - Stats: [`calculate_fact_stats`], [`count_facts`], [`count_facts_with_temporal_tags`]

// Submodules
mod acronyms;
mod chunks;
mod citations;
mod core;
pub mod format;
mod links;
pub mod repair;
mod review;
mod sources;
mod stats;
mod temporal;

// Re-export core types and functions
pub(crate) use core::normalize_type;
pub use core::{content_hash, DocumentProcessor};

// Re-export acronym deduplication
pub use acronyms::{dedup_acronym_expansions, strip_glossary_reviewed_markers};

// Re-export temporal types and functions
pub(crate) use temporal::ranges_overlap;
pub use temporal::{
    calculate_recency_boost, detect_illogical_sequences, detect_temporal_conflicts,
    find_malformed_tags, overlaps_point, overlaps_range, parse_temporal_tags, validate_date,
    validate_temporal_tags, TemporalConflict, TemporalSequenceError, TemporalValidationError,
};
pub(crate) use temporal::{line_has_temporal_tag, normalize_temporal_tags};

// Re-export source types and functions
pub use sources::{
    count_facts_with_sources, extract_source_date, parse_source_definitions,
    parse_source_references,
};

// Re-export review types and functions
pub use review::{
    append_review_questions, ensure_review_section, is_callout_review,
    merge_duplicate_review_sections, normalize_conflict_desc, normalize_review_section,
    parse_review_queue, prune_stale_questions, recover_review_section, strip_answered_questions,
    strip_deferred_answers_by_type, unwrap_review_callout, wrap_review_callout,
};

// Re-export citation quality scoring
pub use citations::{
    citation_failure_reason, compile_citation_patterns, detect_citation_type, is_citation_specific,
    is_citation_specific_with_patterns, validate_citation, CitationType,
};

// Re-export chunking types and functions
pub use chunks::{chunk_document, DocumentChunk};

// Re-export stats types and functions
pub use stats::{calculate_fact_stats, count_facts, count_facts_with_temporal_tags};

// Re-export links types and functions
pub use links::{
    append_links_to_content, append_links_to_content_styled, append_referenced_by_to_content,
    append_referenced_by_to_content_styled, extract_wikilink_names, migrate_links,
    parse_links_block, parse_referenced_by_block,
};

// Re-export format layer
pub use format::{
    build_document_header, extract_extra_frontmatter, format_link, format_references_line,
    merge_path_tags, tags_from_path, update_frontmatter_type, wikilink_path,
};

// Note: Unit tests are distributed to their respective submodules:
// - core.rs: 16 tests (ID, title, type, hash)
// - temporal.rs: 24 tests (temporal tag parsing)
// - sources.rs: 31 tests (source reference parsing)
// - review/: 81 tests (review queue parsing, normalization, pruning, appending, callout)
// - chunks.rs: 5 tests (document chunking)
// - stats.rs: 15 tests (fact statistics)
