//! Hot path: indexing a 50 MB body (F06-AC10).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::hint::black_box;
use std::num::NonZeroU32;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferry_core::text::LineIndex;

fn body_50mb() -> Vec<u8> {
    let line = br#"  {"id": 123456, "name": "ferry", "tags": ["a", "b"], "ok": true},"#;
    let mut v = Vec::with_capacity(50 * 1024 * 1024);
    while v.len() < 50 * 1024 * 1024 {
        v.extend_from_slice(line);
        v.push(b'\n');
    }
    v
}

fn bench(c: &mut Criterion) {
    let body = body_50mb();
    let wrap = NonZeroU32::new(4096).unwrap_or(NonZeroU32::MIN);
    let mut g = c.benchmark_group("line_index");
    g.throughput(Throughput::Bytes(body.len() as u64));
    g.sample_size(10);
    g.bench_function("build_50mb", |b| {
        b.iter(|| LineIndex::build(black_box(&body), wrap));
    });
    let minified: Vec<u8> = body.iter().copied().filter(|&b| b != b'\n').collect();
    g.bench_function("build_50mb_single_line", |b| {
        b.iter(|| LineIndex::build(black_box(&minified), wrap));
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
