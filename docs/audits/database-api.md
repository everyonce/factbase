# database.rs Public API Audit

**File:** `src/database.rs`  
**Lines:** 3407  
**Tests:** 51  
**Date:** 2026-01-31

## Public Items (49 total)

### Structs (2)

| Name | Line | Exported via lib.rs | Used By |
|------|------|---------------------|---------|
| `EmbeddingStatus` | 88 | ✓ | scan.rs (check_embedding_status) |
| `Database` | 98 | ✓ | All commands, MCP tools, processor.rs |

### Functions (47)

#### Constructor & Pool Management (4)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `new()` | 112 | Default constructor (pool_size=4, no compression) | Tests |
| `with_pool_size()` | 122 | Constructor with custom pool size | Tests |
| `with_options()` | 139 | Full constructor (pool_size + compression) | All commands |
| `pool_stats()` | 2145 | Get pool statistics | status.rs |

#### Repository Operations (7)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `upsert_repository()` | 360 | Insert or update repository | scan.rs |
| `get_repository()` | 385 | Get repository by ID | commands/mod.rs, scan.rs |
| `list_repositories()` | 400 | List all repositories | repo.rs, status.rs, mcp/tools |
| `add_repository()` | 412 | Add new repository | repo.rs |
| `remove_repository()` | 443 | Remove repository and docs | repo.rs |
| `get_repository_by_path()` | 477 | Find repo by filesystem path | scan.rs |
| `list_repositories_with_stats()` | 503 | List repos with doc counts | repo.rs |

#### Repository Metadata (1)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `update_last_check_at()` | 489 | Update check timestamp | check.rs |

#### Document Operations (9)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `upsert_document()` | 553 | Insert or update document | scanner.rs |
| `update_document_hash()` | 575 | Update content hash | scanner.rs |
| `needs_update()` | 588 | Check if doc needs re-indexing | scanner.rs |
| `get_document()` | 623 | Get document by ID | show.rs, links.rs, mcp/tools |
| `get_document_by_path()` | 643 | Get document by file path | scanner.rs |
| `get_documents_for_repo()` | 664 | Get all docs in repository | check.rs, export.rs |
| `mark_deleted()` | 735 | Mark document as deleted | scanner.rs |
| `hard_delete_document()` | 756 | Permanent delete | scan.rs |

#### Transaction Control (2)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `begin_transaction()` | 601 | Start transaction | scanner.rs |
| `commit_transaction()` | 608 | Commit transaction | scanner.rs |

#### Statistics & Caching (6)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `get_stats()` | 788 | Basic repo stats | stats.rs, status.rs |
| `get_detailed_stats()` | 807 | Extended stats | status.rs |
| `invalidate_stats_cache()` | 820 | Clear cached stats | scanner.rs |
| `compute_temporal_stats()` | 1073 | Temporal tag statistics | status.rs, check.rs |
| `compute_source_stats()` | 1153 | Source reference statistics | status.rs, check.rs |
| `health_check()` | 2138 | Verify DB connectivity | doctor.rs |

#### Embedding Operations (6)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `upsert_embedding()` | 1258 | Store document embedding | scanner.rs |
| `upsert_embedding_chunk()` | 1263 | Store chunked embedding | scanner.rs |
| `delete_embedding()` | 1301 | Remove embedding | scanner.rs |
| `get_chunk_metadata()` | 1322 | Get chunk info for doc | scanner.rs |
| `check_embedding_status()` | 1347 | Check embedding coverage | scan.rs |
| `get_embedding_dimension()` | 1396 | Get vector dimension | scan.rs |

#### Search Operations (6)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `find_similar_documents()` | 1414 | Find duplicates by similarity | check.rs |
| `search_semantic()` | 1482 | Vector similarity search | search.rs, mcp/tools/search.rs |
| `search_by_title()` | 1505 | Title-based search | search.rs |
| `search_content()` | 1583 | Full-text grep search | grep.rs, mcp/tools/search.rs |
| `search_semantic_with_query()` | 1718 | Search with query embedding | mcp/tools/search.rs |
| `search_semantic_paginated()` | 1746 | Paginated semantic search | mcp/tools/search.rs |

#### Link Operations (4)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `get_all_document_titles()` | 1979 | Get titles for link detection | scanner.rs |
| `update_links()` | 2014 | Update document links | scanner.rs |
| `get_links_from()` | 2041 | Get outgoing links | links.rs, mcp/tools/entity.rs |
| `get_links_to()` | 2054 | Get incoming links | links.rs, mcp/tools/entity.rs |

#### Listing & Utilities (2)
| Method | Line | Description | Used By |
|--------|------|-------------|---------|
| `list_documents()` | 2086 | List docs with filters | mcp/tools/entity.rs |
| `vacuum()` | 2163 | Optimize database | db.rs |

## Dependency Graph

```
                         ┌─────────────────┐
                         │   database.rs   │
                         └────────┬────────┘
                                  │
    ┌─────────────────────────────┼─────────────────────────────┐
    │                             │                             │
    ▼                             ▼                             ▼
┌─────────────┐           ┌──────────────┐             ┌──────────────┐
│  scanner.rs │           │   commands/  │             │  mcp/tools/  │
│             │           │              │             │              │
│ - upsert_*  │           │ - db.rs      │             │ - entity.rs  │
│ - get_*     │           │ - doctor.rs  │             │ - search.rs  │
│ - update_*  │           │ - export.rs  │             │ - review.rs  │
│ - delete_*  │           │ - grep.rs    │             │ - document.rs│
│ - links     │           │ - init.rs    │             │ - mod.rs     │
│ - embed     │           │ - links.rs   │             │              │
│             │           │ - check.rs    │             │              │
│             │           │ - mod.rs     │             │              │
│             │           │ - repo.rs    │             │              │
│             │           │ - review.rs  │             │              │
│             │           │ - scan.rs    │             │              │
│             │           │ - search.rs  │             │              │
│             │           │ - serve.rs   │             │              │
│             │           │ - show.rs    │             │              │
│             │           │ - stats.rs   │             │              │
│             │           │ - status.rs  │             │              │
└─────────────┘           └──────────────┘             └──────────────┘
                                  │
                                  ▼
                         ┌─────────────────┐
                         │  processor.rs   │
                         │ (uses Database  │
                         │  for ID checks) │
                         └─────────────────┘
```

## Proposed Module Split

| Module | Functions | Lines (est.) |
|--------|-----------|--------------|
| `mod.rs` | Database struct, constructors, pool_stats, health_check | ~200 |
| `schema.rs` | Schema init, migrations, SCHEMA_VERSION | ~300 |
| `documents.rs` | upsert_document, get_document*, mark_deleted, soft/hard_delete | ~300 |
| `repositories.rs` | *_repository functions, list_repositories_with_stats | ~200 |
| `links.rs` | update_links, get_links_from, get_links_to, get_all_document_titles | ~150 |
| `embeddings.rs` | upsert_embedding*, delete_embedding, get_chunk_metadata, check_embedding_status, get_embedding_dimension | ~250 |
| `search.rs` | search_semantic*, search_by_title, search_content, find_similar_documents | ~500 |
| `stats.rs` | get_stats, get_detailed_stats, compute_temporal_stats, compute_source_stats, cache management | ~400 |

## Categorization by Functionality

### CRUD Operations
- **Repositories:** upsert, get, list, add, remove, get_by_path (7)
- **Documents:** upsert, get, get_by_path, get_for_repo, mark_deleted, hard_delete (8)
- **Links:** update, get_from, get_to (3)
- **Embeddings:** upsert, upsert_chunk, delete (3)

### Query Operations
- **Search:** semantic, by_title, content, paginated, with_query, find_similar (6)
- **Stats:** get_stats, get_detailed_stats, compute_temporal_stats, compute_source_stats (4)
- **Metadata:** get_all_document_titles, get_chunk_metadata, check_embedding_status, get_embedding_dimension (4)

### Infrastructure
- **Constructors:** new, with_pool_size, with_options (3)
- **Transactions:** begin_transaction, commit_transaction (2)
- **Maintenance:** vacuum, health_check, pool_stats, invalidate_stats_cache (4)

## Notes

- `EmbeddingStatus` is the only non-Database public struct
- `PoolStats` is defined in models.rs, not database.rs
- All 47 methods must remain accessible via `Database` struct
- 51 tests need to be distributed to appropriate modules
- Schema/migration code is private but substantial (~300 lines)
- Stats caching logic is complex and should stay together
- Compression handling is internal (not exposed as public API)
