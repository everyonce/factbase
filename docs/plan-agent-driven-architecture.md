# Plan: Agent-Driven Architecture

**Status:** Planned
**Goal:** Remove all server-side LLM usage. Factbase becomes an embedding engine + index + API. All reasoning moves to the client agent via MCP.

## Motivation

Factbase currently runs a server-side LLM (Haiku via Bedrock or Ollama) for link detection, cross-document validation, review application, organize planning, and entity discovery. This creates problems:

- **Double LLM cost** — MCP users pay for the client agent AND the server-side LLM
- **Weaker reasoning** — the server-side model (Haiku) is weaker than the client agent (Claude, GPT-4, etc.)
- **Paging complexity** — LLM-heavy operations use time-budgeted resume tokens, causing agent confusion and workflow failures
- **Infrastructure burden** — users must configure LLM credentials even when an agent is already present
- **Redundant work** — `apply_review_answers` uses a second LLM to rewrite content the agent already reasoned about

## Architecture After

```
factbase = embedding engine + SQLite index + MCP API + web GUI
agent    = all reasoning, all LLM work
human    = deferred questions via web GUI
```

Factbase retains the embedding model (required for indexing and search). The LLM is eliminated entirely.

## Design Decisions

### Link detection
- Scan uses string matching only (regex `[[id]]` + fuzzy pre-filter: full title, unique words, abbreviations)
- New `get_link_suggestions` tool surfaces embedding-similar but unlinked document pairs
- New `store_links` tool writes `[[id]]` references into files + updates DB
- Agent reviews compact suggestion list and confirms links — minimal context cost

### Link file format
Links are document-level metadata, stored at the bottom of the file as a peer of footnotes:

```markdown
---
[^1]: Source one
[^2]: Source two

Links: [[abc123]] [[def456]] [[ghi789]]
```

### Cross-document validation
- New `get_fact_pairs` tool returns embedding-similar fact pairs with similarity scores
- Agent classifies pairs (contradicting/superseding/consistent) and creates review questions
- No server-side LLM classification

### Review application
- Remove `apply_review_answers` MCP tool
- Agent rewrites documents directly via `update_document` after reasoning about answers
- Web GUI uses rule-based mechanical application for simple cases (insert `@t[...]` tag, add footnote)

### Organize merge/split
- Remove LLM planning from merge and split
- Agent reads both documents via `get_entity`, plans the merge/split, executes via CRUD tools
- `organize` tool keeps mechanical operations: move, retype, apply

### Workflow simplification
- Most paging/resume infrastructure removed (only scan still needs it for embedding generation)
- Workflow prompts shrink dramatically — no more paging warnings
- Data-retrieval model: "here's the data, you process it" replaces "keep calling until done"

---

## Phase 1: String-Only Link Detection

Remove the LLM from link detection during scan. The fuzzy string pre-filter already catches most links.

### Tasks

- [ ] Remove LLM call from `LinkDetector::detect_links` — keep regex + `string_match_links` only
- [ ] Remove LLM call from `LinkDetector::detect_links_batch` — same
- [ ] Make `LinkDetector` not require `Box<dyn LlmProvider>` (or replace with standalone functions)
- [ ] Update scanner orchestration (`orchestration/links.rs`) to not require LlmProvider
- [ ] Update `ScanContext` to make LlmProvider optional
- [ ] Update `factbase scan` CLI to not construct LlmProvider for link detection
- [ ] Update `scan_repository` MCP tool to not require LlmProvider for link detection
- [ ] Update tests in `link_detector.rs` (remove MockLlm from string-match-only tests)
- [ ] Update integration tests that expect LLM link detection
- [ ] Verify: `cargo test --lib` passes, `factbase scan` works without LLM config

### What still works after this phase
- All existing links detected by string matching continue to work
- Manual `[[id]]` links still detected
- LLM is still used for other features (cross-validate, organize, etc.)

### What's lost
- Indirect/implicit entity references ("the parent company" → Unrelated Corp) no longer detected during scan
- Recovered in Phase 2 via agent-driven link suggestions

---

## Phase 2: Link Suggestion Tools + File Format

Add the agent-driven link discovery pipeline and the `Links:` file format.

### Tasks

- [ ] Add `Links:` block parsing to document processor — recognize `Links: [[id1]] [[id2]]` at document bottom
- [ ] Update scan to read existing `Links:` blocks and store them as links in DB
- [ ] New MCP tool: `get_link_suggestions`
  - Input: `repo` (optional), `min_similarity` (default 0.6), `max_existing_links` (default 2), `limit` (default 50)
  - Process: for documents with few links, find embedding-similar documents not yet linked
  - Output: `{suggestions: [{doc_id, doc_title, link_count, candidates: [{id, title, similarity}]}]}`
- [ ] New MCP tool: `store_links`
  - Input: `links: [{source_id, target_id}]`
  - Process: group by source, read each file, append new `[[id]]` to `Links:` block (create if missing), write file, update DB
  - Output: `{added, skipped_existing, documents_modified}`
- [ ] Add tool schemas to `schema.rs`
- [ ] Update authoring guide and agent authoring guide with `Links:` format
- [ ] Update example documents (`person.md`, `company.md`) with `Links:` blocks
- [ ] Add unit tests for `Links:` parsing and `store_links` file modification
- [ ] Update workflow prompts: add link review step to `update` workflow

---

## Phase 3: Fact Pairs Tool + Cross-Validation Simplification

Replace server-side LLM classification of fact pairs with a data-retrieval tool.

### Tasks

- [ ] New MCP tool: `get_fact_pairs`
  - Input: `repo` (optional), `min_similarity` (default from config), `limit` (default 50)
  - Process: query pre-computed fact embeddings for similar pairs across documents, exclude already-reviewed pairs
  - Output: `{pairs: [{fact_a: {doc_id, doc_title, text, line}, fact_b: {doc_id, doc_title, text, line}, similarity}]}`
- [ ] Remove LLM classification from `cross_validate.rs` (both call sites)
- [ ] Remove `cross_validate` mode from `check_repository` (or make it just call `get_fact_pairs` internally)
- [ ] Update `update` workflow: replace cross-validate paging loop with "call get_fact_pairs, classify each pair, flag conflicts via answer_questions"
- [ ] Add tool schema
- [ ] Update tests

---

## Phase 4: Remove All Remaining Server-Side LLM Usage

Eliminate every remaining `llm.complete()` call site.

### Tasks

#### Review application (`answer_processor/apply.rs`)
- [ ] Remove `apply_review_answers` MCP tool from tool routing and schema
- [ ] Update resolve workflow: agent rewrites documents via `update_document` instead of calling apply
- [ ] Keep rule-based application logic for web GUI (mechanical: insert @t tag, add footnote) — extract into separate module if needed

#### Inbox processing (`answer_processor/inbox.rs`)
- [ ] Remove LLM call from inbox processing
- [ ] Update ingest workflow: agent handles inbox merging via `update_document`

#### Organize planning (`organize/plan/merge.rs`, `organize/plan/split.rs`)
- [ ] Remove LLM calls from merge and split planning
- [ ] Remove merge/split modes from `organize` MCP tool (or make them return raw data for agent to plan)
- [ ] Update organize workflow: agent reads docs via `get_entity`, plans merge/split, executes via CRUD tools
- [ ] Keep `organize_analyze` (heuristic detection, no LLM)
- [ ] Keep `organize` move/retype/apply modes (mechanical, no LLM)

#### Entity discovery (`organize/detect/entity_discovery.rs`)
- [ ] Remove LLM calls from entity discovery (both call sites)
- [ ] Make entity discovery heuristic-only (co-occurrence analysis, title pattern matching) or move entirely to workflow
- [ ] Remove `discover` mode from `check_repository`

#### Vocabulary extraction (`question_generator/check.rs`)
- [ ] Remove LLM call for domain vocabulary extraction
- [ ] Move to workflow step: agent extracts vocabulary during update/discover

#### Acronym auto-resolution (`mcp/tools/workflow.rs`)
- [ ] Remove LLM call for glossary acronym resolution
- [ ] Agent handles during resolve workflow

### Verification
- [ ] `grep -r '\.complete(' src/` returns zero results (excluding test helpers and trait definitions)
- [ ] All MCP tools work without LLM configuration
- [ ] `cargo test --lib` passes

---

## Phase 5: Workflow and Tool Surface Simplification

Rewrite workflows for the data-retrieval model. Clean up tool surface.

### Tasks

#### Tool consolidation
- [ ] Remove `generate_questions` tool — fold into `check_repository` with optional `doc_id` parameter
- [ ] Simplify `check_repository` — single mode (rule-based quality checks), remove mode parameter
- [ ] Simplify `organize` — keep move/retype/apply only, remove merge/split modes
- [ ] Remove paging/resume infrastructure from tools that no longer need it (check, organize_analyze, discover)

#### Workflow rewrite
- [ ] Rewrite `update` workflow:
  1. scan (paged for embeddings only)
  2. check quality (one call, rule-based)
  3. review link suggestions (get_link_suggestions → confirm → store_links)
  4. review fact pairs (get_fact_pairs → classify → answer_questions)
  5. organize analyze (one call, heuristic)
  6. summary
- [ ] Rewrite `resolve` workflow:
  1. get review queue
  2. agent answers questions (research + reasoning)
  3. agent rewrites documents via update_document
  4. verify
- [ ] Rewrite `improve` workflow — remove apply_review_answers step, agent rewrites directly
- [ ] Rewrite `ingest` workflow — remove inbox LLM processing reference
- [ ] Rewrite `enrich` workflow — simplify, remove paging references
- [ ] Rewrite `setup` workflow — remove fact embedding paging step
- [ ] Remove all "⚠️ PAGING" warning blocks from workflow prompts
- [ ] Update FORMAT_RULES constant if needed

#### Update tool schemas
- [ ] Update `schema.rs` to reflect removed/modified tools
- [ ] Update tool count in docs and comments

---

## Phase 6: LLM Infrastructure Removal + Documentation

Remove the LLM module and update all documentation.

### Tasks

#### Code removal
- [ ] Remove `src/llm/` module (mod.rs, ollama.rs, link_detector.rs, review.rs)
- [ ] Remove `LlmProvider` trait (or keep as dead code if needed for future extensibility — probably remove)
- [ ] Remove `llm` field from AppState / setup code
- [ ] Remove `llm:` config section (or make it fully optional/ignored with deprecation warning)
- [ ] Remove LLM-related config validation
- [ ] Update `factbase doctor` — only check embedding connectivity, not LLM
- [ ] Update `factbase serve` / `factbase mcp` — don't construct LlmProvider
- [ ] Clean up `ollama.rs` if only used by LLM (check if embedding uses it too)
- [ ] Remove LLM-related dependencies if no longer needed
- [ ] Evaluate `bedrock` feature flag — may become `bedrock-embedding` only

#### Documentation
- [ ] Update README.md — remove LLM config from examples, update prerequisites
- [ ] Update `docs/cli-reference.md`
- [ ] Update `docs/agent-integration.md`
- [ ] Update `docs/quickstart.md` — simpler setup without LLM
- [ ] Update `docs/inference-providers.md` — embedding only
- [ ] Update `docs/authoring-guide.md` — add `Links:` format
- [ ] Update `docs/agent-authoring-guide.md` — add `Links:` format
- [ ] Update `docs/review-system.md`
- [ ] Update `examples/config.yaml` — remove `llm:` section
- [ ] Update `examples/person.md`, `examples/company.md` — add `Links:` blocks
- [ ] Update `.kiro/steering/` docs (architecture, module-interactions, current-state, coding-conventions)
- [ ] Update CHANGELOG.md

#### Testing
- [ ] Remove or update all tests that use MockLlm for non-test-helper purposes
- [ ] Verify full test suite passes: `cargo test --lib`, `cargo test --bin factbase`
- [ ] Update integration tests
- [ ] Update web frontend tests if affected

---

## Summary

| Metric | Before | After |
|--------|--------|-------|
| Server-side LLM | Required (Haiku/Ollama) | None |
| LLM call sites | 9 across 7 files | 0 |
| MCP tools | 25 | ~23 |
| Paged/resumable operations | ~6 | 1 (scan only) |
| Config sections | database, embedding, llm, server, web | database, embedding, server, web |
| Workflow paging warnings | ~12 blocks | ~1 block |
| `llm.complete()` calls in codebase | 14 | 0 |
