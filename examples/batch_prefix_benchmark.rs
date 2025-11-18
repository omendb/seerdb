use seerdb::{DBOptions, DB};
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    println!("=== Batch Prefix API Benchmark ===\n");
    println!("Simulating HNSW graph traversal workload:");
    println!("- 10K nodes");
    println!("- ~60 neighbors per node");
    println!("- 18 node visits per query (typical search)\n");

    let dir = tempdir().unwrap();
    let options = DBOptions {
        data_dir: dir.path().to_path_buf(),
        memtable_capacity: 32 * 1024 * 1024,
        block_cache_capacity: 16_384,
        ..Default::default()
    };

    let db = DB::open(options).unwrap();

    println!("Creating graph structure (10K nodes × 60 neighbors)...");
    let num_nodes = 10_000u64;
    let neighbors_per_node = 60u32;

    let start = Instant::now();
    for node_id in 0..num_nodes {
        for neighbor_idx in 0..neighbors_per_node {
            let key = format!("node:{:010}:neighbor:{:05}", node_id, neighbor_idx);
            let value = format!("neighbor_data_{}", neighbor_idx);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        if (node_id + 1) % 1000 == 0 {
            print!("\rProgress: {}/{} nodes", node_id + 1, num_nodes);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }
    println!("\nData created in {:?}", start.elapsed());

    db.flush().unwrap();
    println!("Flushed to SSTables\n");

    let nodes_to_visit = 18usize;
    let selected_nodes: Vec<u64> = (0..nodes_to_visit)
        .map(|i| ((i as u64) * (num_nodes / nodes_to_visit as u64)) % num_nodes)
        .collect();

    println!("--- Baseline: Individual prefix scans ---");
    let mut total_results = 0usize;
    let mut individual_times = Vec::new();

    for trial in 0..10 {
        let start = Instant::now();

        for &node_id in &selected_nodes {
            let prefix = format!("node:{:010}:", node_id);
            let results: Vec<_> = db
                .prefix(prefix.as_bytes())
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            total_results += results.len();
        }

        let elapsed = start.elapsed();
        individual_times.push(elapsed);

        if trial == 0 {
            println!("Trial {}: {:?} ({} neighbors found)", trial + 1, elapsed, total_results / (trial + 1));
        }
    }

    let individual_median = median_duration(&individual_times);
    println!("Median time (10 trials): {:?}", individual_median);
    println!("Avg neighbors per query: {}\n", total_results / (10 * nodes_to_visit));

    println!("--- Optimized: Batch prefix scan ---");
    total_results = 0;
    let mut batch_times = Vec::new();

    for trial in 0..10 {
        let prefixes: Vec<Vec<u8>> = selected_nodes
            .iter()
            .map(|&node_id| format!("node:{:010}:", node_id).into_bytes())
            .collect();

        let prefix_refs: Vec<&[u8]> = prefixes.iter().map(|p| p.as_slice()).collect();

        let start = Instant::now();
        let results = db.prefix_batch(&prefix_refs).unwrap();
        let elapsed = start.elapsed();

        for result_set in &results {
            total_results += result_set.len();
        }

        batch_times.push(elapsed);

        if trial == 0 {
            println!("Trial {}: {:?} ({} neighbors found)", trial + 1, elapsed, total_results / (trial + 1));
        }
    }

    let batch_median = median_duration(&batch_times);
    println!("Median time (10 trials): {:?}", batch_median);
    println!("Avg neighbors per query: {}\n", total_results / (10 * nodes_to_visit));

    let stats = db.stats();
    println!("--- Database Stats ---");
    println!("Cache hit rate: {:.2}%", stats.cache_hit_rate * 100.0);
    println!("Cache hits: {}", stats.cache_hits);
    println!("Cache misses: {}", stats.cache_misses);
    println!("Block cache size: {} / {}", stats.block_cache_size, stats.block_cache_capacity);

    let speedup = individual_median.as_secs_f64() / batch_median.as_secs_f64();
    println!("\n=== Results ===");
    println!("Individual scans: {:?}", individual_median);
    println!("Batch scans:      {:?}", batch_median);
    println!("Speedup:          {:.2}x", speedup);

    if speedup >= 3.0 {
        println!("\n✅ SUCCESS: Achieved {:.2}x speedup (target: 3-5x)", speedup);
    } else if speedup >= 2.0 {
        println!("\n⚠️  PARTIAL: Achieved {:.2}x speedup (target: 3-5x)", speedup);
    } else {
        println!("\n❌ BELOW TARGET: Only {:.2}x speedup (target: 3-5x)", speedup);
    }

    println!("\nNote: Cache hit rate {:.2}% indicates {} sequential access pattern",
             stats.cache_hit_rate * 100.0,
             if stats.cache_hit_rate > 0.90 { "excellent" }
             else if stats.cache_hit_rate > 0.80 { "good" }
             else { "poor" });
}

fn median_duration(times: &[std::time::Duration]) -> std::time::Duration {
    let mut sorted = times.to_vec();
    sorted.sort();
    sorted[sorted.len() / 2]
}
