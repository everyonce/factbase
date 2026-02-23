# lint.rs Public API Audit

**Date:** 2026-01-31
**File:** `src/commands/lint.rs`
**Lines:** 1657
**Unit Tests:** 12

## Summary

lint.rs is a CLI command module with 7 public items. Unlike processor.rs, all public structs are **CLI-only** - they are not exported via lib.rs and are only used within lint.rs itself for serialization output.

## Public Items (7)

### Structs (6)

| Struct | Purpose | External Usage |
|--------|---------|----------------|
| `LintArgs` | CLI argument parsing (clap) | main.rs (CLI dispatch) |
| `LintTemporalStats` | Temporal stats for JSON output | Internal only |
| `LintSourceStats` | Source stats for JSON output | Internal only |
| `ExportedQuestion` | Question export format | Internal only |
| `ExportedDocQuestions` | Document questions export | Internal only |
| `LintResult` | Lint result for JSON output | Internal only |

### Functions (1)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `cmd_lint` | `fn cmd_lint(args: LintArgs) -> anyhow::Result<()>` | Main lint command entry point |

## Dependency Graph

```
main.rs
  └── commands::lint::LintArgs (CLI argument struct)
  
commands/mod.rs
  └── pub use lint::cmd_lint (re-export for CLI dispatch)
```

### External Dependencies (imports from factbase::*)

lint.rs imports these from the library:
- `append_review_questions`
- `calculate_fact_stats`
- `count_facts_with_sources`
- `detect_illogical_sequences`
- `detect_temporal_conflicts`
- `find_repo_for_path`
- `format_json`, `format_yaml`
- `generate_*_questions` (6 functions)
- `parse_review_queue`
- `parse_source_definitions`, `parse_source_references`
- `parse_temporal_tags`, `validate_temporal_tags`
- `FileWatcher`
- `TemporalTagType`
- `MANUAL_LINK_REGEX`
- `config::validate_timeout`

## Internal Structs (not public)

| Struct | Purpose |
|--------|---------|
| `DocLintResult` | Per-document lint result for parallel processing |

## Test Coverage

12 unit tests in `#[cfg(test)] mod tests`:
- `test_lint_args_defaults`
- `test_lint_args_with_repo`
- `test_lint_args_with_all_flags`
- `test_lint_args_check_all_enables_all`
- `test_lint_temporal_stats_default`
- `test_lint_source_stats_default`
- `test_lint_result_serialization`
- `test_lint_result_with_stats`
- `test_exported_question_serialization`
- `test_exported_doc_questions_serialization`
- `test_output_format_default`
- `test_lint_args_timeout_validation`

Integration tests in `tests/cli_integration.rs`:
- `test_lint_json_flag`
- `test_lint_batch_size_flag`
- `test_lint_batch_size_with_parallel`

## Proposed Module Split

```
src/commands/lint/
├── mod.rs          # Re-exports, cmd_lint function
├── args.rs         # LintArgs struct (clap)
├── checks.rs       # Individual lint check functions
├── review.rs       # Review question generation
└── output.rs       # Output structs (LintTemporalStats, etc.)
```

### Module Breakdown

**args.rs (~120 lines)**
- `LintArgs` struct with all clap attributes

**checks.rs (~200 lines)**
- `lint_document()` function (currently internal)
- Orphan detection logic
- Broken link detection
- Stub detection
- Duplicate detection
- Type validation

**review.rs (~300 lines)**
- Review mode orchestration
- Question generation calls
- Dry-run handling
- Question export logic

**output.rs (~100 lines)**
- `LintTemporalStats`
- `LintSourceStats`
- `ExportedQuestion`
- `ExportedDocQuestions`
- `LintResult`
- `DocLintResult` (internal)

**mod.rs (~900 lines)**
- `cmd_lint()` main function
- Watch mode logic
- Parallel processing orchestration
- Output formatting
- Re-exports

## Key Observations

1. **CLI-only module**: Unlike processor.rs, lint.rs is not a library module - it's purely CLI functionality
2. **Self-contained**: All public structs are only used within lint.rs for serialization
3. **Heavy library usage**: Relies heavily on processor.rs functions for actual analysis
4. **Parallel processing**: Uses rayon for parallel document linting
5. **Watch mode**: Integrates with FileWatcher for live re-linting

## Refactoring Notes

- `LintArgs` must remain accessible from main.rs
- Other structs can become `pub(crate)` or even private after split
- `cmd_lint` is the only function that needs external visibility
- Tests are straightforward to distribute to respective modules
