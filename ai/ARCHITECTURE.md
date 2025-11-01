# seerdb Architecture Design

**Date**: November 1, 2025
**Status**: Research Phase Complete → Design Phase
**Target**: 5-10x performance improvement over fjall (438k writes/sec baseline)

---

## Executive Summary

seerdb is a research-grade LSM-tree storage engine combining proven techniques (fjall/RocksDB foundation) with 2018-2024 research innovations (learned components, SIMD, workload-aware optimizations). Target: 2-4M writes/sec, 5-10M reads/sec on vector workloads.

**Key Innovations**:
1. Learned bloom filters (90% space reduction)
2. Learned indexes (1-3x faster lookups)
3. SIMD optimizations (5-10x hot path speedup)
4. Workload-aware compaction (3-5x write amp reduction)
5. WiscKey KV separation (10-100x write amp for large values)

---

## 1. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         seerdb API                          │
│  (RocksDB-compatible + seerdb-native)                       │
└───────────────────┬─────────────────────────────────────────┘
                    │
    ┌───────────────┼───────────────┐
    │               │               │
┌───▼────┐    ┌────▼─────┐   ┌────▼────┐
│  WAL   │    │ Memtable │   │  vLog   │  ← Write Path
└────────┘    └────┬─────┘   └─────────┘
                   │
              ┌────▼─────┐
              │ Immutable│
              │ Memtable │
              └────┬─────┘
                   │ (Flush)
              ┌────▼─────┐
              │ Level 0  │  ← Fresh SSTables
              │ SSTable  │
              └────┬─────┘
                   │ (Compaction)
         ┌─────────┼─────────┐
    ┌────▼───┐ ┌──▼───┐ ┌───▼────┐
    │ Level 1│ │ L2   │ │ L3-6   │  ← Compacted SSTables
    │        │ │      │ │ (Lazy  │
    │(Tiered)│ │(Tier)│ │Leveled)│
    └────────┘ └──────┘ └────────┘
         │         │         │
    ┌────▼─────────▼─────────▼──────┐
    │     Learned Bloom Filters      │  ← Point Query Filter
    │   + Learned Index (ALEX)       │  ← Fast Lookup
    └────────────────────────────────┘
                   │
              ┌────▼─────┐
              │  Block   │  ← Read Path
              │  Cache   │
              └──────────┘
```

---

## 2. Core Components

### 2.1 Write-Ahead Log (WAL)

**Purpose**: Durability - recover memtable after crash

**Design**:
```rust
pub struct WAL {
    file: File,
    offset: u64,
    sync_policy: SyncPolicy,  // SyncAll, SyncData, None
}

enum WALRecord {
    Put { key: Bytes, value: Bytes },
    Delete { key: Bytes },
    Batch { operations: Vec<WALRecord> },
}
```

**Features**:
- Append-only log (sequential writes)
- CRC32 checksum per record
- Group commit (batch multiple operations)
- Rotation: new WAL when memtable flushes

**Reference**: fjall journal, RocksDB WAL
**Complexity**: Low (1-2 days)

---

### 2.2 Memtable

**Purpose**: In-memory buffer for recent writes

**Design**:
```rust
pub struct Memtable {
    data: Arc<SkipMap<Bytes, Bytes>>,  // crossbeam_skiplist
    size: AtomicUsize,
    capacity: usize,  // Flush when size >= capacity
}
```

**Features**:
- Concurrent skiplist (lock-free reads/writes)
- Size-based flush trigger (default: 64MB)
- Snapshot support (clone Arc for point-in-time reads)
- Iterator support (range scans)

**Reference**: fjall memtable, RocksDB skiplist
**Complexity**: Low (2-3 days, use existing library)

---

### 2.3 SSTable Format

**Purpose**: On-disk sorted table with learned components

#### Block Structure
```
┌──────────────────────────────────────┐
│           SSTable File               │
├──────────────────────────────────────┤
│  Data Blocks (4KB each, compressed)  │
│  ┌────────────────────────────┐      │
│  │ [k1, v1, k2, v2, ...]      │      │
│  └────────────────────────────┘      │
├──────────────────────────────────────┤
│  Learned Bloom Filter                │
│  ┌────────────────────────────┐      │
│  │ Model (decision tree/NN)   │      │
│  │ + Backup Traditional Filter│      │
│  └────────────────────────────┘      │
├──────────────────────────────────────┤
│  Learned Index (ALEX-style)          │
│  ┌────────────────────────────┐      │
│  │ Piecewise Linear Models    │      │
│  │ + Fallback Binary Search   │      │
│  └────────────────────────────┘      │
├──────────────────────────────────────┤
│  Metadata                            │
│  - Min/max key, count, level, ...   │
└──────────────────────────────────────┘
```

#### Implementation
```rust
pub struct SSTable {
    file: File,
    index: LearnedIndex,  // ALEX-style or fallback binary search
    bloom: LearnedBloomFilter,  // or traditional
    metadata: Metadata,
    block_cache: Arc<BlockCache>,
}

pub struct LearnedIndex {
    models: Vec<LinearModel>,  // Piecewise linear approximations
    fallback: BinarySearchIndex,  // If model fails
}

pub struct LearnedBloomFilter {
    model: Option<DecisionTree>,  // Trained during compaction
    backup: BloomFilter,  // Traditional (smaller than full)
    threshold: f64,  // Confidence threshold
}
```

**Features**:
- **Learned bloom filter**: 90% space reduction (>10k keys)
- **Learned index**: ALEX-style piecewise linear models
- **Fallback support**: Binary search if learned components fail
- **Compression**: LZ4 (default), zstd (optional)
- **Block cache**: LRU (16-32 MB default)

**Reference**:
- Bloom: ai/research/PAPERS.md (Learned Bloom Filters)
- Index: ALEX paper, code in omen-org/
**Complexity**: High (5-7 days for learned components)

---

### 2.4 Compaction

**Purpose**: Merge SSTables, reduce read/write amplification

#### Strategy: Lazy Leveling (Dostoevsky Paper)

```
Level 0-1: Tiered (write-optimized)
  - Multiple SSTables with overlapping keys
  - Merge only when level full

Level 2-5: Leveled (read-optimized)
  - Largest level: disjoint key ranges
  - Compact when level ratio exceeded
  - Ratio T=10 (Level N+1 = 10x Level N)
```

#### Workload-Aware Optimization (Bourbon/Tucana)

```rust
pub struct WorkloadAnalyzer {
    key_distribution: Histogram,  // Track access patterns
    write_read_ratio: f64,
    value_size_distribution: Histogram,
}

impl CompactionStrategy {
    fn should_compact(&self, level: usize, analyzer: &WorkloadAnalyzer) -> bool {
        // Analyze workload, adapt level ratios
        match analyzer.workload_type() {
            WorkloadType::WritehHeavy => {
                // More tiering (reduce write amp)
            }
            WorkloadType::ReadHeavy => {
                // More leveling (reduce read amp)
            }
            WorkloadType::Mixed => {
                // Lazy leveling (balanced)
            }
        }
    }
}
```

#### Cost-Benefit Analyzer (Bourbon Paper)

```rust
pub struct CostBenefitAnalyzer {
    training_cost: Duration,  // Time to train models
    space_benefit: f64,  // Space saved by learned components
    query_benefit: f64,  // Query speedup
}

impl CostBenefitAnalyzer {
    fn should_train_models(&self, sstable: &SSTable) -> bool {
        // Only train on large, long-lived SSTables
        sstable.size() > 10_000_keys &&
        sstable.level() >= 3  // Lower levels only
    }
}
```

**Reference**:
- Lazy Leveling: ai/research/PAPERS.md (Dostoevsky)
- Workload-aware: ai/research/PAPERS.md (Bourbon, Tucana)
**Complexity**: High (7-10 days for adaptive logic)

---

### 2.5 Key-Value Separation (WiscKey)

**Purpose**: Reduce write amplification for large values

#### Architecture

```
Small values (<4KB): Stay in LSM tree
Large values (≥4KB): Stored in separate vLog

┌─────────────┐
│  LSM Tree   │
│  (keys +    │
│   pointers) │
└──────┬──────┘
       │
       │ pointer = (vlog_file_id, offset, size)
       │
┌──────▼──────┐
│   vLog      │  ← Append-only value log
│  (large     │
│   values)   │
└─────────────┘
```

#### Implementation

```rust
pub struct ValueLog {
    current_file: File,
    file_id: u64,
    offset: u64,
    gc_threshold: f64,  // Trigger GC when fragmentation > threshold
}

pub struct ValuePointer {
    file_id: u64,
    offset: u64,
    size: u32,
}

impl ValueLog {
    async fn append(&mut self, value: &[u8]) -> Result<ValuePointer> {
        // Append value to current log file
        // Return pointer to location
    }

    async fn read(&self, ptr: ValuePointer) -> Result<Bytes> {
        // Random read from vlog (may be slower)
        // Cache recent values
    }

    fn should_gc(&self) -> bool {
        // Measure fragmentation (dead space / total space)
        self.fragmentation() > self.gc_threshold
    }

    async fn garbage_collect(&mut self) -> Result<()> {
        // Scan vlog, rewrite live values
        // Update pointers in LSM tree
    }
}
```

**Features**:
- **Threshold**: Values ≥4KB go to vlog (configurable)
- **GC strategy**: Independent of compaction (unlike fjall)
- **Smart GC**: Hot/cold detection, fragmentation-based triggering
- **Trade-off**: Slower random reads (acceptable for append-heavy workload)

**Reference**: ai/research/PAPERS.md (WiscKey)
**Complexity**: Medium-High (5-7 days)

---

### 2.6 SIMD Optimizations

**Purpose**: 5-10x speedup for hot paths

#### Targets

```rust
// 1. Vectorized key comparison (partition_point)
#[cfg(target_feature = "avx2")]
fn partition_point_simd(keys: &[Bytes], target: &[u8]) -> usize {
    // Compare 4-8 keys in parallel using AVX2
    // Fallback to scalar for remainder
}

// 2. Vectorized bloom filter lookup
#[cfg(target_feature = "avx2")]
fn bloom_contains_simd(filter: &[u64], hashes: &[u64]) -> bool {
    // Check k bits in parallel
}

// 3. Vectorized compression
#[cfg(target_feature = "avx2")]
fn compress_block_simd(data: &[u8]) -> Vec<u8> {
    // SIMD copy/compare in codec
}
```

**Features**:
- Platform detection (AVX2 for x86_64, NEON for ARM)
- Compile-time feature flags
- Fallback to scalar implementation
- Estimate: 5-10x speedup for hot paths

**Reference**: std::simd (nightly), manual intrinsics (stable)
**Complexity**: Medium (3-5 days per optimization)

---

## 3. API Design

### 3.1 RocksDB-Compatible API

```rust
pub struct DB {
    memtable: Arc<Memtable>,
    sstables: Vec<Arc<SSTable>>,
    wal: WAL,
    vlog: ValueLog,
    compaction: CompactionManager,
}

impl DB {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_options(path, Options::default())
    }

    pub fn open_with_options(path: &Path, options: Options) -> Result<Self> {
        // Load existing SSTables
        // Replay WAL
        // Start compaction threads
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        // Write to WAL
        // Insert into memtable
        // Flush if memtable full
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        // Check memtable
        // Check immutable memtables
        // Check SSTables (level by level)
        // Use bloom filter + learned index
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        // Tombstone marker in memtable
    }

    pub fn scan(&self, start: &[u8], end: &[u8]) -> Result<Iterator<Item = (Bytes, Bytes)>> {
        // Merge iterator across memtable + SSTables
    }
}
```

### 3.2 seerdb-Native API (Extended Features)

```rust
pub struct Options {
    // Standard options
    pub memtable_size: usize,
    pub compression: Compression,
    pub block_size: usize,

    // Learned components
    pub enable_learned_bloom: bool,
    pub enable_learned_index: bool,
    pub bloom_training_threshold: usize,  // Min keys to train

    // WiscKey
    pub kv_separation_threshold: usize,  // Default: 4096 bytes
    pub vlog_gc_threshold: f64,  // Default: 0.5 (50% fragmentation)

    // Workload-aware
    pub adaptive_compaction: bool,
    pub workload_profiling: bool,

    // SIMD
    pub enable_simd: bool,  // Auto-detect platform
}

impl DB {
    pub fn stats(&self) -> Stats {
        // Compaction stats, workload analysis, space amp, write amp
    }

    pub fn analyze_workload(&self) -> WorkloadReport {
        // Key distribution, access patterns, recommendations
    }

    pub fn compact_range(&self, start: &[u8], end: &[u8]) -> Result<()> {
        // Manual compaction
    }
}
```

---

## 4. Module Structure

```
src/
├── lib.rs                # Public API
├── db.rs                 # Main DB struct
├── wal/
│   ├── mod.rs            # WAL writer
│   ├── reader.rs         # WAL recovery
│   └── record.rs         # WAL record format
├── memtable/
│   ├── mod.rs            # Memtable implementation
│   └── skiplist.rs       # Skiplist (or use crossbeam_skiplist)
├── sstable/
│   ├── mod.rs            # SSTable reader/writer
│   ├── block.rs          # Data block format
│   ├── index.rs          # Index (binary search)
│   ├── learned_index/    # ALEX-style learned index
│   │   ├── mod.rs
│   │   ├── model.rs      # Piecewise linear models
│   │   └── trainer.rs    # Model training
│   ├── bloom.rs          # Traditional bloom filter
│   └── learned_bloom/    # ML-based bloom filter
│       ├── mod.rs
│       ├── model.rs      # Decision tree/NN
│       └── trainer.rs    # Model training
├── compaction/
│   ├── mod.rs            # Compaction manager
│   ├── leveled.rs        # Leveled compaction
│   ├── tiered.rs         # Tiered compaction
│   ├── lazy_leveling.rs  # Lazy leveling (hybrid)
│   ├── workload.rs       # Workload analyzer
│   └── cba.rs            # Cost-Benefit Analyzer
├── vlog/
│   ├── mod.rs            # Value log manager
│   ├── writer.rs         # Append to vlog
│   ├── reader.rs         # Read from vlog
│   └── gc.rs             # Garbage collection
├── cache/
│   ├── mod.rs            # Block cache (LRU)
│   └── learned_cache.rs  # Learned replacement (future)
├── simd/
│   ├── mod.rs            # SIMD dispatcher
│   ├── avx2.rs           # AVX2 implementations
│   ├── neon.rs           # ARM NEON implementations
│   └── scalar.rs         # Fallback implementations
└── util/
    ├── mod.rs
    ├── crc.rs            # Checksums
    └── varint.rs         # Variable-length integers
```

---

## 5. Implementation Roadmap

### Phase 1: Core Engine (Weeks 5-8)

**Goal**: Match fjall baseline (438k writes/sec)

**Week 5: WAL + Memtable**
- [ ] WAL writer with CRC32 checksums
- [ ] WAL reader/recovery
- [ ] Memtable (using crossbeam_skiplist)
- [ ] Flush memtable to disk
- [ ] Tests: crash recovery, concurrent writes

**Week 6: SSTable (Traditional)**
- [ ] SSTable format (blocks, index, bloom)
- [ ] Block compression (LZ4)
- [ ] Traditional bloom filter (10 bits/key)
- [ ] Binary search index
- [ ] Block cache (LRU)
- [ ] Tests: read/write/scan correctness

**Week 7: Compaction (Leveled)**
- [ ] Basic leveled compaction
- [ ] Level size management (ratio T=10)
- [ ] Compaction scheduling (background threads)
- [ ] Tests: compaction correctness, data integrity

**Week 8: Integration + Benchmarking**
- [ ] Full CRUD API
- [ ] Range scans (merge iterator)
- [ ] Snapshots
- [ ] Benchmark vs fjall (target: match 438k writes/sec)
- [ ] Tests: end-to-end correctness

---

### Phase 2: Learned Components (Weeks 9-12)

**Goal**: 2-3x improvement over fjall with learned components

**Week 9: Learned Bloom Filters**
- [ ] Integrate prototype (src/bloom/learned.rs)
- [ ] Train during compaction (>10k keys)
- [ ] Cost-Benefit Analyzer (skip training on small SSTables)
- [ ] Fallback to traditional if training fails
- [ ] Benchmark: validate 90% space reduction
- [ ] Tests: false positive rate, correctness

**Week 10: Learned Index (ALEX)**
- [ ] Port ALEX code from omen-org/
- [ ] Piecewise linear model training
- [ ] Integrate into SSTable format
- [ ] Fallback to binary search
- [ ] Benchmark: validate 1-3x faster lookups
- [ ] Tests: lookup correctness, edge cases

**Week 11: Integration**
- [ ] Combine learned bloom + learned index
- [ ] Measure space/time improvements
- [ ] Tune model parameters (tree depth, segment size)
- [ ] Benchmark vs fjall: target 2-3x improvement
- [ ] Tests: end-to-end with learned components

**Week 12: Workload Awareness**
- [ ] Workload analyzer (key distribution, access patterns)
- [ ] Adaptive model selection (when to use learned vs traditional)
- [ ] Cost-Benefit Analyzer tuning
- [ ] Benchmark on omen vector workload
- [ ] Tests: adaptive behavior validation

---

### Phase 3: Optimizations (Weeks 13-16)

**Goal**: 5-10x improvement with WiscKey + SIMD + adaptive compaction

**Week 13: WiscKey KV Separation**
- [ ] Value log implementation (append, read, GC)
- [ ] Threshold: values ≥4KB → vlog
- [ ] Smart GC (fragmentation-based)
- [ ] Benchmark: validate 10-100x write amp reduction
- [ ] Tests: large value workloads, GC correctness

**Week 14: SIMD Optimizations**
- [ ] SIMD key comparison (AVX2/NEON)
- [ ] SIMD bloom filter lookup
- [ ] SIMD compression (if time permits)
- [ ] Platform detection + fallback
- [ ] Benchmark: validate 5-10x hot path speedup
- [ ] Tests: correctness on different platforms

**Week 15: Lazy Leveling + Workload-Aware Compaction**
- [ ] Lazy leveling (upper tiered, lower leveled)
- [ ] Workload analyzer integration
- [ ] Adaptive level ratios
- [ ] Benchmark: validate 3-5x write amp reduction
- [ ] Tests: different workload patterns

**Week 16: Tuning + Validation**
- [ ] End-to-end benchmarking vs fjall
- [ ] Optimize parameters (level ratios, thresholds)
- [ ] Measure space/write/read amplification
- [ ] Validate 5-10x improvement claims
- [ ] Tests: stress testing, corner cases

---

### Phase 4: Integration (Weeks 17-18)

**Goal**: Migrate omen from RocksDB to seerdb

**Week 17: omen Migration**
- [ ] RocksDB-compatible shim (if needed)
- [ ] Migrate omen codebase
- [ ] Run omen test suite
- [ ] Benchmark on real vector workload
- [ ] Identify omen-specific optimizations

**Week 18: Polish + Launch**
- [ ] Documentation (README, examples, API docs)
- [ ] Performance guide (tuning, configuration)
- [ ] Write blog post (research summary + results)
- [ ] Announce on GitHub, Hacker News, Reddit

---

## 6. Testing Strategy

### Unit Tests
- Each module has tests (WAL, memtable, SSTable, compaction)
- Test edge cases (empty keys, large values, deletions)
- Test learned components (model training, fallback)

### Integration Tests
- CRUD operations (put, get, delete, scan)
- Crash recovery (WAL replay)
- Compaction correctness (no data loss)
- Concurrent access (multi-threaded)

### Property Tests (proptest)
- Random operations, verify consistency
- Compare output with in-memory B+tree (reference implementation)

### Stress Tests
- Large datasets (100M+ keys)
- Long-running compaction
- Memory limits (constrained environments)

### Benchmarks (criterion)
- YCSB workloads (A, B, C, D, E, F)
- omen vector workload (real data)
- Compare vs fjall, RocksDB, sled

---

## 7. Performance Targets

### Conservative (Just Learned Components)
- Sequential writes: **1-2M ops/sec** (2-5x fjall)
- Random reads: **2-3M ops/sec** (2-4x fjall)
- Mixed 50/50: **1-1.5M ops/sec** (2-3x fjall)
- Space: **50% bloom filter reduction**

### Aggressive (All Optimizations)
- Sequential writes: **2-4M ops/sec** (5-10x fjall)
- Random reads: **5-10M ops/sec** (5-10x fjall)
- Mixed 50/50: **2-3M ops/sec** (3-5x fjall)
- Space: **90% bloom filter reduction, 10x less write amp**

---

## 8. Risk Mitigation

### Fallback Mechanisms
- Traditional bloom filter (if learned training fails)
- Binary search index (if learned index fails)
- Leveled compaction (if workload analysis inconclusive)

### Incremental Rollout
- Phase 1: Core engine without learned components
- Phase 2: Add learned components one at a time
- Phase 3: Combine all optimizations

### Validation
- Compare output with RocksDB on same workload
- Use proptest for correctness verification
- Stress test on large datasets

---

## Conclusion

seerdb architecture is designed to combine proven LSM-tree techniques (fjall/RocksDB foundation) with 2018-2024 research innovations. Clear module boundaries, fallback mechanisms, and incremental rollout reduce risk while enabling 5-10x performance improvements over fjall baseline.

**Next Steps**:
1. Begin Phase 1 implementation (WAL + memtable)
2. Use fjall as reference for correctness
3. Target: Match fjall baseline by Week 8, then add learned components

---

**References**:
- Research papers: ai/research/PAPERS.md
- fjall analysis: ai/research/FJALL_ANALYSIS.md
- Competitive advantages: ai/COMPETITIVE_ADVANTAGES.md
- Benchmarks: ai/research/BENCHMARKS.md
