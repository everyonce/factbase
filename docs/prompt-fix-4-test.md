# Prompt Fix 4 Test Results: Always Scan After Content Changes

## Fix Description

Added a scan reminder to the END of four workflow instruction constants:

- `DEFAULT_INGEST_CREATE_INSTRUCTION` (ingest workflow, step 3 — document creation)
- `DEFAULT_ENRICH_RESEARCH_INSTRUCTION` (add/enrich workflow, step 4 — research & update)
- `DEFAULT_CORRECT_FIX_INSTRUCTION` (correct workflow, step 3 — apply fixes)
- `DEFAULT_TRANSITION_APPLY_INSTRUCTION` (transition workflow, step 4 — apply changes)

Text appended to each:

> ⚠️ AFTER WRITING: Always call factbase(op='scan') after modifying or creating documents. This verifies quality, indexes new facts, and generates review questions. Without scan, your changes are not validated.

The repetition is intentional — weaker models need the reminder close to where the action happens, not just at the top of the workflow.

## Target Steps

| Step | Workflow | Instruction constant |
|------|----------|----------------------|
| 22 | ingest (add) | `DEFAULT_INGEST_CREATE_INSTRUCTION` |
| 29 | correct | `DEFAULT_CORRECT_FIX_INSTRUCTION` |
| 30 | transition | `DEFAULT_TRANSITION_APPLY_INSTRUCTION` |

`DEFAULT_ENRICH_RESEARCH_INSTRUCTION` also updated (enrich/add workflow research step).

## Test Results

- `cargo test --lib`: **2492 passed, 0 failed**
- `cargo build --release` warnings: **0**

## Rationale

The scan reminder already existed at the top of some workflows and in dedicated scan steps. This fix adds it immediately after the write action in each instruction, so the model sees it at the point of action rather than only at the start of the workflow. This is especially important for weaker models that may not carry context from earlier steps.
