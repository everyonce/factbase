//! Self-organizing knowledge base operations.
//!
//! This module provides fact-level tracking for reorganization operations
//! (merge, split, move, retype) to ensure no data is silently lost.

mod audit;
pub mod detect;
pub mod execute;
mod extract;
pub(crate) mod fs_helpers;
mod links;
mod orphans;
pub mod plan;
mod review;
mod snapshot;
pub(crate) mod test_helpers;
mod types;
mod verify;

#[allow(unused_imports)] // operational utility — not yet wired to CLI/MCP
pub(crate) use audit::{
    audit_log_dir, list_audit_logs, read_audit_log, write_audit_log, AuditEntry, FactMapping,
    FactSummary, MergeDetails, MoveDetails, OperationDetails, OperationType, RetypeDetails,
    SplitDetails,
};
pub use detect::{
    assess_staleness, detect_duplicate_entries, detect_merge_candidates, detect_misplaced,
    detect_split_candidates, extract_entity_entries, extract_sections,
    generate_stale_entry_questions, EntityEntry, StaleDuplicate,
};
pub use execute::{
    execute_merge, execute_move, execute_retype, execute_split, extract_type_override, MergeResult,
    MoveResult, RetypeResult, SplitResult,
};
pub use extract::extract_facts;
pub use links::{redirect_database_links, redirect_file_links, redirect_links};
pub use orphans::{write_orphans, OrphanOperation};
pub use plan::{plan_merge, plan_split, MergePlan, ProposedDocument, SplitPlan};
pub use review::{
    count_orphans, has_orphans, load_orphan_entries, orphan_file_path, parse_orphan_entries,
    process_orphan_answers, validate_orphan_answer, OrphanAnswer, OrphanEntry, OrphanProcessResult,
};
pub use snapshot::{cleanup, create_snapshot, rollback, DocumentBackup, FileBackup, Snapshot};
pub use types::{
    DuplicateEntry, EntryLocation, FactAssignment, FactDestination, FactLedger, MergeCandidate,
    MisplacedCandidate, SplitCandidate, SplitSection, TrackedFact,
};
pub use verify::{verify_merge, verify_split, VerificationResult};
