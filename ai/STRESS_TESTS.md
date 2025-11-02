# Stress Test Strategy

**Last Updated**: November 1, 2025
**Status**: Design phase
**Priority**: HIGH-3 (production blocker)

---

## Objective

Verify seerdb behavior under production-like load conditions:
- High throughput (millions of operations)
- Concurrent access (multi-threaded)
- Resource stability (no leaks)
- Performance predictability (latency distribution)

---

## Test Suite Design

### Test Categories

**1. Sequential Write Stress**
- 10M sequential writes (key_000000001 → key_010000000)
- Measure: throughput (ops/sec), memory usage, disk usage
- Verify: all writes succeed, no crashes, predictable performance

**2. Random Write Stress**
- 10M random writes (uniform key distribution)
- Measure: throughput, write amplification, compaction frequency
- Verify: LSM tree balance, no hotspots, stable performance

**3. Read-Heavy Stress**
- 10M reads (Zipfian distribution - realistic workload)
- Measure: p50/p99/p999 latency, cache hit rate
- Verify: bloom filter effectiveness, consistent latency

**4. Mixed Workload Stress**
- 70% reads, 20% writes, 10% deletes
- 10M total operations
- Measure: overall throughput, operation latency by type
- Verify: no operation starvation, fair scheduling

**5. Concurrent Access Stress**
- 8 threads, each doing 1M operations
- Mix of reads/writes/deletes per thread
- Measure: contention, lock wait times, throughput scaling
- Verify: thread safety, no deadlocks, linear scaling

**6. Long-Running Stability**
- 24-hour continuous operation
- Constant write rate (10k ops/sec)
- Measure: memory growth, fd count, CPU usage over time
- Verify: no memory leaks, no fd leaks, stable performance

**7. Large Dataset Stress**
- 100M entries (100GB+ database)
- Measure: compaction overhead, read latency at scale
- Verify: LSM tree depth, space amplification

---

## Implementation Plan

### Phase 1: Basic Stress Tests (Days 1-2)
**Target**: Verify core functionality under load

Tests to implement:
1. `test_stress_sequential_writes` - 1M writes (scaled down for CI)
2. `test_stress_random_writes` - 1M writes
3. `test_stress_concurrent_access` - 4 threads x 250k ops

**Metrics**:
- Total time (calculate throughput)
- Memory usage (process RSS)
- Final database size

**Success Criteria**:
- All operations complete without errors
- Throughput > 100k ops/sec
- Memory growth < 2x initial size

### Phase 2: Performance Metrics (Days 3-4)
**Target**: Measure latency distribution

Enhancements:
1. Add latency tracking (histogram)
2. Calculate p50/p99/p999 percentiles
3. Export metrics to CSV/JSON

**Metrics**:
- Per-operation latency (µs)
- Throughput over time (ops/sec windowed)
- Resource usage snapshots

**Success Criteria**:
- p50 < 50µs for reads (cached)
- p99 < 500µs for reads
- p99 < 5ms for writes

### Phase 3: Resource Leak Detection (Day 5)
**Target**: Verify no memory/fd leaks

Enhancements:
1. Track memory usage every 100k ops
2. Track open file descriptor count
3. Verify cleanup on drop

**Metrics**:
- Memory RSS over time
- Virtual memory size
- Open fd count
- Thread count

**Success Criteria**:
- Memory growth linear with data size (no leak)
- FD count stable (< 100 open files)
- All resources cleaned up on DB drop

### Phase 4: Full-Scale Tests (Days 6-7)
**Target**: Run 10M+ operation tests

Tests (NOT for CI - manual runs):
1. `bench_stress_10m_sequential` - 10M sequential writes
2. `bench_stress_10m_random` - 10M random writes
3. `bench_stress_concurrent_8x1m` - 8 threads x 1M ops
4. `bench_stress_24h_stability` - 24-hour run

**Environment**: Run on dedicated test machine
**Metrics**: Full performance profile + resource tracking

---

## Test Infrastructure

### Metrics Collection

```rust
struct StressMetrics {
    total_ops: u64,
    start_time: Instant,
    latencies: Vec<Duration>,
    memory_samples: Vec<usize>,
    fd_samples: Vec<usize>,
}

impl StressMetrics {
    fn record_op(&mut self, latency: Duration) { ... }
    fn sample_resources(&mut self) { ... }
    fn report(&self) -> StressReport { ... }
}
```

### Resource Monitoring

```rust
fn get_memory_usage() -> usize {
    // Read /proc/self/status on Linux
    // Use sysinfo crate for cross-platform
}

fn get_fd_count() -> usize {
    // Count files in /proc/self/fd
    // Use std::fs::read_dir
}
```

### Latency Percentiles

```rust
fn calculate_percentiles(latencies: &[Duration]) -> (Duration, Duration, Duration) {
    let mut sorted = latencies.to_vec();
    sorted.sort();

    let p50 = sorted[sorted.len() * 50 / 100];
    let p99 = sorted[sorted.len() * 99 / 100];
    let p999 = sorted[sorted.len() * 999 / 1000];

    (p50, p99, p999)
}
```

---

## Test Scenarios

### Scenario 1: Sequential Write Storm
**Simulates**: Bulk data import, time-series ingestion

```rust
#[test]
#[ignore] // Too slow for CI
fn test_stress_sequential_10m() {
    let db = DB::open(options, path).unwrap();

    for i in 0..10_000_000 {
        let key = format!("key_{:010}", i);
        let value = format!("value_{:010}", i);
        db.put(key.as_bytes(), value.as_bytes()).unwrap();

        if i % 100_000 == 0 {
            println!("Progress: {}/10M", i);
        }
    }
}
```

### Scenario 2: Random Write Chaos
**Simulates**: Cache churn, high write amplification

```rust
#[test]
#[ignore]
fn test_stress_random_10m() {
    let db = DB::open(options, path).unwrap();
    let mut rng = thread_rng();

    for _ in 0..10_000_000 {
        let key_num: u64 = rng.gen_range(0..1_000_000_000);
        let key = format!("key_{:010}", key_num);
        let value = vec![0u8; 128]; // 128-byte values
        db.put(key.as_bytes(), &value).unwrap();
    }
}
```

### Scenario 3: Concurrent Mayhem
**Simulates**: Multi-user production workload

```rust
#[test]
fn test_stress_concurrent_8_threads() {
    let db = Arc::new(DB::open(options, path).unwrap());
    let mut handles = vec![];

    for thread_id in 0..8 {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            let mut rng = thread_rng();
            for i in 0..1_000_000 {
                let op: u8 = rng.gen_range(0..10);

                if op < 7 { // 70% reads
                    let key_num: u64 = rng.gen_range(0..10_000_000);
                    let key = format!("key_{:010}", key_num);
                    let _ = db_clone.get(key.as_bytes());
                } else if op < 9 { // 20% writes
                    let key = format!("t{}_key_{:07}", thread_id, i);
                    let value = vec![0u8; 128];
                    db_clone.put(key.as_bytes(), &value).unwrap();
                } else { // 10% deletes
                    let key_num: u64 = rng.gen_range(0..10_000_000);
                    let key = format!("key_{:010}", key_num);
                    db_clone.delete(key.as_bytes()).unwrap();
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
```

### Scenario 4: Memory Leak Hunter
**Simulates**: Long-running service

```rust
#[test]
#[ignore]
fn test_stress_24h_stability() {
    let db = DB::open(options, path).unwrap();
    let start = Instant::now();
    let mut op_count = 0u64;
    let mut memory_samples = vec![];

    while start.elapsed() < Duration::from_secs(24 * 60 * 60) {
        // Write at 10k ops/sec
        let key = format!("key_{:010}", op_count);
        let value = vec![0u8; 128];
        db.put(key.as_bytes(), &value).unwrap();

        op_count += 1;

        // Sample memory every 1M ops
        if op_count % 1_000_000 == 0 {
            let memory = get_memory_usage();
            memory_samples.push(memory);
            println!("Ops: {}M, Memory: {} MB", op_count / 1_000_000, memory / 1024 / 1024);
        }

        // Rate limit to 10k ops/sec
        thread::sleep(Duration::from_micros(100));
    }

    // Verify memory didn't grow > 2x
    let max_memory = memory_samples.iter().max().unwrap();
    let min_memory = memory_samples.iter().min().unwrap();
    assert!(max_memory < min_memory * 2, "Memory leak detected!");
}
```

---

## Success Criteria

### Phase 1 Complete (Days 1-2)
- ✅ 3 stress tests implemented
- ✅ All tests pass (1M ops each)
- ✅ Throughput > 100k ops/sec
- ✅ No crashes or panics

### Phase 2 Complete (Days 3-4)
- ✅ Latency tracking implemented
- ✅ p50/p99/p999 calculated
- ✅ Metrics exported to file
- ✅ Performance meets targets

### Phase 3 Complete (Day 5)
- ✅ Resource monitoring implemented
- ✅ No memory leaks detected
- ✅ No fd leaks detected
- ✅ Cleanup verified

### Phase 4 Complete (Days 6-7)
- ✅ 10M operation tests run successfully
- ✅ 24-hour stability test passes
- ✅ Full performance profile generated
- ✅ Documentation updated

---

## Dependencies

**Crates needed**:
- `sysinfo` - Cross-platform resource monitoring
- `rand` - Random number generation for stress tests
- `hdrhistogram` - Accurate latency percentiles

**Add to Cargo.toml**:
```toml
[dev-dependencies]
sysinfo = "0.29"
hdrhistogram = "7.5"
rand = "0.8" # Already have this
```

---

## File Structure

```
tests/
├── stress_test.rs           # Main stress test suite
└── helpers/
    ├── metrics.rs           # Metrics collection
    └── monitoring.rs        # Resource monitoring

benches/
├── stress_10m_sequential.rs
├── stress_10m_random.rs
├── stress_concurrent.rs
└── stress_24h_stability.rs
```

---

## Notes

**CI Considerations**:
- Mark 10M+ tests with `#[ignore]`
- CI runs scaled-down versions (100k ops)
- Full stress tests run manually or in nightly CI

**Machine Requirements**:
- 16GB+ RAM for 10M tests
- 200GB+ disk for 100M tests
- Multi-core CPU for concurrent tests

**Risk Mitigation**:
- Tests use temp directories (auto-cleanup)
- Timeout guards (5min for 1M tests, 1h for 10M tests)
- Resource monitoring prevents runaway tests

---

*Last Updated: November 1, 2025*
*Priority: HIGH-3 (production blocker)*
*Timeline: 1 week (7 days)*
