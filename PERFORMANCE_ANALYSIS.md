# Performance Analysis & Optimization Strategy

**Created**: November 7, 2025
**Goal**: Identify biggest performance wins and optimal optimization strategy

---

## Benchmark Results Summary (Nov 7, 2025)

### Current Performance vs Competitors

| Workload | seerdb | RocksDB | fjall | sled | seerdb/RocksDB | seerdb/fjall |
|----------|--------|---------|-------|------|----------------|--------------|
| **Writes** | 218K | 356K | **423K** | 71K | **0.61x** 🔴 | **0.52x** 🔴 |
| **Reads** | 872K | 1,070K | 695K | **2,187K** | **0.81x** ⚠️ | **1.25x** ✅ |
| **Mixed** | 311K | 407K | **566K** | 94K | **0.76x** ⚠️ | **0.55x** 🔴 |
| **Scans** | 17,087 | **21,175** | 11,360 | **31,820** | **0.81x** ⚠️ | **1.50x** ✅ |

**Key Findings**:
- ✅ **Reads**: Beating fjall by 25%, competitive with RocksDB
- ✅ **Scans**: Beating fjall by 50%, competitive with RocksDB (after Phase 7 optimization)
- 🔴 **Writes**: **BIGGEST PROBLEM** - 48% slower than fjall, 39% slower than RocksDB
- 🔴 **Mixed**: 45% slower than fjall, 24% slower than RocksDB

---

## Critical Insight: I/O is NOT Our Bottleneck!

### What Other Engines Use

#### fjall (Best Rust LSM - 423K writes/sec)
**I/O Strategy**: `std::fs` (synchronous, blocking I/O)
- Source: https://github.com/fjall-rs/lsm-tree
- No async I/O, no io_uring, no Tokio
- **Yet 1.94x faster than us on writes!**

#### RocksDB (Industry Standard - 356K writes/sec)
**I/O Strategy**: Mostly synchronous with optional async
- Default: `pwrite()` syscalls (blocking)
- Optional: io_uring backend (Linux only, recent)
- Most deployments use sync I/O

#### sled (Rust B-tree - 71K writes/sec)
**I/O Strategy**: `std::fs` (synchronous)
- Lock-free B-tree design
- Slower writes than LSM trees

### The Key Question

**If fjall beats us by 2x using ONLY std::fs (sync I/O), why would async I/O help us?**

**Answer**: It wouldn't! Our bottleneck is NOT I/O - it's something else.

---

## Where Are the Biggest Wins?

### Priority 1: Fix Write Performance (48% slower than fjall!)

**Gap Analysis**:
- fjall: 423K writes/sec with `std::fs`
- seerdb: 218K writes/sec with `std::fs`
- **Gap**: 205K ops/sec (94% slower!)

**This means our problem is NOT I/O**. Possible bottlenecks:

1. **WAL Implementation**
   - Known: WAL is 48.5% of write time
   - Hypothesis: Inefficient batching, too many syscalls
   - fjall likely batches more aggressively

2. **Memtable Operations**
   - Both use crossbeam-skiplist
   - Hypothesis: We may have more lock contention
   - Or inefficient key/value handling

3. **Allocations**
   - Hypothesis: Too many BytesMut allocations
   - Or cloning keys/values unnecessarily

4. **Serialization Overhead**
   - WAL record format may be inefficient
   - Extra encoding/decoding steps

**Action**: Profile write path to identify ACTUAL bottleneck

### Priority 2: Improve Mixed Workload (45% slower than fjall)

**Gap**: 311K vs 566K ops/sec

Mixed workload combines reads (where we're good) and writes (where we're bad).
- Fix writes → mixed workload improves automatically
- No separate optimization needed

### Priority 3: Maintain Read/Scan Performance

**Current Status**: ✅ Good
- Reads: 1.25x faster than fjall (872K vs 695K)
- Scans: 1.50x faster than fjall (17,087 vs 11,360)

**Strategy**: Don't regress these while optimizing writes

---

## I/O Strategy Decision

### Should We Implement io_uring?

**NO - Not Our Bottleneck**

**Evidence**:
1. fjall beats us by 2x using only `std::fs`
2. RocksDB's main deployments use sync I/O
3. User is developing on macOS (io_uring is Linux-only)
4. Security concerns (CVEs)

**Conclusion**: Async I/O (Tokio or io_uring) would give <20% improvement, but we need 2x improvement

### Should We Implement Tokio Async I/O?

**MAYBE - But Not First Priority**

**Pros**:
- Cross-platform (macOS + Linux)
- Safer than io_uring
- +20-30% potential improvement

**Cons**:
- Async/await rewrite is complex
- Won't close the 2x gap with fjall
- Should fix core bottleneck first

**Decision**: **Profile first, then decide**

### Recommended I/O Strategy

**Phase 1** (Now): Keep `std::fs`, optimize what we have
- Profile and fix actual bottlenecks
- Match fjall's write performance (423K ops/sec)
- **Target**: 2x improvement without any async I/O

**Phase 2** (Later): Add Tokio async I/O as optional optimization
- After we match fjall's baseline performance
- Implement as feature flag: `--features tokio-io`
- **Target**: Additional +20-30% improvement

**Phase 3** (Much Later): Consider io_uring as Linux-specific optimization
- Only if we need absolute maximum performance on Linux
- Feature flag: `--features io_uring` (Linux only)
- **Target**: Additional +10-20% on top of Tokio

---

## Profiling Plan (Next Steps)

### 1. Profile Write Path (Priority 1)

**Benchmark**: `examples/write_bench.rs`

```bash
# Install perf (macOS: Instruments, Linux: perf)
cargo install flamegraph

# Profile writes
sudo cargo flamegraph --release --example write_bench -- --count 1000000

# Expected findings:
# - WAL write time (current: 48.5%)
# - Memtable insert time (current: ~30%?)
# - Lock contention (unknown)
# - Allocations (unknown)
```

**Questions to Answer**:
1. Is WAL really 48.5% of time? (validate assumption)
2. What's the memtable insert overhead?
3. Are we lock contending?
4. Where are we allocating?
5. What does fjall do differently?

### 2. Compare with fjall Source Code

**Key Files to Study**:
- `lsm-tree/src/wal/` - WAL implementation
- `lsm-tree/src/memtable/` - Memtable operations
- `lsm-tree/src/batch.rs` - Write batching

**Look For**:
- Batch size and flush strategy
- Allocation patterns
- Syscall frequency
- Lock usage

### 3. Profile Memtable Operations

**Benchmark**: Isolated memtable insert/get test

```bash
# Create microbenchmark
cargo bench --bench memtable_bench
```

**Questions to Answer**:
1. Skiplist insert overhead
2. Lock contention on concurrent inserts
3. Key/value clone overhead
4. Comparison with fjall's memtable

---

## Optimization Roadmap (Revised)

### Phase 8A: Profile and Understand (2-3 days)

**Tasks**:
1. ✅ Install profiling tools (flamegraph, cargo-instruments on macOS)
2. Profile write path end-to-end
3. Profile memtable operations in isolation
4. Study fjall source code (WAL, memtable, batching)
5. Document findings

**Deliverable**: `PROFILING_RESULTS.md` with actual bottlenecks

### Phase 8B: Fix Core Bottlenecks (1-2 weeks)

**Based on profiling results, likely optimizations**:

1. **WAL Batching Optimization** (High Confidence)
   - Increase batch size (8MB → 16MB or 32MB)
   - Reduce flush interval (100ms → 50ms or adaptive)
   - Batch multiple operations into single syscall
   - **Expected**: +20-40% write throughput

2. **Memtable Optimization** (Medium Confidence)
   - Reduce lock contention (if profiling shows it)
   - Pre-allocate capacity
   - Optimize key/value handling
   - **Expected**: +10-20% write throughput

3. **Allocation Reduction** (Medium Confidence)
   - Pool BytesMut for WAL records
   - Reduce unnecessary clones
   - Reuse buffers
   - **Expected**: +5-15% write throughput

4. **Record Encoding** (Low Confidence)
   - Optimize serialization format
   - Reduce encoding overhead
   - **Expected**: +5-10% write throughput

**Target**: 400K+ write ops/sec (1.85x improvement, matching RocksDB)

### Phase 8C: Match fjall (If needed)

If Phase 8B doesn't close the gap:
- Deep dive into fjall's exact implementation
- Diff our WAL vs fjall's WAL line-by-line
- Replicate their exact strategy

**Target**: 423K+ write ops/sec (matching fjall)

### Phase 9: Async I/O (Optional)

**Only after** we match fjall's baseline performance:
- Implement Tokio async I/O as feature flag
- Benchmark improvement
- Document trade-offs

**Expected**: +20-30% additional improvement → 550K+ ops/sec

### Phase 10: io_uring (Optional, Linux only)

**Only if** we need absolute maximum performance:
- Implement io_uring as Linux-specific feature
- Extensive testing for CVE mitigations
- Benchmark vs Tokio

**Expected**: +10-20% additional improvement → 650K+ ops/sec

---

## Success Metrics

### Phase 8A Success (Profiling)
- ✅ Flamegraph generated for write path
- ✅ Top 3 bottlenecks identified with percentages
- ✅ fjall implementation studied and documented
- ✅ Optimization plan prioritized by expected impact

### Phase 8B Success (Core Optimizations)
- ✅ Writes: 400K+ ops/sec (1.12x RocksDB, 0.95x fjall)
- ✅ Mixed: 450K+ ops/sec (1.11x RocksDB, 0.80x fjall)
- ✅ Reads: Maintain 870K+ ops/sec (no regression)
- ✅ Scans: Maintain 17K+ scans/sec (no regression)

### Phase 8C Success (Match fjall)
- ✅ Writes: 423K+ ops/sec (1.19x RocksDB, 1.0x fjall)
- ✅ Mixed: 500K+ ops/sec (1.23x RocksDB, 0.88x fjall)

---

## Conclusion: Where to Focus

### Answer to "Where can we get the biggest wins?"

**1. Write Path Optimization: +94% potential improvement**
- Current: 218K ops/sec
- Target: 423K ops/sec (fjall's performance)
- **Biggest ROI**: Profile and fix WAL/memtable bottlenecks

**2. I/O Strategy: +20-30% potential (AFTER fixing writes)**
- Tokio async I/O (cross-platform, safe)
- NOT our main bottleneck right now

**3. io_uring: +10-20% potential (AFTER Tokio)**
- Linux-only, security concerns
- Lowest priority

### Recommended Action Plan

**This Week**:
1. ✅ Profile write path (flamegraph + Instruments)
2. ✅ Identify actual bottlenecks (validate 48.5% WAL assumption)
3. ✅ Study fjall source code (learn their secrets)
4. Implement top 3 optimizations from profiling

**Next Week**:
- Continue optimizing based on profiling results
- Benchmark after each change
- Target: 400K+ ops/sec writes

**After Matching fjall**:
- Then consider Tokio async I/O
- Then consider io_uring (if needed)

---

**Key Insight**: fjall proves we can get 2x faster writes WITHOUT any async I/O. Let's find out how they do it!

---

**Updated**: November 7, 2025
**Status**: Ready to profile and optimize
**Priority**: 🔴 HIGHEST - Fix writes first, I/O optimizations later
