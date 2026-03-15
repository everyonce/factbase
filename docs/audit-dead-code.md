# Dead Code Audit

**Date:** 2026-03-15  
**Method:** `cargo check` (zero warnings — all dead code is suppressed by `#[allow]` or re-exported via `lib.rs`), manual grep analysis of callers.

> **Note:** `cargo check` emits zero warnings because the codebase uses `#[allow(dead_code)]` annotations and `lib.rs` re-exports to silence the compiler. All findings below were identified by tracing call sites manually.

---

## Findings

| File | Item | Reason it's dead | Safe to remove? |
|------|------|-----------------|-----------------|
| `src/mcp/tools/links.rs:328` | `migrate_repo_links()` | `#[allow(dead_code)]` — removed from MCP dispatch; only called in its own test block | Yes — or keep if tests are valuable |
| `src/mcp/tools/repository.rs:305` | `init_repository()` | `#[allow(dead_code)]` — removed from MCP dispatch; only called in its own test block | Yes — or keep if tests are valuable |
| `src/mcp/tools/entity.rs:45` | `list_repositories()` (MCP tool wrapper) | `#[allow(dead_code)]` — removed from MCP dispatch; `services::list_repositories` is used directly by web API, but this wrapper is never called | Yes — wrapper is dead; underlying service is live |
| `src/answer_processor/validate.rs:12` | `ValidationError::kind` field | `#[allow(dead_code)]` — only accessed in `#[cfg(test)]` blocks; production code only uses `ValidationError::detail` | Caution — used in tests; field itself is harmless |
| `src/database/stats/compression.rs:16` | `#[allow(unused_mut)]` on `compressed_docs` | `compressed_docs` is only mutated inside `#[cfg(feature = "compression")]`; without that feature the mutation is dead | Harmless; annotation is correct |
| `src/lib.rs:101–209` | All `#[deprecated]` re-exports (~20 items) | Compatibility shim layer; used only in integration tests (`tests/`) via `use factbase::EmbeddingProvider`, `use factbase::format_bytes`, `use factbase::cosine_similarity`, etc. | No — still used by integration tests |
| `src/shutdown.rs:52` | `reset_shutdown_flag()` | `pub fn` but only called inside `#[cfg(test)]` blocks within `shutdown.rs` itself | Yes — change to `#[cfg(test)] pub(crate) fn` |
| `src/watcher.rs:54` | `FileWatcher::unwatch_directory()` | Defined but never called anywhere outside the struct | Yes — no callers found |
| `src/link_detection.rs:308` | `LinkDetector::with_batch_size()` | Defined but never called; `LinkDetector::new()` is used everywhere | Yes — no callers found |
| `src/output.rs:41` | `is_tty()` | Only called internally by `should_use_color()` in the same file; not used outside `output.rs` | No — internal helper, used by `should_use_color` |
| `src/output.rs:53` | `should_use_color()` | Only called internally by `should_highlight()` in the same file; not used outside `output.rs` | No — internal helper, used by `should_highlight` |
| `src/output.rs:107` | `highlight_text()` | Only called in its own test block; no production callers found | Yes — no production callers |
| `src/processor/stats.rs:12` | `count_facts()` | Re-exported via `processor::mod` and `lib.rs`; only called internally by `calculate_fact_stats()` and in tests; no external production callers | Caution — used by `calculate_fact_stats` internally |
| `src/processor/stats.rs:20` | `count_facts_with_temporal_tags()` | Re-exported via `processor::mod` and `lib.rs`; only called internally by `calculate_fact_stats()` and in tests; no external production callers | Caution — used by `calculate_fact_stats` internally |
| `src/processor/temporal/validation.rs:76` | `detect_illogical_sequences()` | Re-exported via `processor::mod` and `lib.rs`; no callers found outside its own file's tests | Yes — no production callers |
| `src/processor/temporal/validation.rs:148` | `detect_temporal_conflicts()` | Re-exported via `processor::mod` and `lib.rs`; no callers found outside its own file's tests | Yes — no production callers |
| `src/question_generator/citation.rs:95` | `collect_weak_citations()` | Re-exported via `question_generator::mod` and `lib.rs`; no callers found outside its own file's tests | Yes — no production callers |
| `src/question_generator/citation.rs:159` | `format_citation_triage_batch()` | Re-exported; no callers outside its own file's tests | Yes — no production callers |
| `src/question_generator/citation.rs:190` | `format_citation_resolve_batch()` | Re-exported; no callers outside its own file's tests | Yes — no production callers |
| `src/question_generator/citation.rs:220` | `format_citation_batch()` | Documented as deprecated alias for `format_citation_triage_batch`; no callers outside tests | Yes — no production callers |
| `src/question_generator/fields.rs:18` | `detect_document_fields()` | Only called internally by `generate_required_field_questions()` in the same file; not used outside | No — internal helper |
| `src/organize/detect/staleness.rs:95` | `generate_stale_entry_questions()` | Re-exported via `organize::mod` and `lib.rs`; no callers found outside its own file's tests | Yes — no production callers |
| `src/organize/snapshot.rs:97` | `create_snapshot()` | Re-exported via `organize::mod` and `lib.rs`; no callers found in production code (execute/ module doesn't use it) | Yes — snapshot/rollback system appears unused |
| `src/organize/snapshot.rs:177` | `rollback()` | Same as above — no production callers | Yes — snapshot/rollback system appears unused |
| `src/organize/snapshot.rs:226` | `cleanup()` | Same as above — no production callers | Yes — snapshot/rollback system appears unused |
| `src/answer_processor/apply.rs:16` | `format_changes_for_llm()` | Re-exported via `answer_processor::mod` and `lib.rs`; no callers outside its own file's tests | Yes — LLM rewrite path removed |
| `src/answer_processor/apply.rs:82` | `build_rewrite_prompt()` | Same — no callers outside tests | Yes — LLM rewrite path removed |
| `src/answer_processor/inbox.rs:23` | `extract_inbox_blocks()` | Re-exported via `lib.rs`; no callers outside its own file | Yes — inbox integration unused |
| `src/answer_processor/inbox.rs:57` | `strip_inbox_blocks()` | Only called internally by `apply_inbox_integration()` in the same file | No — internal helper (but parent is dead) |
| `src/answer_processor/inbox.rs:111` | `build_inbox_prompt()` | Only called internally by `apply_inbox_integration()` in the same file | No — internal helper (but parent is dead) |
| `src/answer_processor/inbox.rs:135` | `apply_inbox_integration()` | Re-exported via `lib.rs`; no callers outside its own file | Yes — entire inbox module appears unused |
| `src/answer_processor/apply_all.rs:68` | `apply_all_review_answers()` + `ApplyConfig`, `ApplyResult`, `ApplyDocResult`, `ApplyStatus` | Re-exported via `lib.rs`; no callers found in any production code (commands, mcp, web, services) | Caution — verify no CLI command uses it before removing |
| `src/database/cross_validation.rs:71` | `clear_cross_validation_state()` | Only called in its own test block; no production callers | Yes — no production callers |
| `src/database/cross_validation.rs:148` | `extend_cv_lease()` | Only called in its own test block; no production callers | Yes — no production callers |
| `src/database/cross_validation.rs:159` | `release_cv_lock()` | Only called in its own test block; no production callers | Yes — no production callers |
| `src/services/review/helpers.rs:8` | `parse_type_filter()` | No callers found outside its own test block | Yes — no production callers |

---

## Summary by Category

### `#[allow(dead_code)]` Annotations (4)
- `migrate_repo_links` — removed from dispatch, kept for tests
- `init_repository` — removed from dispatch, kept for tests  
- `list_repositories` (MCP wrapper) — removed from dispatch
- `ValidationError::kind` — test-only field

### Functions with No Production Callers (high confidence)
- `reset_shutdown_flag` — test-only, should be `#[cfg(test)]`
- `FileWatcher::unwatch_directory` — no callers anywhere
- `LinkDetector::with_batch_size` — no callers anywhere
- `highlight_text` — test-only
- `detect_illogical_sequences` / `detect_temporal_conflicts` — test-only
- `collect_weak_citations` / `format_citation_*` — test-only (Tier 2 citation API, never wired up)
- `generate_stale_entry_questions` — test-only
- `create_snapshot` / `rollback` / `cleanup` — snapshot system built but never called
- `format_changes_for_llm` / `build_rewrite_prompt` — LLM rewrite path removed
- `apply_inbox_integration` + entire inbox module — inbox feature never wired up
- `apply_all_review_answers` + types — no callers in any command/mcp/web/service
- `clear_cross_validation_state` / `extend_cv_lease` / `release_cv_lock` — test-only
- `parse_type_filter` — test-only

### Deprecated Re-exports in `lib.rs`
~20 `#[deprecated]` re-exports exist as a compatibility shim. They are still used by integration tests in `tests/`. Safe to remove only after updating those tests to use direct module paths.

### Largest Dead Subsystems
1. **Inbox integration** (`answer_processor/inbox.rs`) — complete feature, never wired to any command or MCP tool
2. **Snapshot/rollback** (`organize/snapshot.rs`) — complete implementation, never called by execute/ operations
3. **LLM rewrite path** (`format_changes_for_llm`, `build_rewrite_prompt`) — remnant of removed LLM-driven answer application
4. **Tier 2 citation API** (`collect_weak_citations`, `format_citation_*`) — designed for batch LLM review, never integrated
