# Cross-Validate Prompt Design

## Overview

This document defines the LLM prompts for cross-document fact validation during `factbase check --deep-check`.

There are two modes:
- **Fact-pair mode** (`cross_validate_pairs` prompt key): Uses pre-computed fact-level embeddings to find semantically similar facts across documents, then classifies each pair. This is the default for `--deep-check`.
- **Legacy mode** (`cross_validate` prompt key): Generates embeddings per-fact at check time and validates against the full document corpus. Used as fallback when fact embeddings are not yet populated.

## Fact-Pair Mode

### Input

Fact pairs are discovered by comparing fact-level embeddings across documents. Each pair consists of two facts from different documents that are semantically similar.

### Prompt Template

```
Compare these fact pairs from different knowledge base documents.

For each pair, trace the evidence chain:
1. Compare Fact A and Fact B — do they address the same claim?
2. Consider source citations and temporal context for each
3. Classify the relationship

Statuses:
- SUPPORTS: Fact B confirms or is consistent with Fact A
- CONTRADICTS: Facts give different answers to the same question about the same entity
- SUPERSEDES: Fact B provides newer information that replaces Fact A
- CONSISTENT: Facts are about different aspects and don't conflict

Common mistakes to avoid:
✗ WRONG: Flagging as CONTRADICTS because the SOURCES are different. Two sources can confirm the same fact.
✗ WRONG: Flagging as SUPERSEDES because one source is older. A 2019 source citing "founded in 1924" is NOT superseded — the fact is timeless.
✗ WRONG: Flagging boundary-month overlaps as CONTRADICTS. "Role A ends 2016-11" + "Role B starts 2016-11" = normal transition.
✗ WRONG: Flagging two DIFFERENT facts about the same entity as contradicting. "Fleet size: 900" and "Destinations: 200" coexist.
✓ RIGHT: CONTRADICTS only when two sources give DIFFERENT answers to the SAME question about the SAME entity.

{fact_pairs}
---

Respond ONLY with a JSON array. Each element must have: pair (number), status (SUPPORTS/CONTRADICTS/SUPERSEDES/CONSISTENT), reason (string).
```

### Output Format

```json
[
  {"pair": 1, "status": "CONSISTENT", "reason": "Different attributes of the same entity"},
  {"pair": 2, "status": "CONTRADICTS", "reason": "Both claim different founding years for the same company"}
]
```

### Resumption

For large repositories, cross-validation may not complete in a single call. The MCP tool returns an opaque `resume` token in the response. Pass it back on subsequent calls to continue where the previous call left off. The `checked_pair_ids` parameter is deprecated and ignored.

## Prompt Customization

Both prompts can be overridden via config:

```yaml
prompts:
  cross_validate_pairs: "Your custom prompt with {fact_pairs} placeholder"
  cross_validate: "Your custom prompt with {document_content} and {evidence} placeholders"
```
