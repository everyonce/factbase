# Test Report: `init_repository` MCP Operation (Fix #663)

**Date:** 2026-03-15  
**Binary:** `target/release/factbase` (v2026.3.37)  
**Transport:** MCP stdio (`factbase mcp`)

---

## Test 1: Fresh Empty Directory — PASS

**Setup:** Created `/tmp/test-init-fresh` (empty directory).

**Call:**
```json
{"op": "init_repository", "path": "/tmp/test-init-fresh"}
```

**Response:**
```json
{
  "id": "test-init-fresh",
  "name": "test-init-fresh",
  "path": "/private/tmp/test-init-fresh",
  "created": [".factbase/", "perspective.yaml", ".gitignore"],
  "markdown_files_found": 0,
  "message": "Repository 'test-init-fresh' initialized at /private/tmp/test-init-fresh. 0 markdown files found. Call scan_repository to index."
}
```

**Verified:**
- `.factbase/` directory created ✓
- `perspective.yaml` scaffolded ✓
- `.gitignore` created ✓
- No errors ✓

---

## Test 2: Directory With Existing Files — PASS

**Setup:** Created `/tmp/test-init-with-files` with 3 markdown files (`note1.md`, `note2.md`, `note3.md`).

**Call:**
```json
{"op": "init_repository", "path": "/tmp/test-init-with-files"}
```

**Response:**
```json
{
  "id": "test-init-with-files",
  "name": "test-init-with-files",
  "path": "/private/tmp/test-init-with-files",
  "created": [".factbase/", "perspective.yaml", ".gitignore"],
  "markdown_files_found": 3,
  "message": "Repository 'test-init-with-files' initialized at /private/tmp/test-init-with-files. 3 markdown files found. Call scan_repository to index."
}
```

**Verified:**
- Initialized correctly ✓
- All 3 existing `.md` files untouched (content unchanged) ✓
- `markdown_files_found: 3` correctly reported ✓

---

## Test 3: Already-Initialized Directory — PASS

**Setup:** Re-used `/tmp/test-init-fresh` from Test 1 (already registered in the database).

**Call:**
```json
{"op": "init_repository", "path": "/tmp/test-init-fresh"}
```

**Response:**
```json
{
  "already_exists": true,
  "id": "test-init-fresh",
  "name": "test-init-fresh",
  "path": "/private/tmp/test-init-fresh",
  "message": "Repository 'test-init-fresh' already registered at this path."
}
```

**Verified:**
- Graceful `already_exists: true` response (no error) ✓
- No duplicate registration ✓
- `.factbase/` still present, no data loss ✓
- `perspective.yaml` still present, no data loss ✓

---

## Unit Tests

Existing unit tests in `src/mcp/tools/repository.rs` all pass:

```
test mcp::tools::repository::tests::test_init_repository_tolerates_preexisting_config ... ok
test mcp::tools::repository::tests::test_init_repository_nonexistent_dir ... ok
test mcp::tools::repository::tests::test_init_repository_already_registered ... ok

test result: ok. 3 passed; 0 failed
```

## Summary

| Test | Result |
|------|--------|
| Fresh empty directory | **PASS** |
| Directory with existing files | **PASS** |
| Already-initialized directory | **PASS** |
| Unit tests | **PASS** (3/3) |
| Build warnings | **0** |
