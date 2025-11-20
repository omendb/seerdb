use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use seerdb::sstable::block::{Block, BlockBuilder};

fn bench_block_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_parsing_full_cycle");

    // Prepare data
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for i in 0..100 {
        keys.push(format!("key{:04}", i).into_bytes());
        values.push(vec![b'x'; 100]); // 100 bytes value
    }

    // 1. Compressed Block
    let mut builder = BlockBuilder::new();
    builder.set_compression(true);
    for (k, v) in keys.iter().zip(values.iter()) {
        builder.add(k, v);
    }
    let compressed_bytes = builder.finish();

    // 2. Uncompressed Block
    let mut builder = BlockBuilder::new();
    builder.set_compression(false);
    for (k, v) in keys.iter().zip(values.iter()) {
        builder.add(k, v);
    }
    let uncompressed_bytes = builder.finish();
    
    println!("Compressed size: {}", compressed_bytes.len());
    println!("Uncompressed size: {}", uncompressed_bytes.len());

    group.bench_function("compressed_parse_iter", |b| {
        b.iter(|| {
            let block = Block::from_bytes(black_box(compressed_bytes.clone())).unwrap();
            black_box(block.iter().count());
        })
    });

    group.bench_function("uncompressed_parse_iter", |b| {
        b.iter(|| {
            // In real zero-copy, we don't clone bytes, but for fair comparison 
            // with from_bytes API we do. The main difference is decompression vs no-decompression
            // and allocation vs slicing.
            let block = Block::from_bytes(black_box(uncompressed_bytes.clone())).unwrap();
            black_box(block.iter().count());
        })
    });

    group.finish();
}

criterion_group!(benches, bench_block_parsing);
criterion_main!(benches);
