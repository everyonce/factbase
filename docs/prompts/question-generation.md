# Question Generation Prompt Design

## Overview

This document defines the LLM prompt for generating review questions during `factbase lint --review`.

## Input Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `document_content` | string | Full document text with line numbers prepended |
| `known_entities` | array | List of `{id, title}` for duplicate detection |
| `temporal_stats` | object | `{total_facts, facts_with_tags, coverage_percent}` |
| `source_stats` | object | `{total_refs, orphan_refs, orphan_defs}` |
| `stale_threshold_days` | int | Days after which `@t[~...]` is considered stale |
| `current_date` | string | Today's date in YYYY-MM-DD format |

## Output Format

```json
[
  {
    "type": "temporal",
    "line_ref": 5,
    "description": "\"VP Engineering at BigCo\" has no end date - is this role still current?"
  },
  {
    "type": "conflict",
    "line_ref": 4,
    "description": "CTO ended 2022, VP started 2022 - same month? Overlap?"
  }
]
```

## Question Types

| Type | Trigger Conditions |
|------|-------------------|
| `temporal` | Fact without `@t[...]` tag; role with `@t[YYYY..]` older than 1 year |
| `conflict` | Overlapping date ranges for mutually exclusive facts; same attribute with different values |
| `missing` | Fact without `[^N]` source reference; orphan footnote reference |
| `ambiguous` | Unclear phrasing; location without context; relationship without direction |
| `stale` | `@t[~...]` date older than threshold; source date older than threshold |
| `duplicate` | High similarity with entity in known_entities list |

## Final Prompt Template

Tested with deepseek-coder-v2:16b on 2026-01-29. Produces specific, actionable questions.

```
You are a fact-checking assistant. Analyze this document and generate review questions for issues.

Document (with line numbers):
{document_with_line_numbers}

Known Entities:
{entities_list}

Statistics:
- Temporal coverage: {temporal_stats.coverage_percent}% ({temporal_stats.facts_with_tags}/{temporal_stats.total_facts} facts have @t[...] tags)
- Current date: {current_date}
- Stale threshold: {stale_threshold_days} days

Question Types:
- temporal: Facts without @t[...] tags (e.g., roles, positions, dates)
- conflict: Contradictory facts (overlapping dates for exclusive roles, different values)
- missing: Facts without [^N] source references
- ambiguous: Unclear meaning (location context, vague phrasing)
- stale: @t[~...] dates older than threshold, or @t[YYYY..] ongoing roles >1 year old
- duplicate: Similar to known entities

IMPORTANT: In the description field, QUOTE the specific text from the document and ask a specific question about it.

Examples of GOOD descriptions:
- '"Senior Software Engineer at Example Corp" - when did this role start? Is it still current?'
- '"Joined in 2022" - what month did they join? Use @t[2022-MM..] format'
- '"Previously worked at CloudScale Inc" - what years? What role?'
- '"CTO at Acme Corp @t[2020..2023]" and "CEO at Acme Corp @t[2022..2024]" overlap in 2022-2023 - were both roles held simultaneously?'

Examples of BAD descriptions (too generic):
- 'Facts without @t[...] tags'
- 'Missing temporal information'

Return ONLY a JSON array with objects having:
- "type": one of temporal, conflict, missing, ambiguous, stale, duplicate
- "line_ref": line number (integer, 1-indexed)
- "description": specific question quoting the text

Return [] if no issues.
```

## Implementation Notes

### Line Number Prepending

Before sending to LLM, prepend line numbers to document:

```rust
fn prepend_line_numbers(content: &str) -> String {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:4}: {}", i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}
```

### JSON Extraction

Use same pattern as link detection - try direct parse, then extract JSON from response:

```rust
fn extract_questions(response: &str) -> Vec<ReviewQuestion> {
    // Try direct parse
    if let Ok(questions) = serde_json::from_str::<Vec<RawQuestion>>(response) {
        return questions.into_iter().filter_map(validate_question).collect();
    }
    
    // Try to extract JSON array from response
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            let json_str = &response[start..=end];
            if let Ok(questions) = serde_json::from_str::<Vec<RawQuestion>>(json_str) {
                return questions.into_iter().filter_map(validate_question).collect();
            }
        }
    }
    
    Vec::new()
}
```

### Validation

Validate each question before accepting:

```rust
fn validate_question(raw: RawQuestion) -> Option<ReviewQuestion> {
    // Validate type is one of the known types
    let question_type = match raw.r#type.as_str() {
        "temporal" => QuestionType::Temporal,
        "conflict" => QuestionType::Conflict,
        "missing" => QuestionType::Missing,
        "ambiguous" => QuestionType::Ambiguous,
        "stale" => QuestionType::Stale,
        "duplicate" => QuestionType::Duplicate,
        _ => return None,
    };
    
    // Validate line_ref is positive
    if raw.line_ref < 1 {
        return None;
    }
    
    // Validate description is non-empty
    if raw.description.trim().is_empty() {
        return None;
    }
    
    Some(ReviewQuestion {
        question_type,
        line_ref: Some(raw.line_ref),
        description: raw.description,
        answered: false,
        answer: None,
        line_number: 0, // Set when writing to file
    })
}
```

## Test Results

### Test 1: Person with missing temporal tags

Input document:
```markdown
   1: <!-- factbase:abc123 -->
   2: # Alice Chen
   3: 
   4: Senior Software Engineer at Example Corp.
   5: 
   6: ## Role
   7: Tech lead for the Platform API project.
   8: 
   9: ## Notes
  10: Joined in 2022. Previously worked at CloudScale Inc.
```

LLM output (deepseek-coder-v2:16b, 2026-01-29):
```json
[
    {
        "type": "temporal",
        "line_ref": 4,
        "description": "\"Senior Software Engineer at Example Corp\" - when did this role start? Is it still current?"
    },
    {
        "type": "missing",
        "line_ref": 9,
        "description": "\"Joined in 2022\" - what month did they join? Use @t[2022-MM..] format"
    }
]
```

✓ Correctly identified missing temporal tags
✓ Specific questions with quoted text
✓ Actionable suggestions (use @t[2022-MM..] format)

### Test 2: Person with conflicts

Input document:
```markdown
   1: <!-- factbase:def456 -->
   2: # Bob Martinez
   3: 
   4: ## Career
   5: - CTO at Acme Corp @t[2020..2023]
   6: - CEO at Acme Corp @t[2022..2024]
   7: - Advisor at StartupX @t[2021..]
   8: 
   9: ## Education
  10: - MBA from Stanford @t[?]
  11: - BS Computer Science from MIT
  12: 
  13: ---
  14: [^1]: LinkedIn profile, scraped 2024-01-15
```

LLM output:
```json
[
    {
        "type": "temporal",
        "line_ref": 10,
        "description": "\"MBA from Stanford @t[?]\" - what year did they complete this degree?"
    },
    {
        "type": "conflict",
        "line_ref": 6,
        "description": "\"CEO at Acme Corp @t[2022..2024]\" and \"CTO at Acme Corp @t[2020..2023]\" overlap in 2022-2023 - were both roles held simultaneously?"
    },
    {
        "type": "missing",
        "line_ref": 5,
        "description": "\"CTO at Acme Corp @t[2020..2023]\" - this role is missing a source reference."
    },
    {
        "type": "ambiguous",
        "line_ref": 11,
        "description": "\"BS Computer Science from MIT\" - what year did they graduate?"
    }
]
```

✓ Correctly identified overlapping date ranges as conflict
✓ Identified missing source references
✓ Identified @t[?] as needing clarification
✓ Identified missing graduation year

## Error Handling

1. **LLM returns malformed JSON**: Return empty array, log warning
2. **LLM returns invalid question type**: Skip that question, continue with others
3. **LLM returns line_ref outside document range**: Skip that question
4. **LLM timeout**: Propagate error to caller
5. **Empty document**: Return empty array without calling LLM
