# Benchmarks - seerdb vs RocksDB/sled/fjall

**Last Updated**: November 5, 2025
**Status**: 🚨 CRITICAL PERFORMANCE ISSUES FOUND

---

## Baseline Benchmark Results (Nov 5, 2025)

**Configuration**:
- Operations: 100,000
- Value size: 1024 bytes (1KB)
- Memtable: 64MB
- Sync: Disabled (fast mode)
- seerdb config: vLog enabled (4KB threshold), background compaction

### Results Summary

| Engine | Sequential Writes | Random Reads | Mixed 50/50 | Range Scans |
|--------|------------------|--------------|-------------|-------------|
| **RocksDB** | 370,620 ops/sec | 1,037,751 ops/sec | 392,330 ops/sec | 20,016 scans/sec |
| **fjall** | 450,436 ops/sec | 735,247 ops/sec | 581,475 ops/sec | 11,721 scans/sec |
| **sled** | 70,020 ops/sec | 3,441,215 ops/sec | 87,403 ops/sec | 50,052 scans/sec |
| **seerdb** | 250,007 ops/sec | 2,800 ops/sec | 3,661 ops/sec | 18 scans/sec |

### seerdb vs RocksDB Comparison

| Workload | RocksDB | seerdb | Performance Ratio |
|----------|---------|--------|-------------------|
| **Sequential Writes** | 370,620 ops/sec (2.70 µs) | 250,007 ops/sec (4.00 µs) | **0.67x (33% slower)** |
| **Random Reads** | 1,037,751 ops/sec (0.96 µs) | 2,800 ops/sec (357.16 µs) | **0.0027x (370x SLOWER)** ❌ |
| **Mixed 50/50** | 392,330 ops/sec (2.55 µs) | 3,661 ops/sec (273.17 µs) | **0.0093x (107x SLOWER)** ❌ |
| **Range Scans** | 20,016 scans/sec (0.05 ms) | 18 scans/sec (54.66 ms) | **0.0009x (1112x SLOWER)** ❌ |

---

## Critical Findings

### 🚨 CLAIM VALIDATION: FAILED

**Original Claims**:
- ❌ "10x better write amplification" - NOT VALIDATED (not measured)
- ❌ "5x faster queries" - **FAILED: 370x SLOWER reads**

**Reality Check**:
- Writes: 0.67x (slightly slower, acceptable)
- **Reads: 0.0027x (370x SLOWER)** ← CRITICAL REGRESSION
- **Mixed: 0.0093x (107x SLOWER)** ← CRITICAL REGRESSION
- **Scans: 0.0009x (1112x SLOWER)** ← CRITICAL REGRESSION

### Read Path Performance Breakdown

**Random Reads** (per operation):
- RocksDB: **0.96 µs/op**
- seerdb: **357.16 µs/op**
- **Overhead: 356.2 µs** (372x slower)

**Range Scans** (per scan of 100 keys):
- RocksDB: **0.05 ms/scan**
- seerdb: **54.66 ms/scan**
- **Overhead: 54.61 ms** (1093x slower)

---

## Root Cause Analysis (Hypothesis)

### Likely Culprits (in order of probability)

**1. SSTable Lookup Algorithm** (Most Likely)
- Hypothesis: Checking all SSTables instead of stopping at first match
- Expected: Bloom filter → early exit on negative
- Actual: Possibly iterating all 7 levels for every miss
- Impact: O(n) instead of O(1) for negative lookups

**2. Bloom Filter Not Working**
- Hypothesis: Bloom filters not being checked or always returning true positive
- Expected: 99% of negative lookups filtered at bloom check
- Actual: Possibly checking every SSTable on disk
- Impact: Extra disk I/O for every lookup

**3. ALEX Index Overhead**
- Hypothesis: ALEX learned index adding latency instead of reducing it
- Expected: 1.04-1.42x speedup (from isolated benchmarks)
- Actual: Possibly 100x+ overhead in integrated system
- Impact: Every SSTable lookup pays ALEX cost

**4. vLog Indirection**
- Hypothesis: All reads going through vLog even for small values
- Expected: Only values >4KB use vLog
- Actual: Possibly all values being redirected
- Impact: 2x I/O for every read (SSTable → vLog)

**5. Inefficient Merge Logic**
- Hypothesis: Merging memtable + all levels inefficiently
- Expected: Efficient merge iterator
- Actual: Possibly O(n²) merge algorithm
- Impact: Degrades with dataset size

---

## Diagnostic Plan

### Phase 1: Isolate the Problem (IMMEDIATE)

**Test 1: Disable All SOTA Features**
```bash
# Test with minimal LSM (no vLog, no ALEX, no learned bloom)
# Expected: Should match RocksDB baseline
```

**Test 2: Enable Features One by One**
```bash
# Isolate which feature causes regression:
# 1. Baseline (minimal LSM)
# 2. + Bloom filters
# 3. + ALEX
# 4. + vLog
# 5. + Learned bloom
# 6. + Dostoevsky
```

**Test 3: Profile Read Path**
```bash
# Use flamegraph/perf to find where 357µs is spent
cargo flamegraph --example baseline_benchmark
```

### Phase 2: Fix Critical Path (URGENT)

**If SSTable Lookup:**
- Fix: Add early exit on first match
- Fix: Ensure bloom filter check happens before disk read
- Validate: Reads should be 1-10µs, not 357µs

**If Bloom Filter:**
- Fix: Verify bloom filter construction
- Fix: Check if bloom.contains() is actually called
- Validate: 99% negative lookup filtering

**If ALEX:**
- Fix: Benchmark ALEX in context
- Fix: Consider disabling if overhead > benefit
- Validate: ALEX should speed up, not slow down

**If vLog:**
- Fix: Check value_size threshold logic
- Fix: Ensure small values don't use vLog
- Validate: Only >4KB values redirected

### Phase 3: Measure Write Amplification

**After reads are fixed:**
- Instrument bytes written to disk
- Compare with/without vLog
- Validate "10x better write amp" claim

---

## Comparison to Other Engines

### sled (B-tree, not LSM)

**Strengths**:
- **Fastest reads**: 3.4M ops/sec (3.3x faster than RocksDB)
- **Fastest scans**: 50K scans/sec (2.5x faster than RocksDB)

**Weaknesses**:
- Slow writes: 70K ops/sec (5.3x slower than RocksDB)
- Slow mixed: 87K ops/sec (4.5x slower than RocksDB)

**Architecture**: B-tree favors reads over writes (opposite of LSM)

### fjall (Modern Rust LSM)

**Strengths**:
- **Fastest writes**: 450K ops/sec (1.2x faster than RocksDB)
- **Good mixed**: 581K ops/sec (1.5x faster than RocksDB)
- Competitive reads: 735K ops/sec (0.7x vs RocksDB)

**Weaknesses**:
- Slower scans: 11K scans/sec (0.6x vs RocksDB)

**Architecture**: Modern LSM, well-optimized, good baseline

### RocksDB (Industry Standard)

**Profile**:
- Balanced: Good at everything, excellent at nothing
- Write: 370K ops/sec
- Read: 1M ops/sec
- Mixed: 392K ops/sec
- Scans: 20K scans/sec

**Status**: **Best overall performance** - this is why it's the standard

---

## Benchmark Environment

**Hardware**:
- CPU: M3 Max (Apple Silicon)
- RAM: 128GB
- Storage: SSD
- OS: macOS

**Software**:
- Rust: Nightly (portable_simd)
- Optimization: --release (opt-level 3, LTO)

**Dataset**:
- Keys: Sequential (key_00000000 to key_00099999)
- Values: Random 1KB blobs
- Total: ~100MB

---

## Action Items

**CRITICAL** (Before any other work):
1. Profile read path to find 357µs bottleneck
2. Fix critical regression (target: <10µs reads)
3. Re-run benchmark to validate fix
4. Only then: measure write amplification

**BLOCKED** (Until reads are fixed):
- Write amplification measurement
- SOTA feature validation
- omen integration
- Production deployment

---

## Historical Context

**Development Timeline**:
- Phase 1-5: Core engine implemented (123 tests passing)
- SOTA features: vLog, ALEX, Dostoevsky, learned bloom, std::simd
- Nov 5: First end-to-end benchmark vs RocksDB
- Result: **Critical performance regression discovered**

**Lesson Learned**:
- Isolated benchmarks (ALEX: 1.4x faster, vLog: 10x write amp) are misleading
- **End-to-end integration can introduce massive regressions**
- Need to benchmark the **entire system**, not just components

**Next Phase**:
- Fix reads (target: match or beat RocksDB)
- Then validate write amp claims
- Then consider omen integration

---

**Status**: 🚨 NOT READY FOR PRODUCTION - Critical read performance regression
