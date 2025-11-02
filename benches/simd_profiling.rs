// SIMD profiling benchmark - identify hot paths for optimization
// Measures: binary search, bloom filter, key comparison

use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use seerdb::{BloomFilter, SSTableBuilder};
use std::time::Duration;
use tempfile::tempdir;

fn benchmark_binary_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_search");

    for size in [1_000, 10_000, 100_000] {
        group.throughput(Throughput::Elements(1));

        // Create SSTable with known keys
        let dir = tempdir().unwrap();
        let path = dir.path().join("bench.sst");

        let mut builder = SSTableBuilder::new();
        for i in 0..size {
            let key = format!("key_{:08}", i);
            let value = format!("value_{:08}", i);
            builder.add(Bytes::from(key), Bytes::from(value));
        }
        let mut sstable = builder.build(&path).unwrap();

        // Benchmark: existing keys (worst case - at end)
        let target_key = format!("key_{:08}", size - 1);
        group.bench_with_input(
            BenchmarkId::new("existing_key", size),
            &size,
            |b, _| {
                b.iter(|| {
                    black_box(sstable.get(target_key.as_bytes()).unwrap());
                });
            },
        );

        // Benchmark: missing keys
        let missing_key = format!("key_{:08}", size + 1000);
        group.bench_with_input(
            BenchmarkId::new("missing_key", size),
            &size,
            |b, _| {
                b.iter(|| {
                    black_box(sstable.get(missing_key.as_bytes()).unwrap());
                });
            },
        );
    }

    group.finish();
}

fn benchmark_bloom_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("bloom_filter");

    for size in [1_000, 10_000, 100_000] {
        group.throughput(Throughput::Elements(1));

        // Create bloom filter
        let mut bloom = BloomFilter::new(size, 0.01); // 1% FPR
        for i in 0..size {
            let key = format!("key_{:08}", i);
            bloom.insert(&key);
        }

        // Benchmark: positive lookups
        let existing_key = format!("key_{:08}", size - 1);
        group.bench_with_input(
            BenchmarkId::new("positive_lookup", size),
            &size,
            |b, _| {
                b.iter(|| {
                    black_box(bloom.contains(&existing_key));
                });
            },
        );

        // Benchmark: negative lookups
        let missing_key = format!("key_{:08}", size + 1000);
        group.bench_with_input(
            BenchmarkId::new("negative_lookup", size),
            &size,
            |b, _| {
                b.iter(|| {
                    black_box(bloom.contains(&missing_key));
                });
            },
        );
    }

    group.finish();
}

fn benchmark_key_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_comparison");
    group.throughput(Throughput::Bytes(16));

    // Different key sizes
    for key_size in [8, 16, 32, 64, 128] {
        let key1 = vec![0u8; key_size];
        let key2 = vec![1u8; key_size];

        group.bench_with_input(
            BenchmarkId::new("scalar_cmp", key_size),
            &key_size,
            |b, _| {
                b.iter(|| {
                    black_box(key1.cmp(&key2));
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = simd_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = benchmark_binary_search, benchmark_bloom_filter, benchmark_key_comparison
}

criterion_main!(simd_benches);
