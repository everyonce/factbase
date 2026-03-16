# Prompt Fix 5 Test Results: Glossary Check Before Adding New Terms

## Change Summary

Added a structured `GLOSSARY DISCIPLINE` section to two workflow instructions:

- `DEFAULT_INGEST_CREATE_INSTRUCTION` — replaced the inline glossary bullet in Document rules
- `DEFAULT_ENRICH_RESEARCH_INSTRUCTION` — replaced item 5 (Glossary maintenance)

### New text (both instructions, adapted for context):

```
GLOSSARY DISCIPLINE: Before writing any abbreviation, acronym, or domain-specific term:
1. Call search(query='definitions [term]') or factbase(op='list', doc_type='definition')
2. If the term is in the glossary → use it without defining
3. If NOT in the glossary → add a definition entry FIRST, then use it
4. This prevents ambiguous questions from being generated on future scans
```

## Step 27 Target: Unknown Term Should Trigger Glossary Check

The previous inline glossary guidance was buried in prose and easy to skip. The new `GLOSSARY DISCIPLINE:` label with explicit numbered steps makes the required behavior unambiguous:

- Step 1 forces a lookup before writing any term
- Step 3 requires creating the definition entry *before* using the term (not after)
- Step 4 explains the consequence, reinforcing compliance

This directly addresses the eval failure where an agent introduced an unknown term without checking the glossary first, causing an `ambiguous` review question on the next scan.

## Test Results

```
cargo test --lib
test result: ok. 2492 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo build --release
0 warnings
```

### Updated tests

- `test_ingest_create_has_glossary_maintenance` — still passes (checks `factbase(op='list'` and `ambiguous questions`, both present in new text)
- `test_enrich_research_has_glossary_maintenance` — updated to assert `GLOSSARY DISCIPLINE` and `definition entry FIRST`
