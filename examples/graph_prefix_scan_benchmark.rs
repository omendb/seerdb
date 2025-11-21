// Benchmark to simulate Graph HNSW edge storage pattern
// Tests prefix scan performance with data spread across multiple SSTables
// This is the workload where block cache should provide 10-20x improvement

use seerdb::{DBOptions, SyncPolicy, DB};
use std::time::Instant;
use tempfile::tempdir;

const NUM_NODES: usize = 1000;
const EDGES_PER_NODE: usize = 32;
const NUM_LEVELS: usize = 4;
const VALUE_SIZE: usize = 64;

fn main() {
    println!("=== Graph Prefix Scan Benchmark ===");
    println!("Simulating HNSW graph edge storage pattern\n");

    let dir = tempdir().unwrap();
    let opts = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 8 * 1024 * 1024, // 8MB to force frequent flushes
        wal_sync_policy: SyncPolicy::None,
        background_compaction: false,
        block_cache_capacity: 16_384, // 64MB cache
        ..Default::default()
    };

    let db = DB::open(opts).unwrap();
    let value = vec![0u8; VALUE_SIZE];

    println!("Phase 1: Building HNSW graph structure...");
    println!(
        "  {} nodes × {} edges × {} levels = {} total entries",
        NUM_NODES,
        EDGES_PER_NODE,
        NUM_LEVELS,
        NUM_NODES * EDGES_PER_NODE * NUM_LEVELS
    );

    let write_start = Instant::now();
    let mut flush_count = 0;

    // Simulate HNSW edge storage: key = node_id || level || neighbor_id
    for node_id in 0..NUM_NODES {
        for level in 0..NUM_LEVELS {
            for edge in 0..EDGES_PER_NODE {
                let key = format!("node:{:06}:L{}:edge:{:04}", node_id, level, edge);
                db.put(key.as_bytes(), &value).unwrap();
            }
        }

        // Flush periodically to create multiple SSTables
        if (node_id + 1) % 100 == 0 {
            db.flush().unwrap();
            flush_count += 1;
            print!(".");
        }
    }
    db.flush().unwrap();
    flush_count += 1;
    println!("\n  Created {} SSTables", flush_count);

    let write_duration = write_start.elapsed();
    let write_throughput =
        (NUM_NODES * EDGES_PER_NODE * NUM_LEVELS) as f64 / write_duration.as_secs_f64();
    println!(
        "  Write time: {:.2}s ({:.0} ops/sec)\n",
        write_duration.as_secs_f64(),
        write_throughput
    );

    // Get stats after writes
    let stats = db.stats();
    println!("Database State:");
    println!("  Total SSTables: {}", stats.total_sstables);
    println!("  SSTable levels: {:?}", stats.sstables_per_level);
    println!(
        "  Block cache size: {} blocks ({:.2} MB)",
        stats.block_cache_size,
        (stats.block_cache_size * 4096) as f64 / 1024.0 / 1024.0
    );
    println!();

    // Phase 2: Prefix scans (simulate HNSW neighbor lookup)
    println!("Phase 2: Prefix scans (neighbor lookups)...\n");

    // Test 1: Cold cache - first scan of each node
    println!(
        "Test 1: Cold Cache - First scan of {} nodes",
        NUM_NODES / 10
    );
    let stats_before = db.stats();
    let start = Instant::now();
    let mut total_edges_found = 0;

    for node_id in (0..NUM_NODES).step_by(10) {
        let prefix = format!("node:{:06}:", node_id);
        let results: Vec<_> = db.prefix(prefix.as_bytes()).unwrap().collect();
        total_edges_found += results.len();
    }

    let duration = start.elapsed();
    let stats_after = db.stats();
    let scans_per_sec = (NUM_NODES / 10) as f64 / duration.as_secs_f64();
    let new_hits = stats_after.cache_hits - stats_before.cache_hits;
    let new_misses = stats_after.cache_misses - stats_before.cache_misses;
    let hit_rate = if new_hits + new_misses > 0 {
        new_hits as f64 / (new_hits + new_misses) as f64 * 100.0
    } else {
        0.0
    };

    println!("  Scans: {} nodes", NUM_NODES / 10);
    println!("  Edges found: {}", total_edges_found);
    println!("  Throughput: {:.0} scans/sec", scans_per_sec);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Cache hits: {} (new)", new_hits);
    println!("  Cache misses: {} (new)", new_misses);
    println!("  Cache hit rate: {:.2}%", hit_rate);
    println!();

    // Test 2: Hot cache - repeated scans of same nodes
    println!("Test 2: Hot Cache - Repeated scans (100 nodes × 10 iterations)");
    let stats_before = db.stats();
    let start = Instant::now();
    let mut total_edges_found = 0;

    for _ in 0..10 {
        for node_id in 0..100 {
            let prefix = format!("node:{:06}:", node_id);
            let results: Vec<_> = db.prefix(prefix.as_bytes()).unwrap().collect();
            total_edges_found += results.len();
        }
    }

    let duration = start.elapsed();
    let stats_after = db.stats();
    let scans_per_sec = 1000.0 / duration.as_secs_f64();
    let new_hits = stats_after.cache_hits - stats_before.cache_hits;
    let new_misses = stats_after.cache_misses - stats_before.cache_misses;
    let hit_rate = if new_hits + new_misses > 0 {
        new_hits as f64 / (new_hits + new_misses) as f64 * 100.0
    } else {
        0.0
    };

    println!("  Scans: 1000");
    println!("  Edges found: {}", total_edges_found);
    println!("  Throughput: {:.0} scans/sec", scans_per_sec);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Cache hits: {} (new)", new_hits);
    println!("  Cache misses: {} (new)", new_misses);
    println!("  Cache hit rate: {:.2}%", hit_rate);
    println!();

    // Test 3: Random access pattern (simulate actual HNSW traversal)
    println!("Test 3: Random Access Pattern (1000 random node lookups)");
    let stats_before = db.stats();
    let start = Instant::now();
    let mut total_edges_found = 0;

    for i in 0..1000 {
        let node_id = (i * 7919) % NUM_NODES; // Pseudo-random
        let prefix = format!("node:{:06}:", node_id);
        let results: Vec<_> = db.prefix(prefix.as_bytes()).unwrap().collect();
        total_edges_found += results.len();
    }

    let duration = start.elapsed();
    let stats_after = db.stats();
    let scans_per_sec = 1000.0 / duration.as_secs_f64();
    let new_hits = stats_after.cache_hits - stats_before.cache_hits;
    let new_misses = stats_after.cache_misses - stats_before.cache_misses;
    let hit_rate = if new_hits + new_misses > 0 {
        new_hits as f64 / (new_hits + new_misses) as f64 * 100.0
    } else {
        0.0
    };

    println!("  Scans: 1000");
    println!("  Edges found: {}", total_edges_found);
    println!("  Throughput: {:.0} scans/sec", scans_per_sec);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Cache hits: {} (new)", new_hits);
    println!("  Cache misses: {} (new)", new_misses);
    println!("  Cache hit rate: {:.2}%", hit_rate);
    println!();

    // Final summary
    let final_stats = db.stats();
    println!("=== Final Summary ===");
    println!("Total cache hits: {}", final_stats.cache_hits);
    println!("Total cache misses: {}", final_stats.cache_misses);
    println!(
        "Overall cache hit rate: {:.2}%",
        final_stats.cache_hit_rate * 100.0
    );
    println!();
    println!("Block Cache Status:");
    println!(
        "  Size: {} / {} blocks ({:.2}% full)",
        final_stats.block_cache_size,
        final_stats.block_cache_capacity,
        (final_stats.block_cache_size as f64 / final_stats.block_cache_capacity as f64) * 100.0
    );
    println!(
        "  Memory: {:.2} MB / {:.2} MB",
        (final_stats.block_cache_size * 4096) as f64 / 1024.0 / 1024.0,
        (final_stats.block_cache_capacity * 4096) as f64 / 1024.0 / 1024.0
    );
    println!();

    println!("Performance Analysis:");
    println!("  Target: >200 scans/sec for graph workloads (was 22 QPS baseline)");
    println!("  Cold cache should be slower (disk I/O)");
    println!("  Hot cache should be much faster (10-20x improvement expected)");
    println!("  Random access shows real-world HNSW traversal pattern");
}
