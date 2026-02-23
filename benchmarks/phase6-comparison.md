# Phase 6 Performance Comparison

## Embedding Model Upgrade: nomic-embed-text → qwen3-embedding:0.6b

### Model Specifications

| Metric | nomic-embed-text | qwen3-embedding:0.6b |
|--------|------------------|----------------------|
| Dimensions | 768 | 1024 |
| Context Window | 2K tokens (~8K chars) | 32K tokens (~128K chars) |
| Model Size | 274 MB | 639 MB |
| Memory Usage | ~400 MB | ~684 MB |

### Benchmark Results (2026-01-25)

#### Embedding Generation Performance

| Words | Chars | Time | Chars/sec |
|-------|-------|------|-----------|
| 100 | 600 | 6.43s | 93 |
| 500 | 3000 | 3.74s | 802 |
| 1000 | 6000 | 5.34s | 1124 |
| 2000 | 12000 | 3.74s | 3207 |

Note: First embedding call has cold-start overhead (~6s). Subsequent calls are faster.

#### Batch vs Individual Embedding

| Method | 10 Documents | Per-Doc Time | Speedup |
|--------|--------------|--------------|---------|
| Individual | 20.88s | 2.1s | 1x |
| Batch | 2.21s | 220.9ms | 9.5x |

Batch embedding provides ~9.5x speedup over individual calls.

#### Search Latency

| Query | Total Time | Embed Time | Search Time |
|-------|------------|------------|-------------|
| "test document" | 4.09s | 4.07s | 16ms |
| "lorem ipsum" | 1.57s | 1.56s | 9ms |
| "software engineering" | 317ms | 311ms | 6ms |
| "database performance" | 270ms | 267ms | 3ms |
| "machine learning" | 265ms | 263ms | 2ms |

Statistics:
- Min: 265ms
- Max: 4.09s (cold start)
- Avg: 1.30s
- P50: 317ms

Note: Search time (database query) is consistently fast (2-16ms). Embedding generation dominates latency.

#### Scan Performance

| Docs | Total Time | Per-Doc Time | Docs/sec |
|------|------------|--------------|----------|
| 5 | 7.77s | 1.55s | 0.6 |
| 10 | 95.52s | 9.55s | 0.1 |
| 20 | 103.47s | 5.17s | 0.2 |

Note: Scan includes embedding generation AND link detection (LLM calls). Link detection dominates for larger repos.

### Key Observations

1. **Embedding Quality**: qwen3-embedding:0.6b provides 1024-dim embeddings vs 768-dim, potentially better semantic representation.

2. **Context Window**: 16x larger context window (32K vs 2K tokens) eliminates silent truncation for most documents.

3. **Chunking**: Documents >128K chars are now chunked with 2K overlap, ensuring full content is indexed.

4. **Cold Start**: First embedding call has significant overhead (~4-6s). Subsequent calls are much faster (~265ms).

5. **Batch Efficiency**: Batch embedding provides ~9.5x speedup, critical for large repos.

6. **Search Speed**: Database search is fast (2-16ms). Embedding generation is the bottleneck.

### Recommendations

1. Use batch embedding for scans (already implemented)
2. Consider caching query embeddings for repeated searches
3. For interactive use, expect ~300ms latency after warm-up
4. Link detection (LLM) is the slowest part of scanning - consider incremental updates
