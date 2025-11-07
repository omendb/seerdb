# Research-Driven Implementation Roadmap

**Last Updated**: November 6, 2025
**Based on**: Competitive analysis + SOTA research (2024-2025)

---

## Strategic Context

**Current State**: seerdb is functional with unique learned components
**Gap Analysis**: 95% slower range scans, missing workload-aware tuning
**Opportunity**: First Rust LSM with learned indexes + adaptive optimization

---

## Phase 7: Range Scan Optimization (CRITICAL - Week 11)

**Goal**: Close 20x performance gap with RocksDB (currently 0.05x → target 0.9x+)

### Tasks

1. **SSTable Range Filtering** (2-3 days)
   - Add min_key/max_key metadata to SSTable
   - Check overlap before creating range iterators
   - Expected: 10-15x improvement (based on RocksDB analysis)

2. **Lazy Iterator Optimization** (1-2 days)
   - Verify blocks load on-demand (already implemented)
   - Profile iterator creation overhead
   - Expected: Already optimized (cache hit: 60µs per scan)

3. **Benchmark Validation** (1 day)
   - Measure improvement: baseline_benchmark.rs
   - Compare vs RocksDB: Same workload
   - Target: 0.9x+ RocksDB performance

**Success Criteria**:
- ✅ Range scans: 15,000+ scans/sec (currently 870)
- ✅ Competitive with RocksDB: 0.9x-1.1x
- ✅ No regression on point queries

**Priority**: 🔴 CRITICAL - Blocking production use

---

## Phase 8: Research Validation (Week 12)

**Goal**: Validate research claims and measure learned component impact

### 1. Learned Bloom Filter Validation (2 days)

**Claim**: 90% space reduction vs traditional bloom filters

**Tasks**:
- [ ] Implement traditional bloom filter baseline
- [ ] Measure space usage: learned vs traditional
- [ ] Measure false positive rate (should be equivalent)
- [ ] Measure query time overhead

**Expected Results**:
- Space: 90% reduction (from paper)
- FP rate: Same as traditional (1%)
- Query: 10-20% slower (model inference overhead)

**Deliverable**: `ai/research/BLOOM_FILTER_RESULTS.md`

### 2. Write Amplification Benchmark (2 days)

**Claim**: 1.01x with vLog vs 4.88x traditional LSM (4.82x improvement)

**Tasks**:
- [ ] Measure seerdb write amp over 1B writes
- [ ] Compare vs fjall (traditional LSM)
- [ ] Measure disk writes vs logical writes
- [ ] Validate 4.82x improvement claim

**Expected Results**:
- seerdb: 1.01-1.05x write amp
- fjall: 4-5x write amp
- Confirmation: 4-5x better than traditional LSM

**Deliverable**: `ai/research/WRITE_AMP_VALIDATION.md`

### 3. ALEX Index Impact (1 day)

**Tasks**:
- [ ] Benchmark reads with vs without ALEX
- [ ] Measure memory overhead per SSTable
- [ ] Measure lookup time improvement

**Expected Results**:
- Read improvement: 20-40% (from SOTA paper)
- Memory overhead: 2-4KB per SSTable per 1M keys
- Lookup: 1-2 fewer disk I/O per query

**Deliverable**: `ai/research/ALEX_INDEX_RESULTS.md`

---

## Phase 9: Workload-Aware Optimization (Week 13-14)

**Goal**: Auto-tune LSM parameters based on workload (CAMAL-inspired)

**Based on**: "CAMAL: Optimizing LSM-trees via Active Learning" (Sept 2024)

### 1. Workload Detection (3 days)

**Metrics to Track**:
- Key distribution: sorted, random, skewed
- Access patterns: read-heavy, write-heavy, mixed
- Value sizes: small (<1KB), medium (1-4KB), large (>4KB)
- Temporal patterns: hot/cold data, recency

**Implementation**:
```rust
pub struct WorkloadMetrics {
    read_write_ratio: f64,        // 0.0 = write-only, 1.0 = read-only
    key_sortedness: f64,           // 0.0 = random, 1.0 = sorted
    avg_value_size: usize,
    key_distribution: Distribution, // Uniform, Zipf, Normal
    hot_key_ratio: f64,            // % of keys accessed frequently
}
```

**Collection Points**:
- Memtable flush: Key sortedness
- Read operations: Access frequency
- Write operations: Value size distribution

### 2. Adaptive Parameter Tuning (4 days)

**Parameters to Auto-tune**:

| Parameter | Write-Heavy | Read-Heavy | Large Values | Small Values |
|-----------|-------------|------------|--------------|--------------|
| Compaction | Tiered | Leveled | Tiered | Leveled |
| Bloom filter size | Small (1%) | Large (10%) | Skip | Large |
| vLog threshold | Low (1KB) | High (8KB) | Low (512B) | High (16KB) |
| Memtable size | Large (128MB) | Small (16MB) | Large | Small |

**Implementation**:
```rust
impl DB {
    fn tune_for_workload(&mut self, metrics: &WorkloadMetrics) {
        // Rule-based tuning (Phase 9.1)
        if metrics.read_write_ratio < 0.3 {
            // Write-heavy
            self.set_compaction_strategy(CompactionStrategy::Tiered);
            self.set_memtable_size(128 * 1024 * 1024);
        } else if metrics.read_write_ratio > 0.7 {
            // Read-heavy
            self.set_compaction_strategy(CompactionStrategy::Leveled);
            self.set_bloom_filter_bits_per_key(10);
        }

        // Value size tuning
        if metrics.avg_value_size > 4096 {
            self.set_vlog_threshold(512);  // Aggressive KV separation
        }
    }
}
```

### 3. Model Selection (2 days)

**Based on**: "Benchmarking Learned and LSM Indexes for Data Sortedness" (2024)

**Strategy**:
- Sorted data (>90%): Linear model (fast, simple)
- Random data (<50%): ALEX (adaptive)
- Skewed data: Piecewise linear (exploit hot keys)

**Implementation**:
```rust
fn select_learned_index(sortedness: f64, key_count: usize) -> IndexType {
    if sortedness > 0.9 {
        IndexType::Linear  // Simple, fast for sorted data
    } else if sortedness < 0.5 {
        IndexType::ALEX    // Robust for random data
    } else {
        IndexType::Spline  // Good for semi-sorted
    }
}
```

**Expected Results**:
- Write-heavy: 20-30% throughput improvement
- Read-heavy: 10-15% throughput improvement
- Mixed: 15-20% improvement

---

## Phase 10: Advanced Optimizations (Week 15-16)

### 1. Read Hotness Tracking (3 days)

**Based on**: "LSM-Tree Combined with Read Hotness and Learned Index" (Oct 2025)

**Approach**:
- Track access frequency in block cache (already have LRU)
- Optimize ALEX index for hot keys (more complex models)
- Use simple models for cold keys (save memory)

**Implementation**:
```rust
pub struct HotnessTracker {
    access_counts: HashMap<Bytes, u64>,
    hot_threshold: u64,  // e.g., 100 accesses
}

impl HotnessTracker {
    fn adjust_model_complexity(&self, key: &[u8]) -> ModelComplexity {
        let count = self.access_counts.get(key).unwrap_or(&0);
        if *count > self.hot_threshold {
            ModelComplexity::High  // Better accuracy for hot keys
        } else {
            ModelComplexity::Low   // Save memory for cold keys
        }
    }
}
```

### 2. Adaptive Readahead (2 days)

**Observation**: RocksDB prefetches blocks for sequential scans

**Implementation**:
- Detect sequential range scans (consecutive block reads)
- Prefetch next N blocks in background
- Cache prefetched blocks

**Expected**: 30-50% faster range scans (RocksDB-level optimization)

### 3. io_uring Integration (4 days)

**Based on**: Modern async I/O (Linux 5.1+)

**Benefits**:
- Zero-copy I/O (no kernel/user space copy)
- Batch multiple reads/writes in single syscall
- 50-100% faster I/O (from benchmarks)

**Use Cases**:
- SSTable reads during compaction
- WAL writes (batch multiple flushes)
- Range scan prefetch

**Implementation**: Use `tokio-uring` crate
```rust
use tokio_uring::fs::File;

async fn read_sstable_blocks(paths: Vec<PathBuf>) -> Vec<Bytes> {
    let mut futures = Vec::new();
    for path in paths {
        let file = File::open(path).await.unwrap();
        futures.push(file.read_at(buf, offset));
    }
    // All reads batched in single syscall!
    futures::future::join_all(futures).await
}
```

**Expected**: 2x faster compaction, 50% faster range scans

---

## Phase 11: Competitive Benchmarking (Week 17)

**Goal**: Comprehensive comparison vs fjall, sled, RocksDB

### Benchmark Suite

1. **YCSB Workloads**
   - Workload A: 50% reads, 50% writes
   - Workload B: 95% reads, 5% writes
   - Workload C: 100% reads
   - Workload D: 95% reads, 5% inserts (latest)
   - Workload E: 95% scans, 5% inserts

2. **Metrics**:
   - Throughput (ops/sec)
   - Latency (p50, p99, p999)
   - Write amplification
   - Space amplification
   - Memory footprint

3. **Engines**:
   - seerdb (with all optimizations)
   - fjall 2.8
   - RocksDB 9.x
   - sled (if relevant)

**Deliverable**: `ai/research/BENCHMARK_RESULTS.md`

---

## Phase 12: Research Publication (Week 18-19)

**Goal**: Publish findings on learned indexes + KV separation

### Paper Outline

**Title**: "Learned Indexes and Key-Value Separation: A Practical Evaluation in Rust"

**Abstract**:
- Combine learned indexes (ALEX, learned bloom) with WiscKey-style KV separation
- First comprehensive implementation in safe Rust
- 4.82x better write amplification than traditional LSM
- Competitive read performance (1.04x RocksDB)

**Sections**:
1. Introduction: LSM-tree limitations, recent research
2. Background: Learned indexes, WiscKey
3. Design: seerdb architecture
4. Implementation: Rust-specific optimizations
5. Evaluation: Benchmarks vs RocksDB, fjall
6. Discussion: Trade-offs, lessons learned
7. Conclusion: Future work

**Target Venues**:
- VLDB (Very Large Data Bases)
- SIGMOD (Database systems)
- FAST (File and Storage Technologies)
- arXiv (preprint for fast feedback)

**Unique Contributions**:
- First combination of learned indexes + KV separation
- Safe Rust implementation (no unsafe code)
- Workload-aware adaptive tuning
- Production-ready open source

---

## Success Metrics

### Phase 7 (Range Scans)
- ✅ 15,000+ scans/sec (17x improvement)
- ✅ 0.9x+ RocksDB performance

### Phase 8 (Validation)
- ✅ 90% bloom filter space reduction validated
- ✅ 4.82x write amp improvement confirmed
- ✅ 20-40% ALEX read improvement measured

### Phase 9 (Workload-Aware)
- ✅ 20-30% throughput on write-heavy workloads
- ✅ 10-15% throughput on read-heavy workloads
- ✅ Auto-tuning without manual configuration

### Phase 10 (Advanced)
- ✅ 30-50% range scan improvement (readahead)
- ✅ 2x faster compaction (io_uring)
- ✅ Better memory efficiency (hot/cold separation)

### Phase 11 (Benchmarks)
- ✅ Comprehensive results vs fjall, RocksDB
- ✅ Clear use case guidance (when to use seerdb)

### Phase 12 (Publication)
- ✅ Paper submitted to top venue
- ✅ arXiv preprint published
- ✅ Blog post with benchmarks

---

## Timeline Summary

| Phase | Duration | Status | Priority |
|-------|----------|--------|----------|
| Phase 7: Range Scans | 1 week | Planning | 🔴 CRITICAL |
| Phase 8: Validation | 1 week | Planning | ⚠️ HIGH |
| Phase 9: Workload-Aware | 2 weeks | Planning | ⚠️ HIGH |
| Phase 10: Advanced | 2 weeks | Planning | 🟡 MEDIUM |
| Phase 11: Benchmarks | 1 week | Planning | 🟡 MEDIUM |
| Phase 12: Publication | 2 weeks | Planning | 🟢 LOW |

**Total**: 9 weeks (Nov 6 - Jan 8, 2026)

---

## Risk Mitigation

### Risk 1: Range scan optimization insufficient
- **Mitigation**: SSTable filtering is proven (RocksDB does it)
- **Fallback**: Implement adaptive readahead if needed

### Risk 2: Workload detection overhead
- **Mitigation**: Collect metrics passively (no active probing)
- **Fallback**: Manual tuning via config API

### Risk 3: io_uring complexity
- **Mitigation**: Use battle-tested `tokio-uring` crate
- **Fallback**: Keep traditional sync I/O as option

### Risk 4: Write performance gap vs fjall
- **Mitigation**: Profile WAL and memtable (identify bottleneck)
- **Acceptance**: 20-30% slower acceptable if write amp 4x better

---

## Next Steps (Immediate)

1. ✅ Complete competitive analysis (DONE)
2. ✅ Document SOTA research (DONE)
3. 🎯 **Start Phase 7**: Implement SSTable range filtering
4. 🎯 Update ai/STATUS.md with new roadmap
5. 🎯 Update CLAUDE.md with Phase 7 focus

**Focus**: Fix range scans FIRST (blocking issue), then validation
