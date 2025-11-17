# Soak Testing Guide - Phase 5.1

**Purpose**: Validate long-term stability, memory stability, and performance consistency before production deployment.

**Location**: `tests/soak_test.rs`

**Status**: Tests created, ready to run

---

## Available Tests

### 1. 24-Hour Continuous Operation Test

**Test**: `test_24hour_soak`

**Duration**: 24 hours

**Purpose**: Validate database stability under continuous mixed read/write workload

**Monitors**:
- Memory usage (should remain stable, <3x initial after warmup)
- Disk usage
- Throughput (ops/sec)
- Latency (avg read/write latency in microseconds)

**Workload**:
- 70% reads, 30% writes (typical production ratio)
- 1KB values
- ~1000 ops/sec target
- Background compaction enabled
- Value log (vLog) enabled for values >512 bytes

**Expected Result**:
- No crashes
- No memory leaks (memory stays <3x initial)
- Stable performance over 24 hours
- Reports every 5 minutes

**Run**:
```bash
cargo test --release --test soak_test test_24hour_soak -- --ignored --nocapture
```

**Recommended Environment**: macOS or Linux with ≥8GB RAM

---

### 2. 100GB+ Large Dataset Test

**Test**: `test_large_dataset_100gb`

**Duration**: Several hours (depends on disk speed)

**Purpose**: Validate database can handle large-scale data (100GB+)

**Monitors**:
- Memory usage during write phase (should stay bounded)
- Disk usage (should reach ~100GB)
- Write throughput
- Read validation (100k random reads)

**Workload**:
- Write 100 million 1KB values (~100GB)
- 64MB memtable for faster writes
- Background compaction enabled
- Value log (vLog) enabled for values >512 bytes

**Expected Result**:
- Successfully write 100GB of data
- Memory stays <4x initial during writes
- All random reads succeed after writes
- No memory leak at end

**Run**:
```bash
cargo test --release --test soak_test test_large_dataset_100gb -- --ignored --nocapture
```

**Recommended Environment**: Fedora machine (i9-13900KF, 32GB RAM, fast SSD)

---

## Running Tests

### Quick Validation (Not Full Soak)

For quick validation that tests compile and run:

```bash
# Compile tests (verify no errors)
cargo test --test soak_test --no-run

# List available tests
cargo test --test soak_test -- --ignored --list
```

### Full Soak Tests

#### 24-Hour Test (Recommended First)

```bash
# On macOS or Linux
cargo test --release --test soak_test test_24hour_soak -- --ignored --nocapture

# Expected output:
# === 24-HOUR SOAK TEST ===
# Duration: 24 hours
# Report interval: 300s
# Value size: 1024 bytes
#
# Initial memory: X MB
#
# --- Report at 0h 5m ---
#   Total operations: ~300000
#   Throughput: ~1000 ops/sec
#   Memory usage: X MB
#   ...
```

Monitor the output. The test will report every 5 minutes with:
- Total operations completed
- Current throughput
- Memory usage (should be stable)
- Disk usage
- Average latencies

**Failure Modes**:
- Memory leak: Memory grows >3x initial after 1 hour → FAIL
- Crash: Process dies → FAIL
- Performance degradation: Throughput drops significantly → Investigate

#### 100GB Test (Run on Fedora)

```bash
# SSH to fedora machine
ssh nick@fedora

# Navigate to seerdb
cd ~/seerdb

# Pull latest code
git pull

# Run test
cargo test --release --test soak_test test_large_dataset_100gb -- --ignored --nocapture

# Expected output:
# === 100GB+ DATASET TEST ===
# Target: Write and read 100GB+ of data
# This will take several hours...
#
# Writing 104857600 keys (100 GB)...
#   Progress: 1.0% (1048576 keys, 1 GB disk, 50 MB memory)
#   Progress: 2.0% (2097152 keys, 2 GB disk, 50 MB memory)
#   ...
```

**Estimated Time**: 2-4 hours depending on disk speed

**Failure Modes**:
- Memory grows unbounded during writes → FAIL
- Disk fills up → Ensure >120GB free space
- Write fails → Investigate error
- Read validation fails → Data corruption → CRITICAL FAIL

---

## Success Criteria

### 24-Hour Test

✅ **PASS** if:
- Completes 24 hours without crash
- Memory stays <3x initial after 1 hour warmup
- Throughput stays >500 ops/sec throughout
- No error messages in output

❌ **FAIL** if:
- Process crashes
- Memory grows unbounded (>3x initial)
- Throughput degrades >50%
- Any panics or errors

### 100GB Test

✅ **PASS** if:
- Successfully writes ~100GB of data
- Memory stays <4x initial during writes
- All 100k random reads succeed
- Final memory <3x initial

❌ **FAIL** if:
- Cannot write 100GB (crashes or errors)
- Memory leak during writes (>4x growth)
- Read validation fails (data corruption)
- Final memory >3x initial

---

## What These Tests Validate

### 24-Hour Soak

**Validates**:
- No memory leaks over long periods
- Background compaction works correctly
- No performance degradation
- Stable under continuous load
- No resource exhaustion (file descriptors, etc.)

**Does NOT validate**:
- Large dataset behavior (use 100GB test)
- Real workload patterns (use Phase 5.2 production integration)

### 100GB Test

**Validates**:
- LSM compaction works at scale
- Multiple LSM levels populated correctly
- Memory stays bounded with large datasets
- No corruption with large data volumes
- Read performance at scale

**Does NOT validate**:
- Long-term stability (use 24h test)
- Real workload patterns (use Phase 5.2 production integration)

---

## Known Limitations

### Platform Support

- **Memory tracking**:
  - Linux: Uses `/proc/self/status` (accurate)
  - macOS: Uses `ps -o rss` (accurate)
  - Other platforms: Returns 0 (no monitoring)

- **Recommendation**: Run on Linux (Fedora) or macOS

### Test Duration

- 24-hour test: Cannot be shortened (that's the point)
- 100GB test: Can reduce `TARGET_SIZE_GB` for faster runs (not recommended)

### Resource Requirements

- **24-hour test**:
  - RAM: ≥8GB recommended
  - Disk: ≥10GB free (data accumulates slowly)
  - Time: 24 hours uninterrupted

- **100GB test**:
  - RAM: ≥8GB minimum, 32GB recommended
  - Disk: ≥120GB free (100GB data + overhead)
  - Time: 2-4 hours

---

## Next Steps After Soak Testing

Once both tests pass:

1. **Document results** in `ai/SOAK_RESULTS.md`:
   - Memory usage patterns
   - Throughput over time
   - Any issues encountered
   - Performance baselines

2. **Move to Phase 5.2** (Real Workload Validation):
   - Integrate with target vector database application
   - Test actual production workload
   - Dual-write validation vs RocksDB

3. **Move to Phase 5.3** (Production Readiness):
   - Security review
   - Operational documentation
   - Migration tools

---

## Troubleshooting

### Test Fails with Memory Leak

**Symptom**: Memory grows >3x or >4x threshold

**Diagnosis**:
1. Check if memory growth is bounded (plateaus) or unbounded (keeps growing)
2. If plateaus: May need to adjust threshold
3. If unbounded: Real memory leak

**Action**:
- If real leak: Investigate with leak detection tests (`tests/leak_detection_tests.rs`)
- Fix leak before proceeding to production

### Test Crashes

**Symptom**: Process dies before completion

**Diagnosis**:
1. Check error message
2. Check system logs
3. Check available resources (disk space, memory, file descriptors)

**Action**:
- Fix crash cause
- Re-run test

### Performance Degrades

**Symptom**: Throughput drops significantly during test

**Diagnosis**:
1. Check compaction lag (are SSTables accumulating?)
2. Check memory usage (is system swapping?)
3. Check disk I/O (is disk saturated?)

**Action**:
- If compaction lag: Investigate compaction performance
- If swapping: Reduce memtable size or increase RAM
- If disk saturated: Use faster disk or reduce write rate

---

## References

- Test file: `tests/soak_test.rs`
- Leak detection tests: `tests/leak_detection_tests.rs`
- Stress tests: `tests/stress_test.rs`
- Phase 5 plan: `ai/PLAN.md` (Phase 5.1)
