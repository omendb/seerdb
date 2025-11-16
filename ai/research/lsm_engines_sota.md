# LSM Storage Engines - State of the Art Research

**Date**: November 14, 2025
**Phase**: 1 of 4 (LSM Engines for seerdb-core)
**Purpose**: Comprehensive analysis of SOTA LSM-tree designs for public general-purpose storage engine

---

## Executive Summary

**Goal**: Design seerdb-core as a production-grade, public (MIT) general-purpose LSM storage engine that:
- Outperforms RocksDB (already demonstrated: 2.47× writes, 2.07× reads)
- Uses modern techniques (pointer swizzling, variable-size pages, optimized compaction)

**Key Finding**: Don't reinvent the wheel. Adopt proven patterns from LeanStore (buffer management), Umbra (variable pages), modern LSM research (WiscKey value separation, PebblesDB fragmented compaction, LSM-Bush adaptive ratios), and latest 2024 innovations (Bf-Tree mini-pages, Keigo workload-aware placement).

**Applicable to**:
- **seerdb-core** (public): All findings (buffer management, compaction, WAL, MVCC)
- vector applications: Buffer management for L0 memory management, WAL patterns
- **oadb**: WAL patterns for incremental save, buffer management concepts for mmap

---

## Core Papers Analyzed

### 1. LeanStore (ICDE 2018, VLDB 2023, PACMMOD 2025)

**The Gold Standard for Buffer Management**

**Core Innovation**: Pointer Swizzling Without Hash Tables

Traditional buffer managers:
```
Access page → Hash table lookup → Get address → Access data
              (expensive!)
```

LeanStore:
```
Hot page in memory? → Direct pointer access (one if statement)
Cold page on disk?  → Load and swizzle pointer
```

**Key Techniques**:

1. **Swizzled Pointers (SWIP)**
   ```rust
   enum Swip {
       Hot(*mut Page),      // Direct pointer to in-memory page
       Cold(u64),           // Disk offset for evicted page
   }

   fn get_page(&mut self, pid: PageID) -> &mut Page {
       match self.mapping[pid] {
           Swip::Hot(ptr) => unsafe { &mut *ptr },  // O(1) access
           Swip::Cold(offset) => {
               let page = self.load_from_disk(offset);
               let ptr = self.insert_to_cache(page);
               self.mapping[pid] = Swip::Hot(ptr);
               unsafe { &mut *ptr }
           }
       }
   }
   ```

2. **Optimistic Lock Coupling (OLC)**
   - No fine-grained latches on hot path
   - Scales linearly on multi-core CPUs
   - Versioned latches for structure modifications only

   ```rust
   struct OptimisticLatch {
       version_lock: AtomicU64,
   }

   impl OptimisticLatch {
       fn read_version(&self) -> u64 {
           self.version_lock.load(Ordering::Acquire)
       }

       fn validate(&self, old_version: u64) -> bool {
           let current = self.version_lock.load(Ordering::Acquire);
           current == old_version && !is_locked(current)
       }

       fn try_lock(&self) -> Option<u64> {
           let version = self.read_version();
           if is_locked(version) { return None; }

           let locked = version | LOCK_BIT;
           if self.version_lock.compare_exchange(version, locked, ...).is_ok() {
               Some(version)
           } else {
               None
           }
       }
   }
   ```

3. **Speculative Unswizzling**
   - Keeps hot pages in memory without tracking in shared data structures
   - Avoids central point of contention
   - Background thread unswizzles cold pages

4. **NVMe Optimization**
   - io_uring for async I/O (Linux)
   - Can saturate multiple NVMe SSDs (5+ GB/s per device)
   - Direct I/O to bypass kernel page cache

**Performance**:
- In-memory: Matches pure in-memory systems when data fits in RAM
- Out-of-memory: Smoothly degrades, fully exploits SSD bandwidth
- TPC-C: Near-zero overhead vs in-memory B-tree
- 40-60% of traditional system time spent in buffer management → pointer swizzling eliminates most of this

**Recent Advances (2023-2025)**:

**Autonomous Commits** (PACMMOD 2025):
- Replaces group commit for high-throughput, low-latency on NVMe
- Traditional: Buffer writes → Wait for batch → Flush all at once (high throughput, adds latency)
- Autonomous: Each transaction commits independently as soon as ready (high throughput AND low latency)

**Scalable Snapshot Isolation** (VLDB 2023):
- Memory-optimized MVCC for OLTP workloads
- Version chains stored exclusively in memory
- Falls back to different scheme for bulk operations

**Source**: https://github.com/leanstore/leanstore (MIT License)

**Applies to seerdb-core**: ✅ Core buffer manager architecture
**Applies to oadb**: ✅ Buffer concepts for mmap-friendly persistence

---

### 2. WiscKey (FAST 2016)

**Value Separation for SSD-Optimized LSM**

**Problem**: Traditional LSM-trees have high write amplification because they rewrite entire key-value pairs during compaction, even though only keys are needed for search.

**Solution**: Separate keys from values
- **Keys**: Stored in LSM-tree (sorted, compacted normally)
- **Values**: Stored in separate value log (vLog), appended sequentially
- **LSM-tree entry**: Key + pointer to value in vLog

**Architecture**:
```
LSM-Tree (Keys + Pointers):
┌─────────────────────────────┐
│ Level 0: [k1→v_offset_100]  │
│          [k2→v_offset_150]  │
│ Level 1: [k3→v_offset_200]  │
│          [k4→v_offset_250]  │
└─────────────────────────────┘

Value Log (Sequential):
┌─────────────────────────────┐
│ offset 100: [value1_data]   │
│ offset 150: [value2_data]   │
│ offset 200: [value3_data]   │
│ offset 250: [value4_data]   │
└─────────────────────────────┘
```

**Benefits**:
- **2.5-111× faster database loading** than LevelDB
- **1.6-14× faster random lookups**
- **Reduced write amplification**: Only keys are rewritten during compaction
- **Better for large values**: Small overhead for key-pointer pairs

**Trade-offs**:
- Range scans slower (need to seek to vLog for each value)
- Garbage collection needed for vLog (deleted/updated values leave holes)

**Optimizations**:
1. **Parallel Range Queries**: Prefetch values from vLog in parallel
2. **vLog Garbage Collection**: Compact vLog by copying live values to new log
3. **Crash Consistency**: Synchronous writes to vLog, asynchronous to LSM-tree

**Performance** (500M key-value pairs, 45 GB):
- LevelDB: 1,868 GB write I/O (42× amplification)
- RocksDB: 1,222 GB write I/O (27× amplification)
- WiscKey: 756 GB write I/O (17× amplification)

**Applies to seerdb-core**: ✅ For large metadata values (if metadata > 1KB)
**Applies to oadb**: ❌ Not applicable (embedded, no large values)

---

### 3. PebblesDB (SOSP 2017)

**Fragmented LSM-Trees (FLSM) for Lower Write Amplification**

**Problem**: Traditional LSM-trees merge entire levels during compaction, causing high write amplification.

**Solution**: Fragmented Log-Structured Merge Trees (FLSM)
- Inspired by Skip Lists
- Organize logs using "guards" (key ranges)
- Avoid rewriting data within the same level
- Only compact across levels when necessary

**Architecture**:
```
Traditional LSM (Leveling):
Level 0: [Run 1] [Run 2]
         ↓ merge entire level
Level 1: [Merged Run] (rewrites all data)

FLSM (PebblesDB):
Level 0: [Run 1: a-f] [Run 2: g-m] [Run 3: n-z]
         ↓ selective merge based on guards
Level 1: [Guard: a-f] [Guard: g-m] [Guard: n-z]
         (only merge overlapping guards)
```

**Key Concepts**:
1. **Guards**: Key ranges that organize fragments
2. **Fragmented Levels**: Multiple non-overlapping runs per level
3. **Selective Compaction**: Only compact overlapping fragments

**Performance** (vs RocksDB):
- **6.7× higher write throughput**
- **2.4-3× lower write amplification**
- Point lookups: Slightly slower (need to check more fragments)
- Range scans: Similar performance

**Use Cases**:
- Write-heavy workloads (logging, time-series, event streams)
- Applications where write throughput > read latency priority

**Integration with NoSQL Stores**:
- MongoDB with PebblesDB: 18-105% higher throughput, 35-55% less write I/O
- HyperDex with PebblesDB: Similar gains

**Applies to seerdb-core**: ✅ Optional compaction strategy for write-heavy workloads
**Applies to oadb**: ❌ Not applicable (embedded, not write-optimized)

---

### 4. FASTER (SIGMOD 2018)

**Hybrid Log-Structured Store with In-Place Updates**

**Innovation**: Combines log-structuring (good for external storage) with in-place updates (good for in-memory performance).

**Architecture**:
```
┌─────────────────────────────────────┐
│     Hash Index (Cache-Optimized)    │
│  [key1→offset] [key2→offset] ...    │
└─────────────────────────────────────┘
           ↓
┌─────────────────────────────────────┐
│         Hybrid Log                  │
│  ┌──────────────────────────┐       │
│  │ Tail (RAM)              │       │
│  │ In-place updates         │       │
│  └──────────────────────────┘       │
│  ┌──────────────────────────┐       │
│  │ Read-Only (RAM)         │       │
│  │ No updates, can copy     │       │
│  └──────────────────────────┘       │
│  ┌──────────────────────────┐       │
│  │ Head (SSD/Cloud)        │       │
│  │ Read-copy-update         │       │
│  └──────────────────────────┘       │
└─────────────────────────────────────┘
```

**Key Features**:
1. **Hybrid Log Design**:
   - **Tail** (in memory): In-place updates for hot records
   - **Read-Only Region** (in memory): Records get one more chance before eviction
   - **Head** (on disk): Read-copy-update for cold records

2. **Cache-Friendly Hash Index**:
   - Cache-line-sized buckets (64 bytes)
   - 8-byte entries with hash tags + logical pointers
   - Latch-free concurrent access

3. **Epoch-Based Reclamation**:
   - Threads register in epochs
   - Safe memory reclamation without locks

**Performance**:
- **160M operations/sec on single machine** (point reads + blind updates)
- Exceeds pure in-memory data structures when working set fits in memory
- Orders of magnitude faster than Redis, RocksDB for mixed workloads

**Use Cases**:
- Cloud applications with temporal locality (hot records in memory, cold on disk)
- IoT state management (billions of devices, per-device counters)
- Real-time analytics pipelines

**Applies to seerdb-core**: ✅ Hybrid log concept for hot/cold tiering
**Applies to oadb**: ❌ Not applicable (embedded, no tiered storage)

---

### 5. LSM-Bush & Wacky Continuum (SIGMOD 2019)

**Adaptive Compaction for Scalable Write Performance**

**Problem**: Fixed capacity ratios between LSM levels cause:
- Write cost increases with data size: O(log N)
- Exponentially diminishing returns for point reads and memory

**Solution**: LSM-Bush uses **increasing capacity ratios** between adjacent pairs of smaller levels.

**Architecture**:
```
Traditional LSM (fixed ratio T=10):
L0: 10 MB    (ratio: 10×)
L1: 100 MB   (ratio: 10×)
L2: 1 GB     (ratio: 10×)
L3: 10 GB    (ratio: 10×)
L4: 100 GB

LSM-Bush (doubly-exponential ratios):
L0: 10 MB    (ratio: 2×)
L1: 20 MB    (ratio: 4×)
L2: 80 MB    (ratio: 8×)
L3: 640 MB   (ratio: 16×)
L4: 10.24 GB (ratio: 32×)
```

**Benefits**:
- **Write cost**: O(log N) → O(log log N)
- Can trade write savings for better point reads or lower memory
- More scalable as data size grows

**Wacky Design Continuum**:
- Encompasses all merge policies from laziest to greediest
- Includes LSM-Bush, Tiering, Leveling, Lazy Leveling
- Can be searched analytically to find optimal design for given workload

**Compaction Strategies**:
1. **Tiering**: Lazy merging, lowest write amplification, highest read amplification
2. **Leveling**: Greedy merging, highest write amplification, lowest read amplification
3. **Lazy Leveling**: Hybrid approach
4. **LSM-Bush**: Adaptive ratios for best scalability

**Applies to seerdb-core**: ✅ Configurable compaction policies
**Applies to oadb**: ❌ Not applicable (no LSM structure)

---

### 6. Umbra (CIDR 2020+)

**Variable-Size Pages for Heterogeneous Workloads**

Built on LeanStore foundation with key extension: **variable-size pages**.

**Size Classes**:
```
Size Class 0: 64 KB   (smallest, most common)
Size Class 1: 128 KB  (2× larger)
Size Class 2: 256 KB  (2× larger)
...
Size Class N: Up to buffer pool size
```

**Benefits**:
- Small pages for hot transactional data (better cache utilization)
- Large pages for bulk analytical data (fewer page operations)
- No levels of indirection like fixed-size pages
- Reduces fragmentation and memory waste

**Implementation**:
```rust
struct VariableSizePageManager {
    size_classes: Vec<SizeClass>,
}

struct SizeClass {
    page_size: usize,       // 64KB, 128KB, etc.
    free_pages: Vec<Page>,
    allocator: BuddyAllocator,
}

impl VariableSizePageManager {
    fn alloc_page(&mut self, data_size: usize) -> PageID {
        let size_class = self.select_size_class(data_size);
        size_class.allocate()
    }

    // Can grow/shrink pages dynamically
    fn resize_page(&mut self, pid: PageID, new_size: usize) {
        // Move to different size class if needed
    }
}
```

**Additional Features**:
1. **Adaptive Query Compilation**:
   - Flying Start: Fast single-pass x86 codegen for short queries
   - LLVM: Optimized compilation for long-running queries
   - Auto-select based on query cost

2. **Worst-Case Optimal Joins (WCOJ)**:
   - Detects large intermediate results in binary joins
   - Uses multi-way joins via hash-trie indexes
   - Faster for complex join patterns

3. **Memory-Optimized MVCC**:
   - Version chains in memory only
   - Local mapping tables per page
   - Fallback scheme for bulk operations

**Performance** (vs HyPer):
- 3.0× geometric mean speedup on JOB benchmark
- 1.8× speedup on TPC-H
- In-memory speed when working set fits in RAM
- Scales to datasets much larger than memory

**Applies to seerdb-core**: ✅ Variable-size pages for mixed workloads
**Applies to oadb**: ❌ Not applicable (no page-based storage)

---

### 7. Colibri (VLDB 2024)

**Hybrid Row/Column Storage for HTAP Workloads**

**Problem**: OLTP (row storage) vs OLAP (column storage) trade-off.

**Solution**: Hybrid storage that adapts based on data age and access patterns.

**Architecture**:
```
┌─────────────────────────────────────┐
│         Hot Data (Rows)              │
│  Recent inserts/updates in row format│
│  Fast point lookups, transactions    │
└─────────────────────────────────────┘
            ↓ aging
┌─────────────────────────────────────┐
│    Cold Data (Compressed Columns)    │
│  Older data in columnar format       │
│  Fast scans, aggregations            │
└─────────────────────────────────────┘
```

**Key Features**:
1. **Lightweight Compression**:
   - Frame of reference encoding
   - Dictionary encoding for strings
   - No heavyweight compression (LZ4, Snappy)
   - Can evaluate predicates on compressed data

2. **Automatic Hot/Cold Separation**:
   - Monitors access patterns
   - Migrates cold data to columnar format
   - Transparent to applications

3. **Cloud Optimization**:
   - Works efficiently on S3/Azure Blob Storage
   - Async scans exploit object store bandwidth
   - Cost-effective for cloud deployments

**Performance** (Ch-benCHmark: TPC-C + TPC-H):
- Colibri: Handles both OLTP and OLAP well
- Pure row stores: Good OLTP, terrible OLAP (10× slower)
- Pure column stores: Terrible OLTP (4× slower), good OLAP
- **10× improvement for hybrid workloads**

**Source**: https://github.com/umbra-db/colibri-vldb2024

**Applies to seerdb-core**: ❌ Not needed (not HTAP workload)
**Applies to oadb**: ❌ Not applicable

---

### 8. BonsaiKV (VLDB 2024)

**Tiered Memory for Heterogeneous Hardware**

**New Hardware Trend**: NUMA/CXL-based systems with heterogeneous memory:
- **DRAM**: Fast (ns), expensive, volatile
- **NVMM**: Slower than DRAM (μs), cheaper, persistent
- **NVMe SSD**: Slowest (ms), cheapest, persistent

**BonsaiKV Approach**:
```
Hot data    → DRAM
Warm data   → NVMM (Non-Volatile Main Memory)
Cold data   → NVMe SSD
Auto-tiering based on access patterns
```

**Benefits**:
- Combines speed of DRAM with capacity of SSD
- Data persistence without write amplification
- Lower cost per GB than pure DRAM systems

**Applies to seerdb-core**: ⏭️ Future consideration (CXL not widely available yet)
**Applies to oadb**: ❌ Not applicable

---

### 9. Recent VLDB 2024 Innovations

#### Bf-Tree (VLDB 2024)

**Modern Read-Write-Optimized B-Tree**

**Key Insight**: Separate cache pages from disk pages.

**Innovation**:
- Cache page ≠ mirror of disk page
- Cache page = judiciously chosen subset worth caching
- Uses **mini-pages** (variable-length buffer pool)

**Benefits**:
- Record-level caching (not full pages)
- Buffers recent updates efficiently
- Caches range gaps
- Mirrors disk pages when needed

**Performance** (vs RocksDB & B-Tree):
- **2.5× faster scans** than RocksDB (LSM-Tree)
- **6× faster writes** than traditional B-Tree
- **2× faster point lookups** than both

**Applies to seerdb-core**: ✅ Mini-page concept for variable-size records
**Applies to oadb**: ❌ Not applicable

---

#### Keigo (VLDB 2024)

**Workload-Aware Storage Placement for LSM**

**Observation**: No one-size-fits-all placement across storage hierarchy.

**Solution**: Place files across devices based on:
- Parallelism requirements
- I/O bandwidth needs
- Capacity constraints

**Techniques**:
1. **Concurrency-Aware Placement**: High-parallelism files → fast devices
2. **Persistent Read-Only Caching**: Cache frequently read SSTables
3. **Context-Based I/O Differentiation**: Compaction I/O vs query I/O prioritization

**Performance** (vs RocksDB, LevelDB, Speedb):
- **4× throughput improvement** for write-heavy workloads
- **18× throughput improvement** for read-heavy workloads
- Works with heterogeneous storage (NVMe + SSD + HDD)

**Applies to seerdb-core**: ✅ Workload-aware file placement
**Applies to oadb**: ❌ Not applicable

---

#### BACH (VLDB 2024)

**LSM-Trees for Hybrid Graph Transactional/Analytical Processing**

**Problem**: Graph databases need both transactional (adjacency list) and analytical (CSR format) storage.

**Solution**: Expand LSM-Tree design space:
- **Upper levels**: Adjacency list (TP-friendly)
- **Lower levels**: CSR format (AP-friendly)
- **Compaction**: Transforms layout during propagation

**Applies to seerdb-core**: ❌ Graph-specific, not applicable
**Applies to oadb**: ❌ Not applicable

---

## RocksDB Optimizations (Production Patterns)

### Bloom Filters
- Per-SSTable bloom filters for fast negative lookups
- Reduces disk I/O by ~90% for non-existent keys
- Configurable bits per key (10 bits = 1% false positive rate)

### Tiered Compaction
- Level 0-2: SSD (hot data, fast compaction)
- Level 3-6: Cloud storage (cold data, infrequent compaction)
- RocksDB-Cloud: Production-proven S3 tiering (Rockset built billion-scale DB on it)

### DeleteRange
- Efficient range deletions without rewriting data
- Tombstone ranges instead of individual delete markers
- Compaction removes entire range in one pass

### Compression
- Per-level compression policies
- Level 0-1: No compression (fast writes)
- Level 2-4: LZ4 (fast decompression, moderate compression)
- Level 5-6: Zstd (high compression, slower decompression)

### Prefix Bloom Filters
- Optimized for range queries with common prefixes
- Skips SSTables that don't contain prefix
- Useful for time-series data (prefix = timestamp range)

**Applies to seerdb-core**: ✅ All patterns (bloom filters, tiered compaction, DeleteRange, compression)
**Applies to oadb**: ❌ Not applicable

---

## Synthesis: What to Build in seerdb-core

### Must Have (Core Features)

1. **LeanStore-Style Buffer Manager**
   - Pointer swizzling (SWIP: Hot/Cold)
   - Optimistic lock coupling
   - Speculative unswizzling
   - **Rationale**: 40-60% performance win on buffer management

2. **Variable-Size Pages** (Umbra)
   - Size classes: 64KB, 128KB, 256KB, 512KB
   - Dimension-aware for specialized forks
   - **Rationale**: Reduces fragmentation, better cache utilization

3. **Async I/O**
   - Linux: io_uring (mandatory for SOTA)
   - macOS: POSIX AIO fallback
   - Direct I/O to bypass kernel page cache
   - **Rationale**: Saturate NVMe bandwidth (5+ GB/s)

4. **WAL with Autonomous Commits**
   - No group commit batching
   - Per-transaction immediate flush on NVMe
   - **Rationale**: High throughput + low latency

5. **Bloom Filters**
   - Per-SSTable filters
   - 10 bits/key default (1% false positive)
   - **Rationale**: 90% reduction in unnecessary disk I/O

6. **Configurable Compaction Policies**
   - Leveling (default): Greedy, low read amp
   - Tiering: Lazy, low write amp
   - LSM-Bush: Adaptive ratios
   - **Rationale**: Workload-dependent optimization

### Nice to Have (Future Optimizations)

1. **Value Separation** (WiscKey)
   - For large metadata values (> 1KB)
   - Separate vLog for values
   - **When**: If metadata becomes large

2. **Fragmented Compaction** (PebblesDB)
   - FLSM for write-heavy workloads
   - Guards-based selective merging
   - **When**: Streaming vector ingestion bottlenecks on writes

3. **Workload-Aware Placement** (Keigo)
   - Concurrency-aware file placement
   - Context-based I/O differentiation
   - **When**: Heterogeneous storage (NVMe + SSD)

4. **Mini-Pages** (Bf-Tree)
   - Record-level caching
   - Variable-length buffer pool
   - **When**: Mixed record sizes become issue

### Skip for Now

1. ❌ **Full MVCC**: Not needed for vector inserts/updates initially
2. ❌ **HTAP Hybrid Storage** (Colibri): Not HTAP workload
3. ❌ **Tiered Memory** (BonsaiKV): CXL not widely available
4. ❌ **Graph-Specific Features** (BACH): Not graph database

---

## Implementation Roadmap

### Phase 1: Core Buffer Manager (Weeks 1-2)
```rust
// Deliverables:
- PageID, Swip enum
- Pointer swizzling logic
- Page allocation/deallocation
- Simple eviction policy (CLOCK)
- Can load/evict pages from disk
```

### Phase 2: I/O Layer (Week 3)
```rust
// Deliverables:
- io_uring integration (Linux)
- Fallback to sync I/O (macOS)
- Page flush on eviction
- Basic WAL for durability
- Persistent storage with recovery
```

### Phase 3: LSM Structure (Weeks 4-5)
```rust
// Deliverables:
- MemTable (in-memory sorted buffer)
- SSTable format with bloom filters
- Leveling compaction strategy
- Background compaction threads
- Working LSM-tree with persistence
```

### Phase 4: Optimizations (Week 6)
```rust
// Deliverables:
- Variable-size pages (64KB-256KB)
- Autonomous commits for WAL
- Bloom filter tuning
- Performance benchmarks vs RocksDB
- Production-ready storage engine
```

---

## Key Takeaways for Each Repository

### seerdb (Public, MIT)
**Focus**: General-purpose LSM with SOTA buffer management
- ✅ Pointer swizzling (LeanStore)
- ✅ Variable-size pages (Umbra)
- ✅ Configurable compaction (LSM-Bush, PebblesDB)
- ✅ Bloom filters (RocksDB)
- ✅ Autonomous commits (LeanStore 2025)
- ✅ io_uring async I/O (Linux)

**Focus**: Fork of seerdb with vector optimizations
- ✅ Dimension-aware page sizes (extends Umbra variable pages)
- ✅ SIMD-aligned vector storage
- ✅ LSM-VEC compaction hooks (connectivity-aware)
- ✅ Hot/cold tiering with RaBitQ compression
- ✅ Graph-aware storage layouts (DiskANN patterns)

**Focus**: LSM-VEC using specialized storage
- ✅ Buffer management patterns for L0 memory
- ✅ RocksDB-Cloud tiering patterns (L0-L2 SSD, L3-L6 S3)
- ✅ Workload-aware placement (Keigo)
- ✅ Compaction strategies for streaming ingestion

### oadb (Embedded Vector Database)
**Focus**: HNSW in-memory, simple persistence
- ✅ WAL patterns for incremental save (autonomous commits concept)
- ✅ Buffer management ideas for mmap-friendly format
- ❌ No LSM structure needed (not tiered storage)

---

## Open Questions to Resolve

1. **MVCC for seerdb-core**: Do we need full transactional support, or just crash recovery?
   - **Lean towards**: Crash recovery only initially, add MVCC if users request it

2. **Value separation threshold**: What size should trigger vLog separation?
   - **Lean towards**: 1KB threshold (metadata > 1KB goes to vLog)

3. **Compaction policy default**: Leveling, Tiering, or LSM-Bush?
   - **Lean towards**: Leveling (default), expose config for workload tuning

4. **io_uring on macOS**: Wait for native support or stick with POSIX AIO?
   - **Lean towards**: POSIX AIO fallback, monitor macOS io_uring progress

---

## References

**LeanStore & Related**:
- Leis et al., "LeanStore: In-Memory Data Management Beyond Main Memory", ICDE 2018
- Alhomssi & Leis, "Scalable and Robust Snapshot Isolation", VLDB 2023
- Nguyen et al., "Autonomous Commits for High Throughput", PACMMOD 2025
- Source: https://github.com/leanstore/leanstore (MIT)

**LSM Optimizations**:
- Lu et al., "WiscKey: Separating Keys from Values in SSD-conscious Storage", FAST 2016
- Raju et al., "PebblesDB: Building Key-Value Stores using Fragmented LSM-Trees", SOSP 2017
- Chandramouli et al., "FASTER: A Concurrent Key-Value Store with In-Place Updates", SIGMOD 2018
- Dayan & Idreos, "The Log-Structured Merge-Bush & the Wacky Continuum", SIGMOD 2019
- Sarkar et al., "Constructing and Analyzing the LSM Compaction Design Space", VLDB 2021

**Modern Storage Engines**:
- Neumann & Freitag, "Umbra: A Disk-Based System with In-Memory Performance", CIDR 2020
- Schmidt et al., "Colibri: Hybrid Cloud Storage for HTAP", VLDB 2024
- Hao & Chandramouli, "Bf-Tree: A Modern Read-Write-Optimized Concurrent Range Index", VLDB 2024
- Adão et al., "Keigo: Co-designing LSM KVS with Storage Hierarchy", VLDB 2024
- Huang et al., "BACH: Bridging Adjacency List and CSR Format using LSM-Trees", VLDB 2024

**RocksDB**:
- Facebook, "RocksDB Documentation", https://rocksdb.org/
- Rockset, "RocksDB-Cloud", https://github.com/rockset/rocksdb-cloud

---

**END OF LSM ENGINES SOTA RESEARCH**

This document provides complete foundation for seerdb-core architecture decisions.
Next: Phase 2 (Vector Storage), Phase 3 (Embedded DBs), Phase 4 (General Storage Engine).
