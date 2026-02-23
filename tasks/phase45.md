# Phase 45: Cross-Document Fact Validation

Spec: [tasks/cross-conflict.md](cross-conflict.md)

Reference the spec for the full design: per-fact semantic search, LLM-based conflict/staleness detection, batched prompts, and re-check tracking.

## Task 1: Fact extraction expansion

- [x] 1.1 Create `src/question_generator/facts.rs` with `extract_all_facts(content: &str) -> Vec<FactLine>` that extracts ALL list items (any indentation level), not just temporally-tagged ones. `FactLine` should include: line_number, text (cleaned of markdown bullets/checkboxes), section heading (if under an `## H2`). Reuse the section-tracking pattern from `collect_facts_with_ranges` in `conflict.rs`.
- [x] 1.2 Add unit tests for `extract_all_facts`: plain list items, nested items, items with temporal tags, items without, items under different section headings, non-list lines excluded.

## Task 2: Per-fact semantic search

- [x] 2.1 Create `src/question_generator/cross_validate.rs` with the async function signature: `async fn cross_validate_document(content: &str, doc_id: &str, db: &Database, embedding: &dyn EmbeddingProvider, llm: &dyn LlmProvider) -> Result<Vec<ReviewQuestion>>`. Start with the skeleton that calls `extract_all_facts` from Task 1.1 and iterates over facts. Register the module in `src/question_generator/mod.rs`.
- [x] 2.2 For each fact from 2.1, generate an embedding using the fact text via `embedding.generate()`, then call `db.search_semantic()` with that embedding. Filter results to exclude the source document (`doc_id`). Collect top 10 results per fact. This is the same search path used by `search_knowledge` in the MCP tools — look at `src/mcp/tools/search.rs` for the pattern.
- [x] 2.3 Add relevance filtering: skip search results with low similarity scores (threshold TBD, start with 0.3). Skip facts that return zero relevant results — no cross-check needed if nothing in the factbase is related. This reduces LLM calls significantly for facts about unique/new topics.

## Task 3: LLM conflict detection

- [x] 3.1 Build the LLM prompt template. For each batch of facts (5-10 per call), format: the source document name, each fact with its line number, and for each fact its top search results with source document IDs and relevant text snippets. The prompt asks the LLM to classify each fact as CONSISTENT, CONFLICT, STALE, or UNCERTAIN. See the spec for the exact prompt format. Put the prompt template in `cross_validate.rs`.
- [x] 3.2 Implement the batching logic in `cross_validate_document`: group facts into batches of up to 10, build the prompt for each batch using 3.1, call `llm.complete()` (same pattern as `src/llm/review.rs`), parse the JSON response. Handle LLM response parsing errors gracefully — log and skip malformed responses rather than failing the whole document.
- [x] 3.3 Convert LLM results to `ReviewQuestion` instances. For CONFLICT results, generate `@q[conflict]` with description citing the source document and conflicting fact. For STALE results, generate `@q[stale]` with description citing the source document and evidence of staleness. CONSISTENT and UNCERTAIN produce no questions. Include the cross-document source in the question description so the reviewer knows where to look.

## Task 4: Integration into lint

- [x] 4.1 Wire `cross_validate_document` into `src/commands/lint/review.rs`. This runs as a separate pass AFTER existing question generators (temporal, conflict, stale, missing, ambiguous, duplicate). It needs `&Database`, `&dyn EmbeddingProvider`, and `&dyn LlmProvider` — the lint command already has access to the database; embedding and LLM providers need to be set up (same as scan command). Add a `--cross-check` flag to `lint` to enable this pass (expensive, opt-in for now).
- [x] 4.2 The lint command is currently sync. `cross_validate_document` is async (embedding generation + LLM calls). Use `tokio::runtime::Runtime::new()` to run the async function from the sync lint context, same pattern as `cmd_scan` uses for async operations. Or if lint already runs in a tokio context, use `block_on`.
- [x] 4.3 Add progress output for cross-validation: "Cross-checking document X of Y..." since this pass is slow. Use the existing progress bar pattern from scan if `progress` feature is enabled, or simple stderr prints otherwise.

## Task 5: Re-check tracking

- [x] 5.1 Add a `cross_check_hash` column to the documents table (nullable TEXT). Store the SHA256 of the document content at the time of last cross-validation. On lint `--cross-check`, skip documents where `cross_check_hash` matches current content hash. Schema migration in `src/database/schema.rs`.
- [x] 5.2 After successfully cross-validating a document, update its `cross_check_hash` in the database. This means subsequent `lint --cross-check` runs skip unchanged documents.
- [x] 5.3 When a document changes (detected during scan via existing hash comparison), also clear the `cross_check_hash` of documents that link TO it (via the link graph). This ensures that if Jane's person doc changes, company docs mentioning Jane get re-cross-checked. Use `db.get_links_to(doc_id)` to find affected documents.

## Task 6: MCP and workflow integration

- [x] 6.1 Add cross-validation to the `generate_questions` MCP tool — when called for a specific document, also run cross-validation (not just within-document generators). This makes it available to agents without needing the CLI.
- [x] 6.2 Update the `resolve` workflow in `src/mcp/tools/workflow.rs` to mention cross-document conflicts in its instructions. The workflow already handles review queue resolution — cross-conflict questions will appear in the queue naturally once generated.

## Outcomes

### Task 1.1 + 1.2 — Fact extraction expansion (commit d24731d)

**Summary**: Created `src/question_generator/facts.rs` with `FactLine` struct and `extract_all_facts()` function. Registered module in `question_generator/mod.rs`. Included 21 unit tests covering all specified scenarios.

**Key considerations**:
- `clean_fact_text()` intentionally does NOT truncate (unlike existing `extract_fact_text()` which caps at 80 chars) because the full text is needed for embedding generation in cross-validation.
- Checkbox stripping (`[ ]`, `[x]`, `[X]`) is new — not present in existing `extract_fact_text()`.
- Section tracking reuses the same `## H2` pattern from `collect_facts_with_ranges` in `conflict.rs`.
- Module is `pub(crate)` so `cross_validate.rs` (Task 2) can import `FactLine` and `extract_all_facts`.

**Difficulties**: None — clean implementation following established patterns. All 890 lib tests + 347 bin tests pass.

### Task 2.1 + 2.2 + 2.3 — Per-fact semantic search (commit 2696326)

**Summary**: Created `src/question_generator/cross_validate.rs` with `cross_validate_document` async function. Registered `cross_validate` module in `question_generator/mod.rs`. The function extracts all facts, generates per-fact embeddings, searches for related documents (top 10, excluding source), filters by relevance threshold (0.3), and collects results into `FactWithContext` structs for Task 3's LLM prompt building.

**Key considerations**:
- `FactWithContext` struct holds `FactLine` + `Vec<SearchResult>` — ready for Task 3 to batch into LLM prompts.
- `RELEVANCE_THRESHOLD = 0.3` filters out low-similarity results before LLM calls. Facts with zero relevant results after filtering are skipped entirely.
- Uses `search_semantic_paginated` (same as MCP `search_knowledge` tool) rather than a simpler search method, for consistency.
- `_llm` parameter is unused for now (prefixed with underscore) — Task 3 will consume it.
- `#[allow(dead_code)]` on `FactWithContext` fields since they're consumed by Task 3.
- 3 unit tests with mock embedding/LLM providers verify empty content, no-list-items, and facts-without-LLM paths.

**Difficulties**: None. All 893 lib tests + 347 bin tests pass.

### Task 3.1 + 3.2 + 3.3 — LLM conflict detection (commit 18db85c)

**Summary**: Implemented the full LLM conflict detection pipeline in `cross_validate.rs`: prompt template (`build_prompt`), batching logic (10 facts per call), JSON response parsing (`parse_llm_response` with markdown fence stripping), and `ReviewQuestion` generation (`result_to_question` for CONFLICT/STALE results). Added `CrossCheckResult` serde struct for LLM response deserialization. 15 unit tests covering all components.

**Key considerations**:
- `build_prompt()` truncates search result snippets to 200 chars to prevent prompt bloat with large documents.
- `parse_llm_response()` strips markdown code fences (`\`\`\`json ... \`\`\``) since LLMs commonly wrap JSON in fences.
- LLM call failures are logged and skipped (per batch) rather than failing the entire document — graceful degradation.
- `result_to_question()` uses `checked_sub(1)` for 1-based fact index safety, returning `None` for index 0 or out-of-bounds.
- `extract_title()` extracts from first `# ` heading, falling back to doc_id — used in prompt for human-readable context.
- Removed `_llm` underscore prefix and `#[allow(dead_code)]` from Task 2 since fields are now consumed.
- Question descriptions include cross-document source citation: "Cross-check with {source_doc}: {fact} — {reason}".

**Difficulties**: None. All 905 lib tests + 347 bin tests pass (12 new tests added).

### Task 4.1 + 4.2 + 4.3 — Integration into lint (commit 41aac04)**Summary**: Wired `cross_validate_document` into `cmd_lint` as a separate pass after existing checks. Added `--cross-check` flag to `LintArgs`. Made `cross_validate_document` public and re-exported from `lib.rs`. Set up embedding + LLM providers when flag is used. Progress output via stderr `eprint!` for inline updates.

**Key considerations**:
- `cmd_lint` is already `async fn` — no need for `Runtime::new()` or `block_on`. Task 4.2's concern was moot.
- Cross-check runs as a separate pass AFTER the batch loop (not inside it) for cleaner separation from existing checks.
- Wired into `mod.rs` directly rather than `review.rs` since it needs async and the embedding/LLM providers, which are set up at the `cmd_lint` level.
- Graceful degradation: individual document cross-check failures are logged as warnings, not fatal errors.
- Respects `--dry-run`: when set, questions are printed but not written to files.
- Progress uses `eprint!`/`eprintln!` with `\r` carriage return for inline updates (simpler than progress bar for a sequential async pass).
- Questions appended via `append_review_questions` (same as existing review path).

**Difficulties**: None. All 966 lib tests (all-features) + 354 bin tests (all-features) pass.

### Task 5.1 — Schema migration and skip logic (commit 21171e1)

**Summary**: Added `cross_check_hash` nullable TEXT column to the documents table via schema migration v5. Added three database methods: `needs_cross_check()` compares `file_hash` vs `cross_check_hash`, `set_cross_check_hash()` copies current `file_hash` to `cross_check_hash`, and `clear_cross_check_hashes()` nulls the column for a list of IDs. Modified lint `--cross-check` loop to skip documents where hashes match, with a count of skipped documents in output.

**Key considerations**:
- `cross_check_hash` stores the `file_hash` value (not a separate SHA256 computation) — comparing `cross_check_hash == file_hash` is sufficient since `file_hash` already tracks content changes.
- `needs_cross_check()` returns `true` when no hash is stored (new documents) or when hashes differ (changed documents).
- `set_cross_check_hash()` uses `SET cross_check_hash = file_hash` in a single UPDATE — no need to read and write separately.
- `clear_cross_check_hashes()` accepts a slice of IDs for batch invalidation (used by Task 5.3 for linked documents).
- `upsert_document()` does INSERT OR REPLACE which resets `cross_check_hash` to NULL for changed documents (since the column isn't in the INSERT column list). This is correct behavior — changed documents need re-checking.
- 6 new tests covering column existence, all three methods, and edge cases.

**Difficulties**: None. All 972 lib tests (all-features) + 354 bin tests (all-features) pass.

### Task 5.2 — Update hash after validation (commit d8bd664)

**Summary**: Added `db.set_cross_check_hash(&doc.id)?` call in the `Ok(questions)` arm of the cross-check loop in `src/commands/lint/mod.rs`. The call runs after successful cross-validation regardless of whether questions were generated — the point is that the document was checked.

**Key considerations**:
- Respects `--dry-run`: hash is only updated when `!args.dry_run`, so users can preview cross-check results without marking documents as checked. A subsequent non-dry-run run will re-check and persist state.
- Placed outside the `!questions.is_empty()` block — documents with zero conflicts still get marked as checked, which is correct (they were successfully validated).
- Failed cross-validations (the `Err(e)` arm) do NOT update the hash, so those documents will be retried on the next run.
- Only 4 lines of code added — the infrastructure from Task 5.1 (`set_cross_check_hash`, `needs_cross_check`) did all the heavy lifting.

**Difficulties**: None. All 972 lib tests (all-features) + 354 bin tests (all-features) pass.

### Task 5.3 — Linked document invalidation on change (commit d899ec5)

**Summary**: Added cross-check hash invalidation in `full_scan` (scanner/orchestration/mod.rs). After committing document changes, iterates over `changed_ids`, calls `db.get_links_to()` for each to find referencing documents, and calls `db.clear_cross_check_hashes()` on the collected source IDs. Added one integration-style test in `database/documents.rs` verifying the full pattern.

**Key considerations**:
- Placed after `commit_transaction()` so upserted documents are visible, but before duplicate check and link detection.
- Excludes documents already in `changed_ids` from invalidation — they already have their `cross_check_hash` reset to NULL by `upsert_document`'s INSERT OR REPLACE.
- For new documents, `get_links_to` returns empty (no links exist yet), so the iteration is harmless.
- For moved-only documents (content unchanged), clearing linked docs' hashes is conservative but correct — a few unnecessary re-checks are better than stale results.
- Uses `info!` tracing to log how many linked documents were invalidated.
- Only runs when `!opts.dry_run` (inside the existing dry-run guard block).

**Difficulties**: None. All 912 lib tests + 347 bin tests pass.

### Task 6.1 + 6.2 — MCP and workflow integration (commit 27b2f7b)

**Summary**: Threaded `LlmProvider` through the MCP infrastructure so `generate_questions` can run cross-document fact validation. Updated the resolve workflow to guide agents on handling cross-document conflicts.

**Key considerations**:
- `AppState` gains `llm: Option<Box<dyn LlmProvider>>` — optional because tests use `None`.
- `McpServer::new` takes an additional `llm: Option<Box<dyn LlmProvider>>` parameter (last position).
- `handle_tool_call` gains `llm: Option<&dyn LlmProvider>` parameter, threaded from both HTTP and stdio transports.
- `generate_questions` changed from sync to async — dispatch changed from `blocking_tool!` macro to direct async call with `embedding` and `llm` parameters.
- Cross-validation runs after all sync generators (temporal, conflict, missing, ambiguous, stale, duplicate) and only when LLM is available (`if let Some(llm)`).
- Graceful degradation: cross-validation failures are logged as warnings, not fatal errors.
- Both `cmd_serve` and `cmd_mcp` create an LLM provider via `setup_llm_with_timeout` and pass it through.
- Resolve workflow step 2 now explains cross-document conflicts and instructs agents to use `get_entity` to read the referenced source document.
- 7 files changed, 50 insertions, 13 deletions — minimal footprint.

**Difficulties**: None. All 973 lib tests (all-features) + 354 bin tests (all-features) pass.
