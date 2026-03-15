//! Benchmarks for link fetching operations
//!
//! Run with: cargo bench --bench links
//!
//! ## Baseline Performance (WSL2/Linux, AMD Ryzen, NVMe SSD)
//!
//! These benchmarks compare N+1 query pattern vs batch fetching.
//! Each document has 2 outgoing links (circular graph).
//!
//! ### N+1 Pattern (get_links_from + get_links_to per document)
//! - 10 docs: ~33 µs (20 queries)
//! - 100 docs: ~350 µs (200 queries)
//! - 1000 docs: ~3.6 ms (2000 queries)
//!
//! ### Batch Pattern (get_links_for_documents)
//! - 10 docs: ~23 µs (2 queries) - 1.4x faster
//! - 100 docs: ~196 µs (2 queries) - 1.8x faster
//! - 1000 docs: ~2.0 ms (2 queries) - 1.8x faster
//!
//! The batch pattern shows consistent ~1.8x improvement at scale.
//! Run `cargo bench --bench links -- --save-baseline <name>` to save results.

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use factbase::{Database, DetectedLink, Document, Repository};
use tempfile::TempDir;

/// Create a test database with N documents and links between them
fn setup_database(num_docs: usize) -> (TempDir, Database, Vec<String>) {
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

    let mut doc_ids = Vec::with_capacity(num_docs);

    // Insert documents
    for i in 0..num_docs {
        let id = format!("{:06x}", i);
        doc_ids.push(id.clone());

        let doc = Document {
            id: id.clone(),
            repo_id: "bench".to_string(),
            file_path: format!("docs/doc_{}.md", i),
            file_hash: format!("hash_{}", i),
            title: format!("Document {}", i),
            doc_type: Some("note".to_string()),
            content: format!(
                "<!-- factbase:{} -->\n# Document {}\n\nContent here.",
                id, i
            ),
            file_modified_at: Some(Utc::now()),
            indexed_at: Utc::now(),
            is_deleted: false,
        };
        db.upsert_document(&doc).expect("Failed to insert document");
    }

    // Create links: each document links to 2-3 others (creating a connected graph)
    for i in 0..num_docs {
        let source_id = &doc_ids[i];
        let mut links = Vec::new();

        // Link to next document (circular)
        let target1_idx = (i + 1) % num_docs;
        let target1 = &doc_ids[target1_idx];
        links.push(DetectedLink {
            target_id: target1.clone(),
            target_title: format!("Document {}", target1_idx),
            mention_text: format!("doc {}", target1_idx),
            context: format!("Reference to doc {}", target1_idx),
        });

        // Link to document 5 positions ahead (if enough docs)
        if num_docs > 5 {
            let target2_idx = (i + 5) % num_docs;
            let target2 = &doc_ids[target2_idx];
            links.push(DetectedLink {
                target_id: target2.clone(),
                target_title: format!("Document {}", target2_idx),
                mention_text: format!("doc {}", target2_idx),
                context: format!("Reference to doc {}", target2_idx),
            });
        }

        db.update_links(source_id, &links)
            .expect("Failed to insert links");
    }

    (temp_dir, db, doc_ids)
}

/// Benchmark N+1 pattern: call get_links_from and get_links_to for each document
fn bench_n_plus_one(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_fetch_n_plus_one");

    for num_docs in [10, 100, 1000] {
        let (_temp_dir, db, doc_ids) = setup_database(num_docs);

        group.throughput(Throughput::Elements(num_docs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &num_docs, |b, _| {
            b.iter(|| {
                for id in &doc_ids {
                    let _outgoing = black_box(db.get_links_from(id).unwrap());
                    let _incoming = black_box(db.get_links_to(id).unwrap());
                }
            });
        });
    }

    group.finish();
}

/// Benchmark batch pattern: single call to get_links_for_documents
fn bench_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_fetch_batch");

    for num_docs in [10, 100, 1000] {
        let (_temp_dir, db, doc_ids) = setup_database(num_docs);
        let doc_id_refs: Vec<&str> = doc_ids.iter().map(|s| s.as_str()).collect();

        group.throughput(Throughput::Elements(num_docs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_docs), &num_docs, |b, _| {
            b.iter(|| {
                let _links = black_box(db.get_links_for_documents(&doc_id_refs).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark comparison: same workload, different approaches
fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_fetch_comparison");

    // Use 100 docs as representative workload
    let num_docs = 100;
    let (_temp_dir, db, doc_ids) = setup_database(num_docs);
    let doc_id_refs: Vec<&str> = doc_ids.iter().map(|s| s.as_str()).collect();

    group.bench_function("n_plus_one_100", |b| {
        b.iter(|| {
            for id in &doc_ids {
                let _outgoing = black_box(db.get_links_from(id).unwrap());
                let _incoming = black_box(db.get_links_to(id).unwrap());
            }
        });
    });

    group.bench_function("batch_100", |b| {
        b.iter(|| {
            let _links = black_box(db.get_links_for_documents(&doc_id_refs).unwrap());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_n_plus_one, bench_batch, bench_comparison);
criterion_main!(benches);
