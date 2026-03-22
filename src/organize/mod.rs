//! Self-organizing knowledge base operations.
//!
//! This module provides fact-level tracking for reorganization operations
//! (merge, split, move, retype) to ensure no data is silently lost.

pub mod detect;
pub mod entity_folder;
pub mod execute;
mod extract;
pub(crate) mod fs_helpers;
mod links;
mod orphans;
pub mod plan;
mod review;
pub mod suggestions;
pub(crate) mod test_helpers;
mod types;
mod verify;

pub use detect::{
    assess_staleness, cosine_similarity, detect_duplicate_entries, detect_ghost_files,
    detect_merge_candidates, detect_misplaced, detect_split_candidates, discover_entities,
    extract_entity_entries, extract_sections, EntityEntry, StaleDuplicate, SuggestedEntity,
};
pub use entity_folder::is_entity_folder;
pub use execute::{
    execute_merge, execute_move, execute_retype, execute_split, extract_type_override, MergeResult,
    MoveResult, RetypeResult, SplitResult,
};
pub use extract::extract_facts;
pub use fs_helpers::clean_canonicalize;
pub use links::{redirect_database_links, redirect_file_links, redirect_links};
pub use orphans::{write_orphans, OrphanOperation};
pub use plan::{plan_merge, plan_split, MergePlan, ProposedDocument, SplitPlan};
pub use review::{
    count_orphans, has_orphans, load_orphan_entries, orphan_file_path, parse_orphan_entries,
    process_orphan_answers, validate_orphan_answer, OrphanAnswer, OrphanEntry, OrphanProcessResult,
};
pub use suggestions::{execute_suggestions, SuggestionExecutionResult};
pub use types::{
    DuplicateEntry, EntryLocation, FactAssignment, FactDestination, FactLedger, GhostFile,
    MergeCandidate, MisplacedCandidate, SplitCandidate, SplitSection, TemporalIssue, TrackedFact,
};
pub use verify::{verify_merge, verify_split, VerificationResult};
