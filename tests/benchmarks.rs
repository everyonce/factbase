//! Performance benchmarks for establishing baselines.
//! These tests measure and log performance metrics without pass/fail criteria.
//! Run with: cargo test benchmark --release -- --ignored --nocapture

mod common;

use common::ollama_helpers::require_ollama;
use common::run_scan;
use factbase::{config::Config, database::Database, embedding::OllamaEmbedding, EmbeddingProvider};
use std::fs;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Generate test documents of specified count
fn generate_test_docs(repo_path: &std::path::Path, count: usize) {
    fs::create_dir_all(repo_path.join("docs")).unwrap();
    for i in 0..count {
        let content = format!(
            "# Document {}\n\nThis is test document number {}.\n\n{}",
            i,
            i,
            "Lorem ipsum dolor sit amet. ".repeat(50)
        );
        fs::write(repo_path.join(format!("docs/doc{}.md", i)), content).unwrap();
    }
}

/// Benchmark 10.2: Scan performance by repo size
#[tokio::test]
#[ignore] // Requires Ollama
async fn benchmark_scan_performance() {
    require_ollama().await;

    println!("\n=== Scan Performance Benchmark ===\n");
    println!("| Docs | Total Time | Per-Doc Time | Docs/sec |");
    println!("|------|------------|--------------|----------|");

    for doc_count in [5, 10, 20] {
        // Reduced for faster testing
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().join("repo");
        generate_test_docs(&repo_path, doc_count);

        let db_path = temp_dir.path().join("test.db");
        let db = Database::new(&db_path).unwrap();

        let repo = common::test_repo("bench", repo_path);
        db.add_repository(&repo).unwrap();

        let config = Config::default();

        let start = Instant::now();
        let result = run_scan(&repo, &db, &config).await.unwrap();
        let elapsed = start.elapsed();

        let per_doc = elapsed / doc_count as u32;
        let docs_per_sec = doc_count as f64 / elapsed.as_secs_f64();

        println!(
            "| {:4} | {:>10.2?} | {:>12.2?} | {:>8.1} |",
            doc_count, elapsed, per_doc, docs_per_sec
        );

        assert_eq!(result.added, doc_count, "All docs should be indexed");
    }

    println!();
}

/// Benchmark 10.3: Search latency
#[tokio::test]
#[ignore] // Requires Ollama
async fn benchmark_search_latency() {
    require_ollama().await;

    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path().join("repo");
    generate_test_docs(&repo_path, 10); // Reduced for faster testing

    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    let repo = common::test_repo("bench", repo_path);
    db.add_repository(&repo).unwrap();

    let config = Config::default();

    // Index documents
    run_scan(&repo, &db, &config).await.unwrap();

    // Create embedding provider for search queries
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    println!("\n=== Search Latency Benchmark ===\n");

    let queries = [
        "test document",
        "lorem ipsum",
        "software engineering",
        "database performance",
        "machine learning",
    ];

    let mut latencies: Vec<Duration> = Vec::new();

    for query in &queries {
        let start = Instant::now();
        let query_emb = embedding.generate(query).await.unwrap();
        let embed_time = start.elapsed();

        let search_start = Instant::now();
        let results = db
            .search_semantic_with_query(&query_emb, 10, None, None, None)
            .unwrap();
        let search_time = search_start.elapsed();

        let total = start.elapsed();
        latencies.push(total);

        println!(
            "Query: '{}' -> {} results in {:?} (embed: {:?}, search: {:?})",
            query,
            results.len(),
            total,
            embed_time,
            search_time
        );
    }

    // Calculate statistics
    latencies.sort();
    let min = latencies.first().unwrap();
    let max = latencies.last().unwrap();
    let avg = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let p50 = &latencies[latencies.len() / 2];

    println!("\nStatistics:");
    println!("  Min: {:?}", min);
    println!("  Max: {:?}", max);
    println!("  Avg: {:?}", avg);
    println!("  P50: {:?}", p50);
    println!();
}

/// Benchmark 10.4: Embedding generation performance
#[tokio::test]
#[ignore] // Requires Ollama
async fn benchmark_embedding_generation() {
    require_ollama().await;

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    println!("\n=== Embedding Generation Benchmark ===\n");
    println!("| Words | Chars | Time | Chars/sec |");
    println!("|-------|-------|------|-----------|");

    let word = "lorem ";
    for word_count in [100, 500, 1000, 2000] {
        let text = word.repeat(word_count);
        let char_count = text.len();

        let start = Instant::now();
        let emb = embedding.generate(&text).await.unwrap();
        let elapsed = start.elapsed();

        let chars_per_sec = char_count as f64 / elapsed.as_secs_f64();

        println!(
            "| {:5} | {:5} | {:>4.2?} | {:>9.0} |",
            word_count, char_count, elapsed, chars_per_sec
        );

        assert_eq!(emb.len(), config.embedding.dimension);
    }

    println!();
}

/// Benchmark 10.5: Batch vs individual embedding
#[tokio::test]
#[ignore] // Requires Ollama
async fn benchmark_batch_embedding() {
    require_ollama().await;

    let config = Config::default();
    let embedding = OllamaEmbedding::new(
        &config.embedding.base_url,
        &config.embedding.model,
        config.embedding.dimension,
    );

    println!("\n=== Batch vs Individual Embedding Benchmark ===\n");

    let texts: Vec<String> = (0..10)
        .map(|i| format!("This is test document number {} with some content.", i))
        .collect();
    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

    // Individual embedding
    let start = Instant::now();
    for text in &texts {
        embedding.generate(text).await.unwrap();
    }
    let individual_time = start.elapsed();

    // Batch embedding
    let start = Instant::now();
    let batch_results = embedding.generate_batch(&text_refs).await.unwrap();
    let batch_time = start.elapsed();

    println!("10 documents:");
    println!(
        "  Individual: {:?} ({:.1?}/doc)",
        individual_time,
        individual_time / 10
    );
    println!(
        "  Batch:      {:?} ({:.1?}/doc)",
        batch_time,
        batch_time / 10
    );
    println!(
        "  Speedup:    {:.1}x",
        individual_time.as_secs_f64() / batch_time.as_secs_f64()
    );
    println!();

    assert_eq!(batch_results.len(), 10);
}

/// Benchmark: Parallel vs Sequential lint performance
/// This benchmark compares lint performance with and without the --parallel flag
#[test]
#[ignore] // CPU-intensive benchmark
fn benchmark_lint_parallel() {
    use factbase::{
        calculate_fact_stats, parse_source_definitions, parse_source_references,
        parse_temporal_tags, validate_temporal_tags,
    };
    use rayon::prelude::*;

    println!("\n=== Lint Parallel vs Sequential Benchmark ===\n");

    // Generate test documents with temporal tags and sources
    let docs: Vec<String> = (0..100)
        .map(|i| {
            format!(
                "# Document {}\n\n\
                - Fact one @t[2020..2022] [^1]\n\
                - Fact two @t[2023..] [^2]\n\
                - Fact three without tag\n\
                - Fact four @t[=2024-01] [^3]\n\
                - Fact five @t[~2024-06]\n\
                {}\n\n\
                ---\n\
                [^1]: LinkedIn profile, 2024-01-15\n\
                [^2]: Press release, 2023-03-01\n\
                [^3]: News article, 2024-01-20\n",
                i,
                "Lorem ipsum dolor sit amet. ".repeat(20)
            )
        })
        .collect();

    // Sequential processing
    let start = Instant::now();
    for doc in &docs {
        let _ = calculate_fact_stats(doc);
        let _ = parse_temporal_tags(doc);
        let _ = validate_temporal_tags(doc);
        let _ = parse_source_references(doc);
        let _ = parse_source_definitions(doc);
    }
    let sequential_time = start.elapsed();

    // Parallel processing
    let start = Instant::now();
    docs.par_iter().for_each(|doc| {
        let _ = calculate_fact_stats(doc);
        let _ = parse_temporal_tags(doc);
        let _ = validate_temporal_tags(doc);
        let _ = parse_source_references(doc);
        let _ = parse_source_definitions(doc);
    });
    let parallel_time = start.elapsed();

    println!("100 documents with temporal tags and sources:");
    println!(
        "  Sequential: {:?} ({:.2?}/doc)",
        sequential_time,
        sequential_time / 100
    );
    println!(
        "  Parallel:   {:?} ({:.2?}/doc)",
        parallel_time,
        parallel_time / 100
    );
    let speedup = sequential_time.as_secs_f64() / parallel_time.as_secs_f64();
    println!("  Speedup:    {:.2}x", speedup);
    println!();

    // Parallel should be faster on multi-core systems
    // On single-core, it may be slightly slower due to overhead
    assert!(
        parallel_time < sequential_time * 2,
        "Parallel should not be significantly slower than sequential"
    );
}

/// Benchmark summary
#[tokio::test]
#[ignore] // Requires Ollama
async fn benchmark_summary() {
    require_ollama().await;

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           FACTBASE PERFORMANCE BASELINE SUMMARY              ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Run individual benchmarks for detailed metrics:              ║");
    println!("║   cargo test benchmark_scan --release -- --nocapture         ║");
    println!("║   cargo test benchmark_search --release -- --nocapture       ║");
    println!("║   cargo test benchmark_embedding --release -- --nocapture    ║");
    println!("║   cargo test benchmark_batch --release -- --nocapture        ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
}
