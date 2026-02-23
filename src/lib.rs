/// Write formatted text to a `String`. Wraps `write!` with `std::fmt::Write`,
/// panicking on error (writing to `String` is infallible).
#[macro_export]
macro_rules! write_str {
    ($dst:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        write!($dst, $($arg)*).expect("write to String infallible")
    }};
}

/// Write formatted text with newline to a `String`. Wraps `writeln!` with `std::fmt::Write`,
/// panicking on error (writing to `String` is infallible).
#[macro_export]
macro_rules! writeln_str {
    ($dst:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        writeln!($dst, $($arg)*).expect("write to String infallible")
    }};
}

pub mod answer_processor;
pub(crate) mod async_helpers;
#[cfg(feature = "bedrock")]
pub mod bedrock;
pub(crate) mod cache;
pub mod config;
pub mod database;
pub mod embedding;
pub mod embeddings_io;
pub mod error;
pub mod llm;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod models;
pub(crate) mod ollama;
pub mod organize;
pub mod output;
pub(crate) mod patterns;
pub mod processor;
pub mod progress;
pub mod question_generator;
pub mod scanner;
pub(crate) mod shutdown;
pub mod watcher;
#[cfg(feature = "web")]
pub mod web;

/// Default repository ID used when no explicit ID is provided.
pub const DEFAULT_REPO_ID: &str = "default";

/// Boxed future type alias for async trait methods (no async-trait crate).
pub(crate) type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub use answer_processor::{
    apply_all::{apply_all_review_answers, ApplyConfig, ApplyDocResult, ApplyResult, ApplyStatus},
    apply_changes_to_section, apply_confirmations, apply_source_citations, classify_answer,
    identify_affected_section,
    inbox::{apply_inbox_integration, extract_inbox_blocks},
    interpret_answer, remove_processed_questions, replace_section, stamp_reviewed_by_text,
    stamp_reviewed_lines, stamp_reviewed_markers, stamp_sequential_by_text,
    stamp_sequential_lines, uncheck_deferred_questions, AnswerType,
    ChangeInstruction, InterpretedAnswer,
};
pub use config::Config;
pub use database::{ContentSearchParams, Database};
pub use embedding::{CachedEmbedding, EmbeddingProvider, OllamaEmbedding, PersistentCachedEmbedding};
pub use embeddings_io::{
    embeddings_status, export_embeddings, export_embeddings_to_file, import_embeddings,
    import_embeddings_from_file, EmbeddingExportHeader, EmbeddingRecord, EmbeddingsStatusInfo,
    ImportResult, FORMAT_VERSION as EMBEDDING_FORMAT_VERSION,
};
pub use error::{format_user_error, format_warning, repo_not_found, FactbaseError};
pub use llm::{DetectedLink, LinkDetector, LlmProvider, OllamaLlm, ReviewLlm};
#[cfg(feature = "mcp")]
pub use mcp::McpServer;
pub use models::{
    load_perspective_from_file, normalize_pair, ContentMatch, ContentSearchResult, DetailedStats,
    Document, Link, PoolStats, QuestionType, RepoStats, Repository, ReviewQuestion, ScanResult,
    ScanStats, SearchResult, SourceStats, TemporalStats, TemporalTagType, PERSPECTIVE_TEMPLATE,
};
pub use ollama::create_http_client;
pub use organize::{
    assess_staleness, cleanup, cosine_similarity, create_snapshot, detect_duplicate_entries,
    detect_merge_candidates, detect_misplaced, detect_split_candidates, execute_merge,
    execute_move, execute_split, extract_sections, generate_stale_entry_questions, has_orphans,
    load_orphan_entries, plan_merge, plan_split, process_orphan_answers, rollback, verify_merge,
    verify_split, DuplicateEntry, EntryLocation, MergeCandidate, MergePlan, MergeResult,
    MisplacedCandidate, MoveResult, SplitCandidate, SplitPlan, SplitResult, SplitSection,
    StaleDuplicate, VerificationResult,
};
pub use output::{ansi, format_bytes, format_json, format_yaml, set_no_color, should_highlight};
pub use patterns::{
    extract_reviewed_date, strip_reviewed_markers, FACT_LINE_REGEX, MANUAL_LINK_REGEX,
};
pub use processor::{
    append_review_questions, calculate_fact_stats, calculate_recency_boost, chunk_document,
    content_hash, count_facts_with_sources, detect_illogical_sequences, detect_temporal_conflicts,
    normalize_conflict_desc, normalize_review_section, overlaps_point, overlaps_range,
    parse_review_queue, prune_stale_questions,
    parse_source_definitions, parse_source_references, parse_temporal_tags, validate_temporal_tags,
    DocumentProcessor,
};
pub use progress::{ProgressReporter, ProgressSender};
pub use question_generator::cross_validate::cross_validate_document;
pub use question_generator::{
    extract_defined_terms, filter_sequential_conflicts, generate_ambiguous_questions,
    generate_conflict_questions, generate_corruption_questions,
    generate_duplicate_questions, generate_duplicate_entry_questions, generate_missing_questions,
    generate_required_field_questions, generate_source_quality_questions,
    generate_stale_questions, generate_temporal_questions,
};
pub use scanner::{full_scan, scan_all_repositories, ScanContext, ScanOptions, Scanner};
pub use shutdown::init_shutdown_handler;
pub use watcher::{find_repo_for_path, FileWatcher, ScanCoordinator};
#[cfg(feature = "web")]
pub use web::start_web_server;
