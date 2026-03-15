//! Benchmarks for search operations
//!
//! Run with: cargo bench --bench search
//!
//! ## Baseline Performance (WSL2/Linux, AMD Ryzen, NVMe SSD)
//!
//! These benchmarks measure database-level search performance without Ollama.
//! Semantic search benchmarks use pre-generated random embeddings.
//!
//! ### Title Search (SQL LIKE pattern)
//! - 100 docs: ~24 µs
//! - 1000 docs: ~89 µs
//!
//! ### Content Search (grep-style)
//! - 100 docs: ~49 µs
//! - 1000 docs: ~269 µs
//!
//! ### Semantic Search (sqlite-vec KNN)
//! - 100 docs, limit 10: ~786 µs
//! - 1000 docs, limit 10: ~104 ms
//!
//! Note: Semantic search scales with document count due to KNN complexity.
//! Run `cargo bench --bench search -- --save-baseline <name>` to save results.

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use factbase::{ContentSearchParams, Database, Document, ProgressReporter, Repository};
use tempfile::TempDir;

/// Create a test database with N documents and embeddings
fn setup_database(num_docs: usize) -> (TempDir, Database) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("bench.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // Add a test repository
    let repo = Repository {
        id: "bench".to_string(),
        name: "Benchmark Repo".to_string(),
        path: temp_dir.path().to_path_buf(),
        perspective: None,
        created_at: Utc::now(),
        last_indexed_at: None,
        last_lint_at: None,
    };
    db.add_repository(&repo).expect("Failed to add repository");

    // Insert documents with embeddings
    for i in 0..num_docs {
        let id = format!("{:06x}", i);
        let title = format!("Document {} about topic {}", i, i % 10);
        let content = generate_document_content(i);
        let file_path = format!("docs/doc_{}.md", i);
        let doc_type = match i % 5 {
            0 => "person",
            1 => "project",
            2 => "concept",
            3 => "note",
            _ => "misc",
        };

        let doc = Document {
            id: id.clone(),
            repo_id: "bench".to_string(),
            file_path,
            file_hash: format!("hash_{}", i),
            title,
            doc_type: Some(doc_type.to_string()),
            content,
            file_modified_at: Some(Utc::now()),
            indexed_at: Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).expect("Failed to insert document");

        // Generate deterministic embedding (1024 dims)
        let embedding = generate_embedding(i);
        db.upsert_embedding(&id, &embedding)
            .expect("Failed to insert embedding");
    }

    (temp_dir, db)
}

/// Generate document content with searchable patterns
fn generate_document_content(index: usize) -> String {
    let topics = ["API", "database", "search", "performance", "testing"];
    let topic = topics[index % topics.len()];

    format!(
        "<!-- factbase:{:06x} -->\n# Document {}\n\n\
        This document discusses {} implementation details.\n\n\
        ## Overview\n\n\
        The {} system provides functionality for handling requests.\n\
        Key features include:\n\
        - Feature A for {}\n\
        - Feature B with optimizations\n\
        - Feature C supporting multiple backends\n\n\
        ## Implementation\n\n\
        The implementation uses pattern matching and caching.\n\
        Performance is critical for {} operations.\n\n\
        TODO: Add more documentation for {} edge cases.\n\
        FIXME: Handle error conditions in {} module.\n",
        index, index, topic, topic, topic, topic, topic, topic
    )
}

/// Generate deterministic embedding vector (1024 dimensions)
fn generate_embedding(seed: usize) -> Vec<f32> {
    // Use simple deterministic generation for benchmarks
    // Real embeddings would come from Ollama
    (0..1024)
        .map(|i| {
            let x = ((seed * 1000 + i) % 10000) as f32 / 10000.0;
            (x * 2.0 - 1.0) * 0.1 // Small values centered around 0
        })
        .collect()
}

fn bench_title_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("title_search");

    for num_docs in [100, 500, 1000] {
        let (_temp, db) = setup_database(num_docs);
        group.throughput(Throughput::Elements(num_docs as u64));

        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &db, |b, db| {
            b.iter(|| {
                db.search_by_title(black_box("Document"), 10, None, None)
                    .expect("Search failed")
            })
        });
    }

    group.finish();
}

fn bench_title_search_with_type_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("title_search_filtered");

    for num_docs in [100, 500, 1000] {
        let (_temp, db) = setup_database(num_docs);
        group.throughput(Throughput::Elements(num_docs as u64));

        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &db, |b, db| {
            b.iter(|| {
                db.search_by_title(black_box("Document"), 10, Some("person"), None)
                    .expect("Search failed")
            })
        });
    }

    group.finish();
}

fn bench_content_search_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_search_simple");

    for num_docs in [100, 500, 1000] {
        let (_temp, db) = setup_database(num_docs);
        group.throughput(Throughput::Elements(num_docs as u64));

        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &db, |b, db| {
            b.iter(|| {
                db.search_content(&ContentSearchParams {
                    pattern: black_box("TODO"),
                    limit: 10,
                    doc_type: None,
                    repo_id: None,
                    context_lines: 2,
                    since: None,
                    progress: &ProgressReporter::Silent,
                })
                .expect("Search failed")
            })
        });
    }

    group.finish();
}

fn bench_content_search_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_search_pattern");

    // Test different pattern complexities
    let patterns = [
        ("simple", "API"),
        ("multi_word", "performance"),
        ("case_mixed", "Implementation"),
    ];

    let (_temp, db) = setup_database(500);

    for (name, pattern) in patterns {
        group.bench_with_input(BenchmarkId::from_parameter(name), &pattern, |b, pattern| {
            b.iter(|| {
                db.search_content(&ContentSearchParams {
                    pattern: black_box(pattern),
                    limit: 10,
                    doc_type: None,
                    repo_id: None,
                    context_lines: 2,
                    since: None,
                    progress: &ProgressReporter::Silent,
                })
                .expect("Search failed")
            })
        });
    }

    group.finish();
}

fn bench_semantic_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_search");

    for num_docs in [100, 500, 1000] {
        let (_temp, db) = setup_database(num_docs);
        let query_embedding = generate_embedding(99999); // Different from any doc

        group.throughput(Throughput::Elements(num_docs as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_docs),
            &(&db, &query_embedding),
            |b, (db, embedding)| {
                b.iter(|| {
                    db.search_semantic_with_query(black_box(embedding), 10, None, None, None)
                        .expect("Search failed")
                })
            },
        );
    }

    group.finish();
}

fn bench_semantic_search_varying_limits(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_search_limits");

    let (_temp, db) = setup_database(500);
    let query_embedding = generate_embedding(99999);

    for limit in [5, 10, 25, 50] {
        group.bench_with_input(
            BenchmarkId::from_parameter(limit),
            &(&db, &query_embedding, limit),
            |b, (db, embedding, limit)| {
                b.iter(|| {
                    db.search_semantic_with_query(black_box(embedding), *limit, None, None, None)
                        .expect("Search failed")
                })
            },
        );
    }

    group.finish();
}

fn bench_semantic_search_with_filters(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_search_filtered");

    let (_temp, db) = setup_database(500);
    let query_embedding = generate_embedding(99999);

    // No filter
    group.bench_function("no_filter", |b| {
        b.iter(|| {
            db.search_semantic_with_query(black_box(&query_embedding), 10, None, None, None)
                .expect("Search failed")
        })
    });

    // Type filter
    group.bench_function("type_filter", |b| {
        b.iter(|| {
            db.search_semantic_with_query(
                black_box(&query_embedding),
                10,
                Some("person"),
                None,
                None,
            )
            .expect("Search failed")
        })
    });

    // Repo filter
    group.bench_function("repo_filter", |b| {
        b.iter(|| {
            db.search_semantic_with_query(
                black_box(&query_embedding),
                10,
                None,
                Some("bench"),
                None,
            )
            .expect("Search failed")
        })
    });

    // Both filters
    group.bench_function("both_filters", |b| {
        b.iter(|| {
            db.search_semantic_with_query(
                black_box(&query_embedding),
                10,
                Some("person"),
                Some("bench"),
                None,
            )
            .expect("Search failed")
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_title_search,
    bench_title_search_with_type_filter,
    bench_content_search_simple,
    bench_content_search_pattern,
    bench_semantic_search,
    bench_semantic_search_varying_limits,
    bench_semantic_search_with_filters,
);
criterion_main!(benches);
