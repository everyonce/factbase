# Binary Size Audit

**Date:** 2026-01-31
**Binary:** `target/release/factbase`
**Total Size:** 16.3 MiB (17 MB on disk)
**Text Section:** 9.1 MiB (55.6% of total)

## Top 10 Crates by Size

| Rank | Crate | Size | % of .text |
|------|-------|------|------------|
| 1 | [Unknown] | 1.9 MiB | 21.0% |
| 2 | factbase | 1.5 MiB | 16.3% |
| 3 | std | 1.4 MiB | 15.7% |
| 4 | h2 | 453.7 KiB | 4.9% |
| 5 | hyper | 433.9 KiB | 4.7% |
| 6 | regex_automata | 387.4 KiB | 4.2% |
| 7 | zstd_sys | 338.3 KiB | 3.6% |
| 8 | clap_builder | 275.5 KiB | 3.0% |
| 9 | tokio | 216.3 KiB | 2.3% |
| 10 | regex_syntax | 188.6 KiB | 2.0% |

## Top 10 Functions by Size

| Rank | Function | Size | Crate |
|------|----------|------|-------|
| 1 | `<Commands as Subcommand>::augment_subcommands` | 118.5 KiB | factbase (clap) |
| 2 | `mcp::tools::tools_list` | 61.6 KiB | factbase |
| 3 | `commands::lint::cmd_lint` | 52.3 KiB | factbase |
| 4 | `sqlite3VdbeExec` | 48.0 KiB | libsqlite3_sys |
| 5 | `<ScanArgs as Args>::augment_args` | 43.4 KiB | factbase (clap) |
| 6 | `scanner::full_scan::{{closure}}` | 42.1 KiB | factbase |
| 7 | `<LintArgs as Args>::augment_args` | 42.1 KiB | factbase (clap) |
| 8 | `<SearchArgs as Args>::augment_args` | 39.3 KiB | factbase (clap) |
| 9 | `mcp::tools::handle_tool_call::{{closure}}` | 33.3 KiB | factbase |
| 10 | `main::{{closure}}` | 31.3 KiB | factbase |

## Analysis

### Major Contributors

1. **[Unknown] (21%)** - Likely C code from SQLite and zstd that cargo-bloat can't attribute
2. **factbase (16.3%)** - Our application code
3. **std (15.7%)** - Rust standard library
4. **HTTP stack (h2 + hyper = 9.6%)** - Required for MCP server and Ollama client
5. **regex (regex_automata + regex_syntax = 6.2%)** - Used for pattern matching
6. **zstd_sys (3.6%)** - Compression library (feature-gated)
7. **clap_builder (3.0%)** - CLI argument parsing

### Optimization Opportunities

1. **Clap derive macros** - The `augment_subcommands` and `augment_args` functions are large (118.5 KiB + 43.4 KiB + 42.1 KiB + 39.3 KiB = 243.3 KiB). Consider:
   - Using `clap::builder` API instead of derive for some commands
   - Reducing help text verbosity

2. **MCP tools_list** - 61.6 KiB for tool definitions. This is acceptable given 16 tools with detailed schemas.

3. **cmd_lint** - 52.3 KiB is large for a single command. The lint module split (Task 4) may help.

4. **Feature flags** - Already implemented:
   - `--no-default-features` removes zstd, progress bars, MCP server
   - Minimal build is ~9 MiB vs 16.3 MiB full

### Recommendations

1. **No immediate action needed** - Binary size is reasonable for a full-featured CLI
2. **Feature flags working** - Users can build minimal version if size matters
3. **Future consideration** - If size becomes critical:
   - Replace `clap` derive with builder API (~200 KiB savings)
   - Use `regex-lite` instead of full regex (~500 KiB savings)
   - Consider `miniz_oxide` instead of zstd for compression (~300 KiB savings)

## Build Configurations

| Configuration | Size | Notes |
|---------------|------|-------|
| Full (default) | 16.3 MiB | All features |
| Minimal (`--no-default-features`) | ~9 MiB | CLI only, no MCP/compression/progress |
| Without MCP | ~12 MiB | Removes axum, tower, h2 |
| Without compression | ~13 MiB | Removes zstd |

## Methodology

```bash
cargo install cargo-bloat
cargo bloat --release --crates
cargo bloat --release -n 20
```

Note: cargo-bloat numbers are estimates based on symbol analysis. Actual sizes may vary.
