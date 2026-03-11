#![recursion_limit = "256"]
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
pub mod link_detection;
#[cfg(feature = "local-embedding")]
pub mod local_embedding;
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
pub mod services;
pub(crate) mod shutdown;
pub mod watcher;
pub(crate) mod write_guard;
#[cfg(feature = "web")]
pub mod web;

/// Default repository ID used when no explicit ID is provided.
pub const DEFAULT_REPO_ID: &str = "default";

/// Entries that should be in .gitignore for any factbase repository.
const GITIGNORE_ENTRIES: &[&str] = &[".factbase/", ".fastembed_cache/"];

/// Ensure `.gitignore` in `repo_root` contains factbase entries.
/// Creates the file if missing; appends missing entries if it exists.
/// Returns the list of entries that were added (empty if all already present).
pub fn ensure_gitignore(repo_root: &std::path::Path) -> std::io::Result<Vec<&'static str>> {
    let path = repo_root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: std::collections::HashSet<&str> = existing.lines().map(|l| l.trim()).collect();
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .iter()
        .copied()
        .filter(|e| !lines.contains(e))
        .collect();
    if missing.is_empty() {
        return Ok(vec![]);
    }
    let mut append = String::new();
    if !existing.is_empty() && !existing.ends_with('\n') {
        append.push('\n');
    }
    for entry in &missing {
        append.push_str(entry);
        append.push('\n');
    }
    if existing.is_empty() {
        std::fs::write(&path, append)?;
    } else {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
        f.write_all(append.as_bytes())?;
    }
    Ok(missing)
}

/// Boxed future type alias for async trait methods (no async-trait crate).
pub(crate) type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub use answer_processor::{
    apply_all::{apply_all_review_answers, ApplyConfig, ApplyDocResult, ApplyResult, ApplyStatus},
    apply_changes_to_section, apply_confirmations, apply_source_citations, classify_answer,
    dedup_titles, identify_affected_section,
    inbox::{apply_inbox_integration, extract_inbox_blocks},
    interpret_answer, remove_processed_questions, replace_section, stamp_reviewed_by_text,
    stamp_reviewed_lines, stamp_reviewed_markers, stamp_sequential_by_text,
    stamp_sequential_lines, uncheck_deferred_questions, AnswerType,
    ChangeInstruction, InterpretedAnswer,
};
pub use config::Config;
pub use config::prompts::{resolve_prompt, PromptsConfig};
pub use config::workflows::{resolve_workflow_text, WorkflowsConfig};
pub use database::{ContentSearchParams, Database};
pub use embedding::{CachedEmbedding, EmbeddingProvider, OllamaEmbedding, PersistentCachedEmbedding};
#[cfg(feature = "local-embedding")]
pub use local_embedding::LocalEmbeddingProvider;
pub use embeddings_io::{
    embeddings_status, export_embeddings, export_embeddings_to_file, import_embeddings,
    import_embeddings_from_file, EmbeddingExportHeader, EmbeddingRecord, EmbeddingsStatusInfo,
    FactEmbeddingRecord, ImportResult, FORMAT_VERSION as EMBEDDING_FORMAT_VERSION,
};
pub use error::{format_user_error, format_warning, repo_not_found, FactbaseError};
pub use link_detection::{DetectedLink, LinkDetector};
#[cfg(feature = "mcp")]
pub use mcp::McpServer;
pub use models::{
    load_perspective_from_file, normalize_pair, ContentMatch, ContentSearchResult, DetailedStats,
    Document, FormatConfig, IdPlacement, Link, LinkStyle, PoolStats, QuestionType, RepoStats,
    Repository, ResolvedFormat, ReviewQuestion, ScanResult, ScanStats, SearchResult, SourceStats,
    TemporalStats, TemporalTagType, PERSPECTIVE_TEMPLATE,
};
pub use ollama::create_http_client;
pub use organize::{
    assess_staleness, cleanup, cosine_similarity, create_snapshot, detect_duplicate_entries,
    detect_ghost_files, detect_merge_candidates, detect_misplaced, detect_split_candidates,
    discover_entities, execute_merge, execute_move, execute_split, extract_sections,
    generate_stale_entry_questions, has_orphans, load_orphan_entries, plan_merge, plan_split,
    process_orphan_answers, rollback, verify_merge, verify_split, DuplicateEntry, EntryLocation,
    GhostFile, MergeCandidate, MergePlan, MergeResult, MisplacedCandidate, MoveResult,
    SplitCandidate, SplitPlan, SplitResult, SplitSection, StaleDuplicate, SuggestedEntity,
    TemporalIssue, VerificationResult,
};
pub use output::{ansi, format_bytes, format_json, format_yaml, set_no_color, should_highlight};
pub use patterns::{
    content_body, convert_inline_reviewed_to_frontmatter, extract_frontmatter_reviewed_date,
    extract_reviewed_date, is_reference_doc, set_frontmatter_reviewed_date,
    strip_reviewed_markers, FACT_LINE_REGEX, MANUAL_LINK_REGEX, REFERENCE_MARKER,
    WIKILINK_REGEX,
};
pub use processor::{
    append_links_to_content, append_referenced_by_to_content,
    append_referenced_by_to_content_styled, append_review_questions,
    calculate_fact_stats,
    calculate_recency_boost, chunk_document,
    content_hash, count_facts_with_sources, detect_illogical_sequences, detect_temporal_conflicts,
    is_callout_review, is_citation_specific,
    merge_duplicate_review_sections, normalize_conflict_desc, normalize_review_section,
    overlaps_point, overlaps_range,
    parse_links_block, parse_referenced_by_block, parse_review_queue, prune_stale_questions,
    extract_wikilink_names,
    parse_source_definitions, parse_source_references, parse_temporal_tags,
    strip_answered_questions, unwrap_review_callout, validate_temporal_tags,
    wrap_review_callout,
    build_document_header, format_link, format_references_line, wikilink_path,
    DocumentProcessor,
};
pub use progress::{ProgressReporter, ProgressSender};
pub use question_generator::check::VocabCandidate;
pub use question_generator::cross_validate::make_pair_id;
pub use question_generator::{
    collect_defined_terms, collect_defined_terms_with_types, extract_acronym_from_question, extract_defined_terms, is_glossary_doc, is_glossary_doc_with_types,
    filter_sequential_conflicts, generate_ambiguous_questions,
    generate_conflict_questions, generate_corruption_questions,
    generate_duplicate_questions, generate_duplicate_entry_questions, generate_missing_questions,
    generate_precision_questions, generate_required_field_questions,
    generate_source_quality_questions, generate_weak_source_questions,
    generate_stale_questions, generate_temporal_questions,
};
pub use scanner::{full_scan, run_fact_embedding_phase, scan_all_repositories, FactEmbeddingInput, FactEmbeddingOutput, ScanContext, ScanOptions, Scanner};
pub use shutdown::init_shutdown_handler;
pub use watcher::{find_repo_for_path, FileWatcher, ScanCoordinator};
#[cfg(feature = "web")]
pub use web::start_web_server;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_gitignore_creates_new_file() {
        let tmp = TempDir::new().unwrap();
        let added = ensure_gitignore(tmp.path()).unwrap();
        assert_eq!(added, vec![".factbase/", ".fastembed_cache/"]);
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content, ".factbase/\n.fastembed_cache/\n");
    }

    #[test]
    fn test_ensure_gitignore_appends_missing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "dist/\n").unwrap();
        let added = ensure_gitignore(tmp.path()).unwrap();
        assert_eq!(added, vec![".factbase/", ".fastembed_cache/"]);
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.starts_with("dist/\n"));
        assert!(content.contains(".factbase/\n"));
    }

    #[test]
    fn test_ensure_gitignore_skips_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), ".factbase/\n.fastembed_cache/\n").unwrap();
        let added = ensure_gitignore(tmp.path()).unwrap();
        assert!(added.is_empty());
    }

    #[test]
    fn test_ensure_gitignore_partial_existing() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), ".factbase/\n").unwrap();
        let added = ensure_gitignore(tmp.path()).unwrap();
        assert_eq!(added, vec![".fastembed_cache/"]);
    }

    #[test]
    fn test_ensure_gitignore_no_trailing_newline() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "dist/").unwrap();
        ensure_gitignore(tmp.path()).unwrap();
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        // Should add newline before appending
        assert!(content.starts_with("dist/\n"));
    }

    #[test]
    fn test_ensure_gitignore_idempotent() {
        let tmp = TempDir::new().unwrap();
        ensure_gitignore(tmp.path()).unwrap();
        let added = ensure_gitignore(tmp.path()).unwrap();
        assert!(added.is_empty());
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches(".factbase/").count(), 1);
    }
}
