//! Benchmarks for parsing operations
//!
//! Run with: cargo bench --bench parsing
//!
//! ## Baseline Performance (WSL2/Linux, AMD Ryzen, NVMe SSD)
//!
//! ### Temporal Tag Parsing
//! - 10 tags: 4.3µs
//! - 50 tags: 22µs
//! - 100 tags: 44µs
//! - 500 tags: 219µs
//! - 1000 tags: 431µs
//!
//! ### Source Reference Parsing
//! - 10 refs: 1.6µs
//! - 50 refs: 6.5µs
//! - 100 refs: 12.7µs
//! - 500 refs: 64µs
//! - 1000 refs: 129µs
//!
//! ### Source Definition Parsing
//! - 10 defs: 5.9µs
//! - 50 defs: 31µs
//! - 100 defs: 62µs
//! - 500 defs: 300µs
//! - 1000 defs: 605µs
//!
//! ### Fact Stats Calculation
//! - 10 facts: 1.1µs
//! - 50 facts: 5.1µs
//! - 100 facts: 10µs
//! - 500 facts: 51µs
//! - 1000 facts: 102µs
//!
//! ### Review Queue Parsing
//! - 5 questions: 2.5µs
//! - 10 questions: 4.8µs
//! - 25 questions: 11µs
//! - 50 questions: 22µs
//! - 100 questions: 44µs
//!
//! ### Document Chunking (8K chunk size, 200 overlap)
//! - 1K chars: 16ns (no chunking needed, single chunk)
//! - 10K chars: 116ns (2 chunks)
//! - 100K chars: 1.4µs (13 chunks)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use factbase::{
    calculate_fact_stats, chunk_document, parse_review_queue, parse_source_definitions,
    parse_source_references, parse_temporal_tags,
};

/// Generate a document with N facts, each with temporal tags and source references
fn generate_document(num_facts: usize) -> String {
    let mut content = String::from("<!-- factbase:abc123 -->\n# Test Document\n\n");

    for i in 1..=num_facts {
        let year = 2020 + (i % 5);
        let end_year = year + 1 + (i % 3);
        content.push_str(&format!(
            "- Fact number {} with temporal tag @t[{}..{}] [^{}]\n",
            i, year, end_year, i
        ));
    }

    content.push_str("\n---\n\n");

    for i in 1..=num_facts {
        let month = 1 + (i % 12);
        let day = 1 + (i % 28);
        content.push_str(&format!(
            "[^{}]: LinkedIn profile, scraped 2024-{:02}-{:02}\n",
            i, month, day
        ));
    }

    content
}

/// Generate a document with review queue questions
fn generate_document_with_review_queue(num_questions: usize) -> String {
    let mut content = String::from("<!-- factbase:abc123 -->\n# Test Document\n\n");
    content.push_str("Some content here.\n\n");
    content.push_str("<!-- factbase:review -->\n## Review Queue\n\n");

    let types = [
        "temporal",
        "conflict",
        "missing",
        "ambiguous",
        "stale",
        "duplicate",
    ];
    for i in 1..=num_questions {
        let qtype = types[i % types.len()];
        let checked = if i % 3 == 0 { "x" } else { " " };
        content.push_str(&format!(
            "- [{}] `@q[{}]` Line {}: \"Fact {}\" - question about this fact?\n",
            checked, qtype, i, i
        ));
        if i % 3 == 0 {
            content.push_str(&format!("  > Answer for question {}\n", i));
        }
    }

    content
}

/// Generate a document of approximately N characters
fn generate_document_by_size(target_chars: usize) -> String {
    let mut content = String::from("<!-- factbase:abc123 -->\n# Test Document\n\n");

    // Each paragraph is roughly 200 chars
    let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.\n\n";

    while content.len() < target_chars {
        content.push_str(paragraph);
    }

    content.truncate(target_chars);
    content
}

fn bench_parse_temporal_tags(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_temporal_tags");

    for size in [10, 50, 100, 500, 1000].iter() {
        let doc = generate_document(*size);
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, doc| {
            b.iter(|| parse_temporal_tags(black_box(doc)))
        });
    }

    group.finish();
}

fn bench_parse_source_references(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_source_references");

    for size in [10, 50, 100, 500, 1000].iter() {
        let doc = generate_document(*size);
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, doc| {
            b.iter(|| parse_source_references(black_box(doc)))
        });
    }

    group.finish();
}

fn bench_parse_source_definitions(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_source_definitions");

    for size in [10, 50, 100, 500, 1000].iter() {
        let doc = generate_document(*size);
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, doc| {
            b.iter(|| parse_source_definitions(black_box(doc)))
        });
    }

    group.finish();
}

fn bench_calculate_fact_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("calculate_fact_stats");

    for size in [10, 50, 100, 500, 1000].iter() {
        let doc = generate_document(*size);
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, doc| {
            b.iter(|| calculate_fact_stats(black_box(doc)))
        });
    }

    group.finish();
}

fn bench_parse_review_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_review_queue");

    for size in [5, 10, 25, 50, 100].iter() {
        let doc = generate_document_with_review_queue(*size);
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, doc| {
            b.iter(|| parse_review_queue(black_box(doc)))
        });
    }

    group.finish();
}

fn bench_chunk_document(c: &mut Criterion) {
    let mut group = c.benchmark_group("chunk_document");

    // Standard chunking parameters (from config defaults)
    let chunk_size = 8000;
    let overlap = 200;

    for size in [1_000, 10_000, 100_000].iter() {
        let doc = generate_document_by_size(*size);
        let label = match *size {
            1_000 => "1K",
            10_000 => "10K",
            100_000 => "100K",
            _ => "unknown",
        };
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &doc, |b, doc| {
            b.iter(|| chunk_document(black_box(doc), chunk_size, overlap))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_temporal_tags,
    bench_parse_source_references,
    bench_parse_source_definitions,
    bench_calculate_fact_stats,
    bench_parse_review_queue,
    bench_chunk_document,
);
criterion_main!(benches);
