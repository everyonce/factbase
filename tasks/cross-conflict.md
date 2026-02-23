# Cross-Document Fact Validation

## Problem

Factbase's conflict detection is within-document only. It compares facts inside a single markdown file for overlapping temporal ranges. It cannot detect:

1. **Stale entries**: `companies/acme.md` lists Jane Smith as current VP, but `people/jane-smith.md` says she left Acme in 2024
2. **Cross-document contradictions**: `people/bob.md` says "Jane joined Globex in 2025" but `companies/acme.md` still lists her as active
3. **Outdated roster entries**: Person appears under two different company docs, both claiming them as current — the old entry was never cleaned up

The existing within-document conflict detector catches "two job titles with overlapping dates in the same file." It cannot catch "this fact is inconsistent with what the rest of the factbase knows."

## Design

### Core Concept

Every fact line in a document gets validated against the rest of the factbase using semantic search + LLM judgment. This is conceptually different from within-document analysis — it's external validation.

For each fact:
1. Search the factbase semantically using the full fact text
2. Collect top results (excluding the source document)
3. Ask the LLM: "Is this fact still current and consistent with what the factbase knows about these entities?"

The LLM prompt is deliberately broader than "does this conflict?" — it also catches stale entries where no individual fact contradicts, but the entity has moved on.

### Pipeline

```
Document under review
  │
  ├─ Extract all fact lines (list items, any indentation level)
  │
  ├─ For each fact (or batch of facts):
  │    ├─ Generate embedding for the fact text
  │    ├─ Semantic search against factbase (top 10, exclude self)
  │    ├─ Filter results to those sharing entity mentions
  │    └─ Bundle fact + search results into LLM prompt
  │
  ├─ LLM returns: consistent / conflict / stale / uncertain
  │    ├─ conflict → @q[cross-conflict] with cited source
  │    ├─ stale → @q[stale] with cited source (or new @q[cross-stale])
  │    └─ consistent / uncertain → no question
  │
  └─ Append generated questions to document's review queue
```

### Trigger

Runs during `factbase lint --review`, as a separate pass after existing question generators. Only processes documents that have changed since last cross-check (SHA256 hash tracking, same as scan).

Initial pass: all documents. Incremental: only changed files. A document is also re-checked when documents it references change (via link graph — if Jane's person doc changes, company docs linking to Jane get re-queued).

### Fact Extraction

Expand beyond the current `collect_facts_with_ranges` which only grabs list items with Range/Ongoing temporal tags. For cross-validation, extract ALL list items:

```markdown
- VP Engineering at Acme Corp @t[2020..]        ← has temporal tag (already extracted)
- Based in Seattle                                ← no temporal tag (NEW: now extracted)
- Leads the platform team of 12 engineers         ← no temporal tag (NEW: now extracted)
```

Every statement is a fact. Every fact should be validatable.

### Semantic Search Strategy

Search using the full fact text as the query (not per-entity). This captures the combination of entities in context:

```
Query: "Jane Smith VP Engineering at Acme Corp"
→ Returns: people/jane-smith.md, companies/acme.md (other sections), meetings/q4-review.md, etc.
```

One search per fact. Top 10 results, excluding the source document. The embedding model handles the semantic matching — facts about Jane Smith at Acme will surface regardless of exact wording.

### LLM Prompt

Batch multiple facts per LLM call (5-10 facts with their search results) to reduce call count.

```
You are validating facts from a knowledge base document. For each fact below,
I've included relevant information from other documents in the knowledge base.

Determine if each fact is:
- CONSISTENT: agrees with or is not contradicted by other sources
- CONFLICT: directly contradicts information in another document
- STALE: may have been true but other sources suggest it's no longer current
- UNCERTAIN: insufficient information to validate

For CONFLICT and STALE, cite the specific document and fact that disagrees.

Document: companies/acme.md
---
Fact 1 (line 15): "Jane Smith - VP Engineering @t[2020..]"
Related information:
- [people/jane-smith.md] "Left Acme Corp to join Globex Inc @t[2024-06]"
- [people/jane-smith.md] "VP Engineering at Globex Inc @t[2024-06..]"

Fact 2 (line 16): "Based in Seattle, WA"
Related information:
- [people/jane-smith.md] "Relocated to Austin, TX @t[2024-08]"
---

Respond in JSON:
[
  {"fact": 1, "status": "STALE", "reason": "Jane Smith left Acme in 2024-06 per people/jane-smith.md", "source_doc": "jane-smith", "source_line": "Left Acme Corp to join Globex Inc"},
  {"fact": 2, "status": "CONFLICT", "reason": "Jane relocated to Austin per people/jane-smith.md", "source_doc": "jane-smith", "source_line": "Relocated to Austin, TX"}
]
```

### Question Types

Generate existing question types with cross-document context:

- `@q[conflict]` — "This fact conflicts with [source_doc]: [cited fact]. Which is correct?"
- `@q[stale]` — "This fact may be outdated based on [source_doc]: [cited fact]. Is it still current?"

No new question type needed — the existing types work, just with richer descriptions that cite the cross-document source.

### Architecture

This is a separate pass in the lint command, not part of the existing pure-function question generators. It requires infrastructure access:

```rust
// Existing generators — pure functions, no dependencies
generate_conflict_questions(content: &str) -> Vec<ReviewQuestion>
generate_stale_questions(content: &str, stale_days: u32) -> Vec<ReviewQuestion>

// New cross-validation — needs DB, embedding, LLM
async fn cross_validate_document(
    content: &str,
    doc_id: &str,
    db: &Database,
    embedding: &dyn EmbeddingProvider,
    llm: &dyn LlmProvider,
) -> Result<Vec<ReviewQuestion>>
```

New file: `src/question_generator/cross_validate.rs`

Integration point: `src/commands/lint/review.rs` — add cross-validation pass after existing question generation, gated behind a flag (`--cross-check` or `--thorough`) since it's expensive.

### Performance

Assumptions: 200 documents, 15 facts each, ~3,000 total facts.

| Operation | Per fact | Total (initial) | Total (incremental, 3 docs) |
|-----------|----------|-----------------|----------------------------|
| Embedding generation | ~50ms | ~2.5 min | ~2s |
| Semantic search (sqlite-vec) | ~5ms | ~15s | ~0.2s |
| LLM call (batched 10/call) | ~1s per batch | ~5 min | ~5s |
| **Total** | | **~8 min** | **~8s** |

Cost at Haiku rates: ~$0.50-1.00 for initial pass, pennies for incremental.

### Re-check Triggers

A document needs cross-validation when:
1. The document itself changes (file hash changed)
2. A document it links to changes (link graph lookup — "what documents link to entities that changed?")

Store last-cross-checked hash per document in the database. Compare on lint run.

## Implementation Order

1. Fact extraction expansion (all list items, not just temporally-tagged)
2. Per-fact semantic search
3. LLM prompt + response parsing
4. Question generation with cross-document citations
5. Integration into lint command
6. Re-check tracking (hash-based skip)
7. Link-graph-based re-queue (documents affected by changes to linked entities)

## Future: Merge Detection for Duplicate Entities

Related but separate concern: the organize module's merge detection (`organize/detect/merge.rs`) currently finds duplicate *documents* (two files about the same entity). It should also detect duplicate *entries within documents* — the same person listed under two different company docs.

This is noted here for future work. The cross-validation system will surface these as stale/conflict questions, but a dedicated "this person appears in multiple company rosters" detection would be more direct. See `organize/detect/merge.rs` for the existing document-level merge detection to extend.
