// Benchmark to test read-ahead prefetching effectiveness
// Larger dataset to show benefits of sequential block prefetching

use seerdb::{DBOptions, SyncPolicy, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_NODES: usize = 10_000;  // 10x larger
const EDGES_PER_NODE: usize = 64;  // 2x larger
const NUM_LEVELS: usize = 4;
const VALUE_SIZE: usize = 128;  // 2x larger

fn main() {
    println!("=== Read-Ahead Prefetching Benchmark ===");
    println!("Testing sequential scan performance with larger dataset\n");

    let dir = tempdir().unwrap();
    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024,  // Smaller to create more SSTables
        wal_sync_policy: SyncPolicy::None,
        background_compaction: false,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!("Phase 1: Building graph...");
    println!("  {} nodes × {} edges × {} levels = {} entries",
             NUM_NODES, EDGES_PER_NODE, NUM_LEVELS,
             NUM_NODES * EDGES_PER_NODE * NUM_LEVELS);

    let write_start = Instant::now();
    let mut flush_count = 0;

    for node_id in 0..NUM_NODES {
        for level in 0..NUM_LEVELS {
            for edge in 0..EDGES_PER_NODE {
                let key = format!("node:{:08}:L{}:edge:{:06}", node_id, level, edge);
                db.put(key.as_bytes(), &value).unwrap();
            }
        }

        if (node_id + 1) % 50 == 0 {
            db.flush().unwrap();
            flush_count += 1;
            if (node_id + 1) % 500 == 0 {
                print!(".");
            }
        }
    }
    db.flush().unwrap();
    flush_count += 1;
    println!("\n  Created {} SSTables in {:.2}s", flush_count, write_start.elapsed().as_secs_f64());

    let stats = db.stats();
    println!("  Total SSTables: {}", stats.total_sstables);
    println!();

    // Test: Sequential prefix scans (where read-ahead helps)
    println!("Test: Sequential Prefix Scans (500 nodes in order)");
    let stats_before = db.stats();
    let start = Instant::now();
    let mut total_edges = 0;

    for node_id in 0..500 {
        let prefix = format!("node:{:08}:", node_id);
        let count = db.prefix(prefix.as_bytes()).unwrap().count();
        total_edges += count;
    }

    let duration = start.elapsed();
    let stats_after = db.stats();
    let scans_per_sec = 500.0 / duration.as_secs_f64();
    let new_hits = stats_after.cache_hits - stats_before.cache_hits;
    let new_misses = stats_after.cache_misses - stats_before.cache_misses;
    let hit_rate = new_hits as f64 / (new_hits + new_misses) as f64 * 100.0;

    println!("  Throughput: {:.0} scans/sec", scans_per_sec);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Edges found: {}", total_edges);
    println!("  Cache hits: {}", new_hits);
    println!("  Cache misses: {}", new_misses);
    println!("  Cache hit rate: {:.2}%", hit_rate);
    println!();

    println!("Expected with read-ahead: Higher cache hit rate, faster throughput");
    println!("Expected without read-ahead: More cache misses, sequential I/O waits");
}
