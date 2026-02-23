pub mod answer_processor;
pub(crate) mod async_helpers;
#[cfg(feature = "bedrock")]
pub mod bedrock;
pub(crate) mod cache;
pub mod config;
pub mod database;
pub mod embedding;
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
pub mod question_generator;
pub mod scanner;
pub(crate) mod shutdown;
pub mod watcher;
#[cfg(feature = "web")]
pub mod web;

pub use answer_processor::{
    apply_changes_to_section, identify_affected_section,
    inbox::{apply_inbox_integration, extract_inbox_blocks},
    interpret_answer, remove_processed_questions, replace_section, ChangeInstruction,
    InterpretedAnswer,
};
pub use config::Config;
pub use database::Database;
pub use embedding::{CachedEmbedding, EmbeddingProvider, OllamaEmbedding};
pub use error::{format_user_error, format_warning, repo_not_found, FactbaseError};
pub use llm::{DetectedLink, LinkDetector, LlmProvider, OllamaLlm, ReviewLlm};
#[cfg(feature = "mcp")]
pub use mcp::McpServer;
pub use models::{
    normalize_pair, ContentMatch, ContentSearchResult, DetailedStats, Document, Link, PoolStats,
    QuestionType, RepoStats, Repository, ReviewQuestion, ScanResult, ScanStats, SearchResult,
    SourceStats, TemporalStats, TemporalTagType,
};
pub use ollama::create_http_client;
pub use organize::{
    cleanup, create_snapshot, detect_merge_candidates, detect_misplaced, detect_split_candidates,
    execute_merge, execute_move, execute_split, extract_sections, has_orphans, load_orphan_entries,
    plan_merge, plan_split, process_orphan_answers, rollback, verify_merge, verify_split,
    MergeCandidate, MergePlan, MergeResult, MisplacedCandidate, MoveResult, SplitCandidate,
    SplitPlan, SplitResult, SplitSection, VerificationResult,
};
pub use output::{ansi, format_bytes, format_json, format_yaml, set_no_color, should_highlight};
pub use patterns::MANUAL_LINK_REGEX;
pub use processor::{
    append_review_questions, calculate_fact_stats, calculate_recency_boost, chunk_document,
    count_facts_with_sources, detect_illogical_sequences, detect_temporal_conflicts,
    overlaps_point, overlaps_range, parse_review_queue, parse_source_definitions,
    parse_source_references, parse_temporal_tags, validate_temporal_tags, DocumentProcessor,
};
pub use question_generator::cross_validate::cross_validate_document;
pub use question_generator::{
    generate_ambiguous_questions, generate_conflict_questions, generate_duplicate_questions,
    generate_missing_questions, generate_required_field_questions, generate_stale_questions,
    generate_temporal_questions,
};
pub use scanner::{full_scan, scan_all_repositories, ScanOptions, Scanner};
pub use shutdown::init_shutdown_handler;
pub use watcher::{find_repo_for_path, FileWatcher, ScanCoordinator};
#[cfg(feature = "web")]
pub use web::start_web_server;
