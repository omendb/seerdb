// Profiling workload for identifying micro-optimization opportunities
// Run with: samply record cargo run --release --example profile_workload

use seerdb::{DBOptions, DB};
use std::env;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workload = env::args().nth(1).unwrap_or_else(|| "all".to_string());

    match workload.as_str() {
        "write" => profile_sequential_writes()?,
        "read" => profile_random_reads()?,
        "scan" => profile_range_scans()?,
        "all" => {
            profile_sequential_writes()?;
            profile_random_reads()?;
            profile_range_scans()?;
        }
        _ => {
            eprintln!("Usage: profile_workload [write|read|scan|all]");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn profile_sequential_writes() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("=== Profiling: Sequential Writes ===");
    let temp_dir = tempfile::tempdir()?;
    let opts = DBOptions {
        data_dir: temp_dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts)?);

    // Sequential writes (triggers memtable flush + compaction)
    let num_keys = 500_000;
    for i in 0..num_keys {
        let key = format!("key_{:08}", i);
        let value = format!("value_{:08}_data_padding_for_realistic_size", i);
        db.put(key.as_bytes(), value.as_bytes())?;

        // Trigger flush periodically
        if i % 100_000 == 0 && i > 0 {
            eprintln!("Written {} keys...", i);
        }
    }

    db.flush()?;
    eprintln!("Completed {} sequential writes", num_keys);
    Ok(())
}

fn profile_random_reads() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("=== Profiling: Random Reads ===");

    // First, create dataset
    let temp_dir = tempfile::tempdir()?;
    let opts = DBOptions {
        data_dir: temp_dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts)?);

    // Write data
    let num_keys = 100_000;
    for i in 0..num_keys {
        let key = format!("key_{:08}", i);
        let value = format!("value_{:08}_data", i);
        db.put(key.as_bytes(), value.as_bytes())?;
    }
    db.flush()?;

    eprintln!("Dataset created, starting random reads...");

    // Random reads (tests SSTable lookup, bloom filters, ALEX index)
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let num_reads = 1_000_000;

    for i in 0..num_reads {
        let key_id = rng.gen_range(0..num_keys);
        let key = format!("key_{:08}", key_id);
        let _ = db.get(key.as_bytes())?;

        if i % 250_000 == 0 && i > 0 {
            eprintln!("Completed {} random reads...", i);
        }
    }

    eprintln!("Completed {} random reads", num_reads);
    Ok(())
}

fn profile_range_scans() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("=== Profiling: Range Scans ===");

    // Create graph-like dataset
    let temp_dir = tempfile::tempdir()?;
    let opts = DBOptions {
        data_dir: temp_dir.path().to_path_buf(),
        memtable_capacity: 4 * 1024 * 1024, // 4MB
        ..Default::default()
    };

    let db = Arc::new(DB::open(opts)?);

    // Create graph edges (user:edges:target format)
    let num_nodes = 1_000;
    let edges_per_node = 50;

    for src in 0..num_nodes {
        for dst in 0..edges_per_node {
            let key = format!("user:{:06}:edges:{:06}", src, dst);
            let value = format!("edge_data_{}", dst);
            db.put(key.as_bytes(), value.as_bytes())?;
        }
    }
    db.flush()?;

    eprintln!("Graph dataset created, starting range scans...");

    // Range scans (prefix scans for each node's edges)
    let num_scans = 10_000;
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for i in 0..num_scans {
        let src = rng.gen_range(0..num_nodes);
        let prefix = format!("user:{:06}:edges:", src);

        let mut _count = 0;
        for result in db.prefix(prefix.as_bytes())? {
            let _ = result?;
            _count += 1;
        }

        if i % 2_500 == 0 && i > 0 {
            eprintln!("Completed {} range scans...", i);
        }
    }

    eprintln!("Completed {} range scans", num_scans);
    Ok(())
}
