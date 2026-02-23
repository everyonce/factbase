# Test Fixtures

This directory contains test fixtures for Phase 5 E2E testing.

## test-repo/

A realistic knowledge base with diverse documents for integration testing.

### Structure

- `people/` - Person documents (engineers, managers, designers)
- `projects/` - Project documents with cross-references
- `concepts/` - Technical concept documents
- `notes/` - Edge case documents (empty, large, special chars)

### Usage

```rust
use crate::common::fixtures::copy_fixture_repo;

let temp = copy_fixture_repo("test-repo");
// temp.path() contains a copy of the fixture
```

### Notes

- Documents have factbase headers injected during first scan
- Cross-references use document titles (detected by LLM)
- Some documents have manual `[[id]]` links for testing
