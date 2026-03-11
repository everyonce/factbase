# Architecture Review — Factbase Codebase

**Date**: 2026-03-10
**Codebase**: 81,204 lines of Rust across 240 files (src/)
**Tests**: ~2,117 unit/binary tests + 73 integration + 56 frontend + 12 E2E

---

## 1. Module Structure & Cohesion

### 1.1 Size Distribution

The codebase has a long tail of well-sized files, but several outliers:

| File | Lines | Assessment |
|------|------:|------------|
| `mcp/tools/workflow.rs` | 4,666 | **Critical** — largest file by 2.4x. ~51 `const` instruction strings dominate. |
| `processor/review.rs` | 1,911 | **Large** — review queue parsing + callout format handling + question appending. |
| `mcp/tools/document.rs` | 1,381 | **Large** — CRUD + coverage analysis + format detection. |
| `answer_processor/apply_all.rs` | 1,372 | **Large** — shared apply loop for CLI + web. Acceptable given complexity. |
| `mcp/tools/mod.rs` | 1,366 | **Large** — dispatch routing + types. The dispatch table is clean but the file carries too many responsibilities. |
| `question_generator/conflict.rs` | 1,353 | **Large** — conflict detection is inherently complex. Acceptable. |
| `question_generator/check.rs` | 1,341 | **Large** — shared lint loop for MCP + CLI. Acceptable. |
| `answer_processor/apply.rs` | 1,330 | **Large** — change application logic. Acceptable. |
| `mcp/tools/review/answer.rs` | 1,255 | **Large** — answer processing MCP tool. |
| `answer_processor/interpret.rs` | 1,198 | **Large** — answer classification. Lots of string matching. |
| `patterns.rs` | 1,147 | **Large** — regex consolidation file. Acceptable as a pattern registry. |

**Summary**: 11 files exceed 1,000 lines. `workflow.rs` at 4,666 lines is the clear problem — it's mostly static instruction text, not logic.

### 1.2 Module Boundary Assessment

**Well-structured modules** (clear single responsibility):
- `database/` — Clean split into documents/, search/, stats/, links, embeddings, schema
- `models/` — Pure data structures, well-separated by domain
- `config/` — Each config section in its own file
- `processor/temporal/` — Date parsing, validation, range handling
- `organize/detect/` — Each detection algorithm isolated
- `organize/execute/` — Each operation (merge, split, move, retype) isolated
- `error.rs` — Single error enum with good `From` impls
- `write_guard.rs` — Tiny, focused RAII guard
- `async_helpers.rs` — Single `run_blocking` utility

**Modules with mixed responsibilities**:
- `mcp/tools/workflow.rs` — Mixes instruction text (data) with step dispatch logic (code). The 51 `const` strings are configuration masquerading as code.
- `processor/review.rs` (1,911 lines) — Handles parsing, callout format conversion, question appending, deduplication, and normalization. Could split into `review/parse.rs`, `review/callout.rs`, `review/append.rs`.
- `mcp/tools/document.rs` (1,381 lines) — CRUD operations + coverage analysis + format detection + path resolution. The coverage analysis and format detection could be extracted.
- `commands/utils.rs` + `commands/setup.rs` — Both are "grab bag" utility files. `setup.rs` has 5 different setup functions with a helpful table in the doc comment, but `utils.rs` mixes error helpers, path validation, output formatting, and repository resolution.

**Modules that are too small to justify separate files**:
- `organize/plan/mod.rs` (13 lines) — Just re-exports
- `organize/execute/mod.rs` (14 lines) — Just re-exports
- `web/mod.rs` (11 lines) — Just re-exports
- `commands/mcp.rs` (21 lines) — Single function

These are fine as organizational scaffolding — they don't add cognitive overhead.

### 1.3 Dependency Direction

Dependencies flow cleanly downward:

```
main.rs (bin) → commands/ → lib.rs (factbase crate)
                              ├── mcp/ → tools/ → database, processor, scanner, etc.
                              ├── scanner/ → processor, database, embedding
                              ├── processor/ → patterns, models
                              ├── database/ → models
                              └── models/ (leaf)
```

**No circular dependencies detected.** The dependency graph is acyclic.

**One questionable cross-dependency**: `web/api/` imports from `mcp/tools/` directly:
```rust
// web/api/review.rs
use crate::mcp::tools::{answer_question, bulk_answer_questions, get_review_queue};
// web/api/documents.rs
use crate::mcp::tools::{get_entity, list_repositories};
```

This creates a coupling between the web layer and the MCP layer. The shared logic (answer processing, entity retrieval) should live in a shared service layer, not in MCP tools.

### 1.4 The `lib.rs` Re-export Surface

`lib.rs` has 25 `pub use` blocks re-exporting ~120 symbols. This is a massive flat API surface. While it enables `use factbase::*` convenience, it:
- Makes it hard to understand what's public vs internal
- Creates implicit coupling — any consumer can reach deep internals
- Makes refactoring risky (moving a function breaks external imports)

The re-exports are primarily consumed by `commands/` (the binary crate). Since `commands/` is in the same repo, this is manageable but not ideal.

---

## 2. Interface Quality

### 2.1 MCP Tool Interface

**Strengths**:
- Clean 3-tool schema: `search`, `workflow`, `factbase` (unified ops)
- Legacy tool names preserved as dispatch aliases — good backward compatibility
- The `factbase` unified tool with `op` parameter is well-designed for agent ergonomics
- `op_to_tool_name()` mapping is clean and maintainable

**Weaknesses**:
- `handle_factbase_op()` has special-case logic for `answer` (doc_id propagation), `organize` (action sub-dispatch), `links` (action sub-dispatch), and `embeddings` (action sub-dispatch). This is 4 different dispatch patterns in one function.
- The `search` tool duplicates dispatch logic — it checks `mode` and calls either `search_knowledge` or `search_content`, but this could be handled by the unified `factbase` tool.
- `get_str_array_arg` is defined twice: once in `mcp/tools/helpers.rs` (returns `Option<Vec<String>>`) and once in `mcp/tools/links.rs` (returns `Vec<String>` with lowercase). Different signatures, same name.

### 2.2 CLI Interface

**Strengths**:
- Well-organized subcommands with clear grouping
- Hidden commands (`db`, `completions`, `version`) reduce noise
- Global flags (`--verbose`, `--log-level`, `--no-color`) work consistently
- Good help text with examples in arg definitions

**Weaknesses**:
- `commands/mod.rs` re-exports 30+ items in a flat list. No grouping.
- `setup.rs` has 5 different setup functions (`setup_database`, `setup_database_only`, `setup_database_checked`, `find_repo`, `find_repo_with_config`). The doc comment table helps, but this is a sign of an abstraction gap — there should be a builder or context struct.

### 2.3 Internal APIs

**Consistent patterns**:
- `Result<T, FactbaseError>` used everywhere
- `Database` methods follow `verb_noun` naming (`get_document`, `upsert_document`, `list_repositories`)
- MCP tool functions consistently take `(db, args)` or `(db, embedding, args)`

**Inconsistencies**:
- Some MCP tools are `async fn` (search, scan, check) while others are sync wrapped in `run_blocking`. This is correct (async for embedding calls, sync for DB), but the dispatch table in `mod.rs` has to handle both patterns with different syntax, making it noisy.
- `clean_canonicalize` is defined in `organize/fs_helpers.rs` and re-exported through `organize/mod.rs`, then wrapped in `commands/setup.rs` as a pass-through. The wrapper adds no value.

### 2.4 Error Handling

**Strengths**:
- `FactbaseError` enum is comprehensive with good `Display` messages
- `From` impls for common error types (IO, rusqlite, serde)
- Helper constructors (`FactbaseError::parse()`, `.not_found()`, etc.) reduce boilerplate
- `repo_not_found()` and `doc_not_found()` include actionable suggestions
- Schema mismatch errors include upgrade hints

**Weaknesses**:
- `FactbaseError::Llm` variant is unused in production code (only in tests). The LLM was removed in Phase 6 but the error variant remains.
- `FactbaseError::Ollama` is used in exactly one place (`ollama.rs:237`). Could be merged with `Embedding`.
- Error variants use `String` payloads everywhere. Structured error data (e.g., `NotFound { entity: String, id: String }`) would enable better programmatic handling.

---

## 3. User Experience

### 3.1 Workflow Instructions

**Strengths**:
- Workflow instructions are detailed and prescriptive — agents know exactly what to call next
- `⚠️ NEXT:` markers ensure agents don't stop mid-workflow
- Paging instructions (`continue: true`, `resume` token) are repeated at every relevant step
- Error handling guidance (IO/body errors → split batches) is inline
- Format rules are inlined into create steps so weaker models don't need extra calls

**Weaknesses**:
- Instructions are extremely long. `DEFAULT_INGEST_CREATE_INSTRUCTION` is a single string constant spanning ~30 lines. This works for capable models but may overwhelm smaller ones.
- The `FORMAT_RULES` constant (embedded in workflow.rs) duplicates information from `get_authoring_guide`. If the format changes, both must be updated.
- Workflow aliases (`bootstrap/setup→create`, `update→maintain`, `ingest/enrich/improve→add`) are documented in the schema but the mapping logic is spread across workflow.rs. A single alias table would be clearer.

### 3.2 Tool Descriptions

**Strengths**:
- The `factbase` tool schema has a comprehensive `description` with examples for every `op`
- The `workflow` tool description lists all workflows with use-case triggers ("user says X → workflow Y")
- The `search` tool clearly documents both modes

**Weaknesses**:
- The `factbase` tool description is very long (~100 lines in the schema). Agents with limited context may not read it all.
- No tool provides a "quick reference" — it's all-or-nothing detail.

### 3.3 Error Messages

**Strengths**:
- CLI errors include `hint:` lines with actionable next steps
- Database schema mismatch errors suggest upgrading or re-scanning
- MCP errors return structured JSON with `isError: true`

**Weaknesses**:
- Some MCP errors return raw Rust error strings (e.g., `"IO error: No such file or directory"`) without context about which file or what the user should do.
- The `WriteGuard` error message is good but doesn't tell the user how to check what's running.

### 3.4 Onboarding Path

The `create` workflow (formerly `bootstrap` + `setup`) is well-designed:
1. Bootstrap generates domain-specific suggestions
2. Init creates the repository
3. Perspective configuration with YAML template
4. Validation with clear error/success feedback
5. Example document creation
6. Scan + verify

This is smooth. The main risk is step 3 (perspective.yaml) — if the agent writes invalid YAML, the error message from `serde_yaml_ng` may not be helpful enough.

---

## 4. Dead/Obsolete Code

### 4.1 Unused Error Variants

- `FactbaseError::Llm` — The LLM was removed in Phase 6. This variant is only exercised in error.rs tests. **Remove it.**

### 4.2 Dead Code Annotations

- `mcp/tools/workflow.rs` has 8 `#[allow(dead_code)]` annotations on legacy `DEFAULT_UPDATE_*` constants. The comment says "kept for test coverage, aliased to maintain in production." If they're only used in tests, they should be `#[cfg(test)]` instead.
- `organize/audit.rs` has `#![allow(dead_code)]` on the entire file with comment "operational utility — not yet wired to CLI/MCP." This is 540 lines of code that's built but never called. Either wire it up or gate it behind a feature flag.
- `organize/mod.rs` has `#[allow(unused_imports)]` on audit re-exports.

### 4.3 TODO Comments

Only one real TODO found in production code:
```rust
// src/config/workflows.rs:109
// TODO: update.check and update.scan instructions should explicitly mention time_budget_secs (#197, #198)
```

This is a minor documentation improvement. The codebase is remarkably clean of TODOs.

### 4.4 Duplicate Logic

- **`get_str_array_arg`**: Defined in `mcp/tools/helpers.rs` (returns `Option<Vec<String>>`) and `mcp/tools/links.rs` (returns `Vec<String>`, lowercases values). The `links.rs` version should use the helpers version and apply lowercase separately.
- **`clean_canonicalize`**: Defined in `organize/fs_helpers.rs`, re-exported through `organize/mod.rs`, then wrapped as a pass-through in `commands/setup.rs`. The wrapper should be removed; callers should use `factbase::organize::clean_canonicalize` directly.
- **Review question generation**: `mcp/tools/review/generate.rs` and `question_generator/check.rs` both orchestrate question generation with similar but not identical logic. `check.rs` is the shared version, but `generate.rs` has its own duplicate-question handling and corruption-check logic that diverges.

### 4.5 Potentially Obsolete Code

- **`organize/audit.rs`** (540 lines): Entire module is `#![allow(dead_code)]`. Built but never called from CLI or MCP. Has been in this state since Phase 10.
- **Legacy workflow constants**: 7 `DEFAULT_UPDATE_*` constants in `workflow.rs` are marked `#[allow(dead_code)]`. If the `update` workflow is now aliased to `maintain`, these constants may be fully dead.

---

## 5. Reorganization Opportunities

### 5.1 Critical: Split `workflow.rs` (4,666 lines)

**Problem**: `workflow.rs` is 4,666 lines — 5.7% of the entire codebase in one file. ~3,500 lines are static instruction text constants.

**Proposal**: Extract instruction text into a separate data module:
```
mcp/tools/workflow/
├── mod.rs              # Step dispatch logic (~1,000 lines)
├── instructions.rs     # All DEFAULT_*_INSTRUCTION constants
├── variants.rs         # VARIANT_* constants for resolve
└── helpers.rs          # subagent_fanout_hint, resolve_repo_path
```

This separates data (instruction text) from logic (step dispatch), making both easier to maintain.

### 5.2 High: Extract Shared Service Layer

**Problem**: `web/api/` imports directly from `mcp/tools/` for shared operations (answer processing, entity retrieval, review queue). This couples the web layer to MCP transport concerns.

**Proposal**: Extract shared business logic into a `services/` module:
```
src/services/
├── mod.rs
├── review.rs    # get_review_queue, answer_questions (shared by MCP + web)
├── entity.rs    # get_entity, list_entities (shared by MCP + web)
└── search.rs    # search_knowledge, search_content (shared by MCP + web)
```

MCP tools and web API endpoints would both call into services. This is a larger refactor but would clean up the dependency graph.

### 5.3 Medium: Split `processor/review.rs` (1,911 lines)

**Problem**: Handles parsing, callout format conversion, question appending, deduplication, normalization, and stripping — too many responsibilities.

**Proposal**:
```
processor/review/
├── mod.rs          # Re-exports
├── parse.rs        # parse_review_queue, parse individual questions
├── callout.rs      # is_callout_review, unwrap/wrap_review_callout
├── append.rs       # append_review_questions, merge_duplicate_review_sections
├── normalize.rs    # normalize_review_section, strip_answered_questions
└── prune.rs        # prune_stale_questions
```

### 5.4 Medium: Consolidate Setup Functions

**Problem**: `commands/setup.rs` has 5 setup functions with overlapping functionality. Callers must choose the right one from a table.

**Proposal**: Replace with a builder pattern:
```rust
let ctx = SetupContext::new()
    .with_config()       // loads config
    .with_database()     // opens DB
    .require_repo(id)    // resolves repo, fails if not found
    .build()?;
// ctx.config, ctx.db, ctx.repo all available
```

### 5.5 Low: Wire or Gate `organize/audit.rs`

**Problem**: 540 lines of audit logging code that's built but never called.

**Options**:
1. Wire it into organize execute operations (merge, split, move, retype) — the original intent
2. Gate behind `#[cfg(feature = "audit")]` to avoid dead code in the binary
3. Remove it if audit logging is no longer planned

### 5.6 Low: Consolidate `commands/utils.rs`

**Problem**: `utils.rs` (445 lines) is a grab bag of error helpers, path validation, output formatting, repository resolution, and date parsing.

**Proposal**: Split into focused files:
- Error helpers → `commands/errors.rs` (the comment says "merged from errors.rs" — it was already split once)
- Path validation → `commands/paths.rs` (same — "merged from paths.rs")
- Output formatting → already in `output.rs` at the lib level; remove duplication
- Repository resolution → `commands/setup.rs` (already partially there)

---

## 6. Test Coverage Gaps

### 6.1 Files >100 Lines Without Tests

| File | Lines | Risk |
|------|------:|------|
| `scanner/orchestration/mod.rs` | 726 | **High** — Core scan orchestration. Complex multi-phase logic. |
| `commands/scan/mod.rs` | 397 | **Medium** — CLI scan entry point. Integration-tested but no unit tests. |
| `scanner/orchestration/links.rs` | 219 | **Medium** — Link detection phase. |
| `scanner/orchestration/embedding.rs` | 210 | **Medium** — Embedding phase. |
| `commands/check/execute/review.rs` | 190 | **Medium** — Review question generation orchestration. |
| `commands/scan/prune.rs` | 171 | **Medium** — Orphan entry removal. |
| `commands/grep/execute.rs` | 166 | **Low** — Grep execution. |
| `commands/check/watch.rs` | 131 | **Low** — Watch mode for check. |

The scanner orchestration module (`scanner/orchestration/mod.rs`, 726 lines) is the most critical gap. It contains `full_scan` and `run_scan` — the core indexing pipeline. This code is exercised by integration tests (which require an inference backend) but has no unit tests for its internal logic (document diffing, hash comparison, phase coordination).

### 6.2 Test Quality Observations

**Strengths**:
- 86% of files (206/240) have test modules
- Test helpers exist in `embedding.rs` (MockEmbedding, HashEmbedding), `organize/test_helpers.rs`, and `commands/test_helpers.rs`
- Tests use `tempfile::TempDir` consistently for filesystem isolation
- Integration tests are properly `#[ignore]`-gated

**Weaknesses**:
- `answer_processor/interpret.rs` (1,198 lines) tests are thorough for classification but don't test edge cases around the many string-matching heuristics (e.g., what happens with Unicode in answer text?)
- `mcp/tools/workflow.rs` tests primarily verify instruction text content rather than step dispatch logic
- No property-based testing for the regex-heavy `patterns.rs` (1,147 lines)

### 6.3 Coverage Summary

The test coverage is strong overall. The main gap is the scanner orchestration pipeline, which is the most complex and error-prone code path. Adding unit tests for `full_scan`'s document diffing logic (which documents need re-indexing, which are unchanged) would be high-value.

---

## 7. Additional Observations

### 7.1 The `lib.rs` Re-export Problem

The 25 `pub use` blocks in `lib.rs` re-export ~120 symbols into a flat namespace. This was likely done for convenience when the codebase was smaller, but at 81K lines it creates a maintenance burden. Every new public function must be manually added to `lib.rs`.

**Recommendation**: Gradually migrate callers to use qualified paths (`factbase::processor::parse_review_queue`) instead of flat imports (`factbase::parse_review_queue`). Keep the re-exports for backward compatibility but stop adding new ones.

### 7.2 Instruction Text as Code

`workflow.rs` contains ~3,500 lines of instruction text as Rust `const` strings. These are effectively configuration — they tell agents what to do at each workflow step. Storing them as Rust constants means:
- Changing instruction text requires recompilation
- The text is hard to read/edit (escaped newlines, concatenation)
- No syntax highlighting for the markdown content within strings

The `config/workflows.rs` override system partially addresses this (users can override via config.yaml), but the defaults are still compiled in. This is acceptable for now but worth noting as a scaling concern.

### 7.3 Feature Flag Discipline

Feature flags are well-used:
- `bedrock` — Amazon Bedrock provider
- `local-embedding` — Local CPU embeddings
- `mcp` — MCP server
- `web` — Web UI
- `compression` — zstd compression
- `progress` — Progress bars

The `#[cfg(feature = "...")]` gates are consistently applied. No feature-gated code leaks into non-gated paths.

### 7.4 Codebase Health Metrics

| Metric | Value | Assessment |
|--------|-------|------------|
| Total lines | 81,204 | Large but manageable |
| Files | 240 | Well-distributed |
| Avg lines/file | 338 | Healthy |
| Median lines/file | ~250 | Healthy |
| Files >1000 lines | 11 | Needs attention |
| Files <50 lines | ~15 | Acceptable (mostly mod.rs) |
| Test coverage (files) | 86% | Good |
| TODO/FIXME count | 1 | Excellent |
| `#[allow(dead_code)]` | 10 | Needs cleanup |
| Duplicate functions | 2 | Minor |

---

## 8. Prioritized Refactoring Roadmap

### Tier 1 — High Impact, Low Risk
1. **Split `workflow.rs`** into instruction data + dispatch logic (~2 hours)
2. **Remove `FactbaseError::Llm`** variant — unused since Phase 6 (~15 min)
3. **Deduplicate `get_str_array_arg`** — use helpers version in links.rs (~15 min)
4. **Remove `clean_canonicalize` wrapper** in commands/setup.rs (~5 min)
5. **Gate or wire `organize/audit.rs`** — 540 lines of dead code (~30 min)

### Tier 2 — Medium Impact, Medium Risk
6. **Split `processor/review.rs`** into focused submodules (~2 hours)
7. **Consolidate setup functions** into a builder pattern (~3 hours)
8. **Add unit tests for scanner orchestration** (~4 hours)
9. **Move legacy workflow constants** to `#[cfg(test)]` (~30 min)

### Tier 3 — High Impact, High Risk
10. **Extract shared service layer** from MCP tools for web API (~8 hours)
11. **Migrate lib.rs re-exports** to qualified paths (~4 hours, spread over time)
12. **Restructure `commands/utils.rs`** back into focused files (~2 hours)

---

## Appendix: File Size Distribution

```
Lines    Count   Description
------   -----   -----------
>4000      1     workflow.rs (outlier)
1000-4000  10    Large modules (review, document, apply, interpret, etc.)
500-999    25    Medium modules
200-499    75    Standard modules
100-199    60    Small modules
<100       69    Tiny modules (mod.rs, args, re-exports)
```
