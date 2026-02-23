# processor.rs Public API Audit

**File:** `src/processor.rs`  
**Lines:** 2966  
**Tests:** 158  
**Date:** 2026-01-31

## Public Items (22 total)

### Structs (5)

| Name | Line | Exported via lib.rs | Used By |
|------|------|---------------------|---------|
| `DocumentProcessor` | 17 | ✓ | scan.rs, serve.rs, mcp/tools/document.rs |
| `DocumentChunk` | 207 | ✗ (internal) | processor.rs only |
| `TemporalValidationError` | 445 | ✓ | lint.rs, import.rs |
| `TemporalSequenceError` | 602 | ✓ | lint.rs |
| `TemporalConflict` | 675 | ✓ | lint.rs |

### Functions (17)

#### Core Document Processing (DocumentProcessor methods)
| Method | Description | Used By |
|--------|-------------|---------|
| `new()` | Constructor | scan.rs, serve.rs |
| `compute_hash()` | SHA256 hash | scan.rs |
| `extract_id()` | Get ID from content | scanner.rs |
| `extract_id_static()` | Static version for parallel | scanner.rs |
| `generate_id()` | Random 6-char hex | scanner.rs |
| `is_id_unique()` | Check DB for uniqueness | scanner.rs |
| `generate_unique_id()` | Generate unique ID | scanner.rs |
| `inject_header()` | Add factbase header | scanner.rs |
| `extract_title()` | Parse H1 or filename | scanner.rs |
| `derive_type()` | Type from folder | scanner.rs |

#### Temporal Tag Functions
| Function | Line | Exported | Used By |
|----------|------|----------|---------|
| `parse_temporal_tags` | 110 | ✓ | lint.rs, search.rs, question_generator.rs, mcp/tools/entity.rs, mcp/tools/search.rs |
| `validate_date` | 457 | ✓ | (internal validation) |
| `validate_temporal_tags` | 569 | ✓ | lint.rs, import.rs |
| `detect_illogical_sequences` | 615 | ✓ | lint.rs |
| `detect_temporal_conflicts` | 690 | ✓ | lint.rs |
| `overlaps_point` | 811 | ✓ | search.rs, mcp/tools/search.rs |
| `overlaps_range` | 885 | ✓ | search.rs, mcp/tools/search.rs |
| `calculate_recency_boost` | 974 | ✓ | search.rs |

#### Source Reference Functions
| Function | Line | Exported | Used By |
|----------|------|----------|---------|
| `parse_source_references` | 283 | ✓ | lint.rs, search.rs, import.rs, question_generator.rs, mcp/tools/entity.rs |
| `parse_source_definitions` | 332 | ✓ | lint.rs, import.rs, question_generator.rs, mcp/tools/entity.rs |

#### Fact Statistics Functions
| Function | Line | Exported | Used By |
|----------|------|----------|---------|
| `count_facts` | 1012 | ✓ | (available but rarely used) |
| `count_facts_with_temporal_tags` | 1020 | ✓ | (available but rarely used) |
| `count_facts_with_sources` | 1028 | ✓ | database.rs, lint.rs, mcp/tools/entity.rs |
| `calculate_fact_stats` | 1037 | ✓ | cache.rs, lint.rs, mcp/tools/entity.rs |

#### Chunking Functions
| Function | Line | Exported | Used By |
|----------|------|----------|---------|
| `chunk_document` | 216 | ✓ | scanner.rs (embedding generation) |

#### Review Queue Functions
| Function | Line | Exported | Used By |
|----------|------|----------|---------|
| `parse_review_queue` | 1056 | ✓ | lint.rs, mcp/tools/entity.rs, mcp/tools/review.rs |
| `append_review_questions` | 1161 | ✓ | lint.rs, review.rs, mcp/tools/review.rs |

## Dependency Graph

```
                    ┌─────────────────┐
                    │   processor.rs  │
                    └────────┬────────┘
                             │
    ┌────────────────────────┼────────────────────────┐
    │                        │                        │
    ▼                        ▼                        ▼
┌─────────┐           ┌──────────────┐         ┌──────────────┐
│ scanner │           │   commands/  │         │  mcp/tools/  │
│         │           │              │         │              │
│ - full  │           │ - scan.rs    │         │ - entity.rs  │
│   scan  │           │ - serve.rs   │         │ - search.rs  │
│         │           │ - lint.rs    │         │ - review.rs  │
│         │           │ - search.rs  │         │ - document.rs│
│         │           │ - import.rs  │         │              │
│         │           │ - review.rs  │         │              │
└─────────┘           └──────────────┘         └──────────────┘
    │                        │                        │
    └────────────────────────┼────────────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │   database.rs   │
                    │ (count_facts_   │
                    │  with_sources)  │
                    └─────────────────┘
```

## Proposed Module Split

| Module | Functions | Lines (est.) |
|--------|-----------|--------------|
| `core.rs` | DocumentProcessor struct + methods | ~100 |
| `temporal.rs` | parse_temporal_tags, validate_*, detect_*, overlaps_*, calculate_recency_boost | ~500 |
| `sources.rs` | parse_source_references, parse_source_definitions | ~150 |
| `chunks.rs` | DocumentChunk, chunk_document | ~80 |
| `stats.rs` | count_facts*, calculate_fact_stats | ~50 |
| `review.rs` | parse_review_queue, append_review_questions | ~150 |
| `mod.rs` | Re-exports | ~30 |

## Notes

- `DocumentChunk` is not exported via lib.rs but is public; consider making it `pub(crate)` or exporting
- All 22 public items must remain accessible via `use factbase::*`
- 158 tests need to be distributed to appropriate modules
- Private helper functions (`normalize_type`, `singularize`) stay with DocumentProcessor
