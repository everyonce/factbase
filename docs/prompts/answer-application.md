# Answer Application Prompt Design

## Overview

This document defines the LLM prompt for applying answered review questions during `factbase review --apply`.

## Input Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `section_content` | string | Document section text (lines affected by questions) |
| `qa_pairs` | array | List of `{question, answer}` pairs to apply |
| `current_date` | string | Today's date in YYYY-MM-DD format |

## Output Format

The LLM returns the rewritten section as plain text with:
- Updated temporal tags (`@t[...]`)
- Updated source references (`[^N]`)
- Preserved formatting and structure

## Special Answer Keywords

| Keyword | Behavior |
|---------|----------|
| `dismiss` or `ignore` | Remove question, no changes to document |
| `delete` | Remove the referenced fact line entirely |
| `split: ...` | LLM interprets as instruction to split fact into multiple lines |

## Final Prompt Template

Tested with deepseek-coder-v2:16b on 2026-01-29.

**Key insight**: The LLM performs better when given explicit instructions about what changes to make, rather than asking it to interpret Q&A pairs. The implementation should:
1. Pre-process Q&A pairs to determine specific changes needed
2. Format changes as explicit instructions (e.g., "change @t[2022..] to @t[2022-03..2024-03]")
3. Send the explicit changes to the LLM for application

```
Rewrite this section with the exact changes specified.

ORIGINAL:
{section_content}

CHANGES:
{changes_formatted}

RULES:
1. Apply ALL changes exactly as specified
2. Keep all other lines unchanged
3. Preserve existing source references [^N] unless change says to remove
4. Use ONLY these date formats in @t[] tags:
   - Year only: @t[2022]
   - Year-month: @t[2022-03] (NOT "Mar 2022")
   - Range: @t[2020..2022-02]
5. If change says "delete line", remove that line entirely
6. If change says "split into", create separate list items

Output the complete rewritten section only:
```

## Q&A Pair Formatting

The implementation should pre-process Q&A pairs into explicit change instructions:

```rust
fn format_changes(qa_pairs: &[(String, String)], section: &str) -> String {
    let mut changes = Vec::new();
    
    for (question, answer) in qa_pairs {
        // Extract the fact being questioned (quoted text in question)
        // Interpret the answer to determine the change
        // Format as explicit instruction
        
        // Example transformations:
        // Q: "VP Engineering @t[2022..]" - still current?
        // A: "No, left March 2024"
        // -> "Line with 'VP Engineering': change @t[2022..] to @t[2022..2024-03]"
    }
    
    changes.join("\n")
}
```

Example formatted changes:
```
1. Line "- VP Engineering at BigCo @t[2022..]": change @t[2022..] to @t[2022-03..2024-03]
2. Line "- CTO at Acme Corp @t[2020..2022]": change @t[2020..2022] to @t[2020..2022-02]
```

## Implementation Notes

### Two-Phase Answer Processing

The implementation uses a two-phase approach for better reliability:

**Phase 1: Answer Interpretation (may use LLM)**
- Parse natural language answers into structured changes
- For simple answers (dates, "delete", "dismiss"), use regex/heuristics
- For complex answers (split, ambiguous), optionally use LLM to interpret

**Phase 2: Change Application (uses LLM)**
- Send explicit change instructions to LLM
- LLM applies changes to section text
- Validate output before writing

This separation improves reliability because:
1. Explicit instructions are easier for LLM to follow
2. Interpretation failures can be caught before modification
3. Changes can be validated independently

### Answer Interpretation Heuristics

```rust
fn interpret_answer(question: &str, answer: &str) -> ChangeInstruction {
    let answer_lower = answer.trim().to_lowercase();
    
    // Special keywords
    if answer_lower == "dismiss" || answer_lower == "ignore" {
        return ChangeInstruction::Dismiss;
    }
    if answer_lower == "delete" {
        return ChangeInstruction::Delete;
    }
    if answer_lower.starts_with("split:") {
        return ChangeInstruction::Split(answer[6..].trim().to_string());
    }
    
    // Try to extract date information
    if let Some(dates) = extract_dates_from_answer(answer) {
        return ChangeInstruction::UpdateTemporal(dates);
    }
    
    // Fall back to LLM interpretation
    ChangeInstruction::NeedsLlmInterpretation
}
```

### Date Extraction from Answers

Common patterns to recognize:
- "left in March 2024" → end date 2024-03
- "started January 2020" → start date 2020-01
- "from 2018 to 2020" → range 2018..2020
- "ended Feb 2022, started March 2022" → multiple dates

```rust
fn extract_dates_from_answer(answer: &str) -> Option<DateChanges> {
    // Regex patterns for common date expressions
    let month_year = r"(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{4})";
    let year_only = r"\b(19|20)\d{2}\b";
    
    // Extract and convert to YYYY-MM format
    // ...
}
```

### Section Identification

Identify the affected section by:
1. Find all line numbers referenced in answered questions
2. Expand to include the containing section (from `##` heading to next `##` or end)
3. If questions span multiple sections, process each section separately

```rust
fn identify_affected_sections(
    content: &str,
    questions: &[AnsweredQuestion],
) -> Vec<(usize, usize, String)> {
    // Returns: Vec<(start_line, end_line, section_content)>
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();
    
    for q in questions {
        if let Some(line_ref) = q.line_ref {
            let (start, end) = find_section_bounds(&lines, line_ref);
            // Merge overlapping sections
            // ...
        }
    }
    
    sections
}
```

### Special Answer Detection

Check for special keywords before sending to LLM:

```rust
fn is_special_answer(answer: &str) -> Option<SpecialAnswer> {
    let trimmed = answer.trim().to_lowercase();
    if trimmed == "dismiss" || trimmed == "ignore" {
        return Some(SpecialAnswer::Dismiss);
    }
    if trimmed == "delete" {
        return Some(SpecialAnswer::Delete);
    }
    if trimmed.starts_with("split:") {
        return Some(SpecialAnswer::Split(answer[6..].trim().to_string()));
    }
    None
}
```

### Handling Dismiss

For `dismiss`/`ignore` answers:
- Remove the question from Review Queue
- Make no changes to document content
- Do not send to LLM

### Handling Delete

For `delete` answers:
- Remove the referenced line from document
- Remove the question from Review Queue
- Can be done without LLM

### Handling Split

For `split:` answers:
- Include in LLM prompt with the split instruction
- LLM interprets how to break the fact into multiple lines
- Example: `split: separate role and company` → LLM creates two list items

## Test Results

### Test 1: Simple temporal update

Input section:
```markdown
## Career
- VP Engineering at BigCo @t[2022..]
- CTO at Acme Corp @t[2020..2022]
```

Changes:
```
Line "- VP Engineering at BigCo @t[2022..]": change @t[2022..] to @t[2022..2024-03]
```

LLM output (deepseek-coder-v2:16b, 2026-01-29):
```markdown
## Career
- VP Engineering at BigCo @t[2022..2024-03]
- CTO at Acme Corp @t[2020..2022]
```

✓ Correctly updated end date
✓ Preserved other facts unchanged
✓ Used correct temporal tag format

### Test 2: Multiple related changes

Input section:
```markdown
## Career
- CTO at Acme Corp @t[2020..2022]
- VP Engineering at BigCo @t[2022..]
```

Changes:
```
1. Line "- CTO at Acme Corp @t[2020..2022]": change @t[2020..2022] to @t[2020..2022-02]
2. Line "- VP Engineering at BigCo @t[2022..]": change @t[2022..] to @t[2022-03..2024-03]
```

LLM output:
```markdown
## Career
- CTO at Acme Corp @t[2020..2022-02]
- VP Engineering at BigCo @t[2022-03..2024-03]
```

✓ Applied both changes consistently
✓ Used month precision as specified
✓ Maintained document structure

### Test 3: Delete answer

Input section:
```markdown
## Notes
- Rumored to be joining StartupX
- Known for public speaking
```

Changes:
```
Delete line "- Rumored to be joining StartupX"
```

LLM output:
```markdown
## Notes
- Known for public speaking
```

✓ Line removed entirely
✓ Other content preserved

### Test 4: Split answer

Input section:
```markdown
## Career
- Software Engineer then Tech Lead at Example Corp @t[2018..2022]
```

Changes:
```
Split line "- Software Engineer then Tech Lead at Example Corp @t[2018..2022]" into:
- Software Engineer at Example Corp @t[2018..2020]
- Tech Lead at Example Corp @t[2020..2022]
```

LLM output:
```markdown
## Career
- Software Engineer at Example Corp @t[2018..2020]
- Tech Lead at Example Corp @t[2020..2022]
```

✓ Split into two separate facts
✓ Each has correct temporal tag
✓ Company name preserved on both

### Test 5: Dismiss answer

For `dismiss`/`ignore` answers:
- Handled in pre-processing, not sent to LLM
- Question removed from Review Queue
- Section unchanged

## Error Handling

1. **LLM returns empty response**: Keep original section, log warning
2. **LLM returns malformed output**: Keep original section, log warning, keep question in queue
3. **LLM timeout**: Propagate error to caller
4. **Section not found**: Skip question, log warning
5. **Multiple questions reference deleted line**: Process delete first, skip other questions for that line

## Validation

Before applying LLM output:
1. Verify output is non-empty
2. Verify output preserves section structure (same heading level)
3. Optionally: verify temporal tags are syntactically valid
4. If validation fails, keep original and log warning

```rust
fn validate_rewritten_section(original: &str, rewritten: &str) -> bool {
    // Check non-empty
    if rewritten.trim().is_empty() {
        return false;
    }
    
    // Check heading preserved (if original had one)
    if original.trim_start().starts_with("##") {
        if !rewritten.trim_start().starts_with("##") {
            return false;
        }
    }
    
    true
}
```

## Atomic File Writes

To prevent corruption on failure:

```rust
fn write_document_safely(path: &Path, content: &str) -> Result<()> {
    let temp_path = path.with_extension("md.tmp");
    fs::write(&temp_path, content)?;
    fs::rename(&temp_path, path)?;
    Ok(())
}
```
