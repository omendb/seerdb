# Storage Engine Research Summary
**Date:** November 14, 2025  
**Context:** Research for seerdb (vector-optimized storage engine) and experimental general-purpose storage engine

---

## Executive Summary

This document summarizes research on modern storage engine architectures, with focus on:
1. **Bw-tree** design and production usage
2. **State-of-the-art** storage engines (LeanStore, Umbra, Colibri - VLDB 2024)
3. **Recommendations** for seerdb refactor
4. **Blueprint** for experimental general-purpose storage engine in Mojo

**Key Finding:** Don't build a traditional Bw-tree. Instead, adopt LeanStore's pointer swizzling + Umbra's variable-size pages + Colibri's hybrid storage for best-in-class performance.

---

## 1. Bw-Tree Analysis

### What is Bw-Tree?

**Bw-tree** (Buzz Word Tree) is a latch-free B+-tree variant designed for:
- Multi-core CPUs (no locks, uses CAS operations)
- Flash storage (log-structured writes)
- High concurrency (lock-free delta chains)

**Key Innovations:**
1. **Mapping Table**: Indirection layer (Page ID → Memory Address)
2. **Delta Updates**: Never update pages in-place; prepend deltas
3. **Log-Structured Storage**: Batch writes to flash for efficiency
4. **Lock-Free Operations**: All modifications via atomic CAS

### Architecture

```
┌─────────────────────────────────────────┐
│         Mapping Table (PID → Addr)       │
│  [PID 1 → 0x1000] [PID 2 → 0x2000] ... │
└─────────────────────────────────────────┘
              ↓ CAS operations
┌─────────────────────────────────────────┐
│              Delta Chain                 │
│  [Update δ] → [Insert δ] → [Base Page]  │
└─────────────────────────────────────────┘
              ↓ consolidation
┌─────────────────────────────────────────┐
│         Flash (Log-Structured)           │
│  [Page 1] [Page 2] [Delta Records] ...  │
└─────────────────────────────────────────┘
```

### Production Usage

**Microsoft Products:**
- **SQL Server Hekaton** (In-Memory OLTP) - Uses Bw-tree for range indexes
  - Achieved 10-30x speedups over traditional SQL Server
  - Indexes not persisted to disk, rebuilt at recovery
  - Combined with lock-free hash tables for point lookups

- **Azure Cosmos DB** - Uses Bw-tree for inverted and forward indexes
  - Combined with LSM-trees for Bw-tree modifications on SSD
  - Supports schema-agnostic indexing
  - Handles multi-model data (JSON, graph, key-value)

- **Azure DocumentDB, Bing** - Also use Bw-tree implementations

**Outside Microsoft:** Very limited adoption. CMU's Peloton project has an open-source implementation, but no major production systems outside Microsoft ecosystem use Bw-trees.

### Why Bw-Trees Haven't Spread

1. **Complexity**: Delta chains require careful consolidation policies
2. **Memory overhead**: Mapping table + delta chains consume significant RAM
3. **Limited benefits on modern NVMe**: Log-structured design optimized for slower SSDs
4. **Better alternatives emerged**: LeanStore/Umbra achieve similar goals with simpler designs

### Verdict: Don't Use Bw-Trees

For new storage engines in 2025, Bw-trees are **outdated**. Modern approaches (covered below) achieve better performance with less complexity.

---

## 2. State-of-the-Art Storage Engines (2024-2025)

### LeanStore (TUM, VLDB 2024)

**The Gold Standard for Buffer Management**

**Core Innovation: Pointer Swizzling Without Hash Tables**

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

**Key Features:**

1. **Optimistic Lock Coupling**
   - No fine-grained latches on hot path
   - Scales linearly on multi-core CPUs
   - Uses versioned latches for structure modifications

2. **Speculative Unswizzling**
   - Keeps hot pages in memory without tracking in shared data structures
   - Avoids central point of contention

3. **NVMe Optimization**
   - Directly exploits NVMe bandwidth (5+ GB/s per device)
   - io_uring for async I/O (Linux)
   - Can saturate multiple NVMe SSDs

4. **Performance**
   - In-memory performance: matches pure in-memory systems when data fits in RAM
   - Out-of-memory: smoothly degrades, fully exploits SSD bandwidth
   - TPC-C: Near-zero overhead compared to in-memory B-tree

**Recent Advances (2025):**
- **Autonomous Commits**: Replaces group commit for high-throughput, low-latency on NVMe
- **Scalable Snapshot Isolation**: Memory-optimized MVCC for OLTP workloads

**Source Code:** https://github.com/leanstore/leanstore (MIT License)

---

### Umbra (TUM, CIDR 2020+)

**Disk-Based System with In-Memory Performance**

Built on LeanStore foundation with extensions:

**Variable-Size Pages:**
```
Size Class 0: 64 KB   (smallest, most common)
Size Class 1: 128 KB  (2x larger)
Size Class 2: 256 KB  (2x larger)
...
Size Class N: Up to buffer pool size
```

**Why Variable Sizes?**
- Small pages for hot transactional data
- Large pages for bulk analytical data
- No levels of indirection (unlike fixed-size pages)
- Better cache utilization

**Additional Features:**

1. **Adaptive Query Compilation**
   - Flying Start backend: Fast compilation for short queries (single-pass x86 codegen)
   - LLVM backend: Optimized compilation for long-running queries
   - Automatically chooses based on query cost

2. **Worst-Case Optimal Joins (WCOJ)**
   - Detects when binary joins create large intermediate results
   - Uses multi-way joins via hash-trie indexes
   - Significantly faster for complex join patterns

3. **Memory-Optimized MVCC**
   - Version chains stored exclusively in memory
   - Associated with pages through local mapping tables
   - Falls back to different scheme for bulk operations

**Performance (vs HyPer):**
- 3.0x geometric mean speedup on JOB benchmark
- 1.8x speedup on TPC-H
- In-memory speed when working set fits in RAM
- Scales to datasets much larger than memory

---

### Colibri (TUM, VLDB 2024)

**Hybrid Storage for HTAP Workloads**

**The Problem:** Most systems optimize for either OLTP or OLAP, not both:
- OLTP systems: Row storage, fast updates, poor analytics
- OLAP systems: Columnar storage, fast scans, slow updates
- ETL pipelines: Introduce delay between transaction and analytics

**Colibri's Solution: Hybrid Column-Row Store**

```
┌─────────────────────────────────────────┐
│           Hot Data (Rows)                │
│  Recent inserts/updates in row format    │
│  Fast point lookups and transactions     │
└─────────────────────────────────────────┘
              ↓ aging
┌─────────────────────────────────────────┐
│        Cold Data (Compressed Columns)    │
│  Older data in columnar format           │
│  Fast scans and aggregations             │
└─────────────────────────────────────────┘
```

**Key Design Decisions:**

1. **Lightweight Compression**
   - Frame of reference encoding
   - Dictionary encoding for strings
   - No heavyweight compression (LZ4, Snappy)
   - Can evaluate predicates on compressed data

2. **Data Format Comparison (TPC-H SF100, lineitem table):**
   - Pure Row: Best for updates, worst for scans
   - Pure Column: Best for scans, worst for updates
   - Colibri Hybrid: 90% of column scan performance, 95% of row update performance

3. **Automatic Hot/Cold Separation**
   - Monitors access patterns
   - Migrates cold data to columnar format automatically
   - Transparent to applications

**Performance Results:**

Benchmark: Ch-benCHmark (combined TPC-C + TPC-H)
- Colibri: Handles both OLTP and OLAP well
- Pure row stores: Good OLTP, terrible OLAP (10x slower)
- Pure column stores: Terrible OLTP (4x slower), good OLAP
- Colibri achieves **10x improvement** for hybrid workloads

**Cloud Optimization:**
- Works efficiently on S3/Azure Blob Storage
- Async scans exploit object store bandwidth
- Cost-effective for cloud deployments

**Source Code:** https://github.com/umbra-db/colibri-vldb2024

---

### BonsaiKV (VLDB 2024)

**Tiered Memory Systems**

**New Hardware Trend:** NUMA/CXL-based systems with heterogeneous memory:
- DRAM: Fast, expensive, volatile
- NVMM (Non-Volatile Main Memory): Slower than DRAM, cheaper, persistent
- NVMe SSD: Slowest, cheapest, persistent

**BonsaiKV's Approach:**
1. Hot data in DRAM
2. Warm data in NVMM
3. Cold data on NVMe
4. Automatic tiering based on access patterns

**Benefits:**
- Combines speed of DRAM with capacity of SSD
- Data persistence without write amplification
- Lower cost per GB than pure DRAM systems

---

## 3. Blueprint for State-of-the-Art Storage Engine

### Essential Components

#### Component 1: Buffer Manager (Foundation)

**Based on LeanStore's Pointer Swizzling**

```rust
struct BufferManager {
    // Page mapping: PID → SWIP (Swizzled Pointer)
    mapping_table: HashMap<PageID, Swip>,
    
    // Page cache (in-memory pages)
    page_cache: Vec<Page>,
    
    // Eviction policy (CLOCK, 2Q, or LRU)
    eviction: EvictionPolicy,
    
    // Free page list
    free_list: Vec<PageID>,
}

enum Swip {
    // Page in memory: direct pointer
    Hot(*mut Page),
    
    // Page on disk: offset in storage file
    Cold(u64),
}

impl BufferManager {
    // O(1) page access when hot, load when cold
    fn get_page(&mut self, pid: PageID) -> &mut Page {
        match self.mapping_table[pid] {
            Swip::Hot(ptr) => unsafe { &mut *ptr },
            Swip::Cold(offset) => {
                let page = self.load_from_disk(offset);
                let ptr = self.insert_to_cache(page);
                self.mapping_table[pid] = Swip::Hot(ptr);
                unsafe { &mut *ptr }
            }
        }
    }
    
    // Evict cold pages when memory pressure
    fn evict_page(&mut self) {
        let victim = self.eviction.select_victim();
        self.flush_to_disk(victim);
        self.mapping_table[victim.pid] = Swip::Cold(offset);
    }
}
```

**Variable-Size Pages (Umbra-style):**

```rust
struct VariableSizePageManager {
    // Size classes: 64KB, 128KB, 256KB, 512KB, ...
    size_classes: Vec<SizeClass>,
}

struct SizeClass {
    page_size: usize,       // 64KB, 128KB, etc.
    free_pages: Vec<Page>,
    allocator: BuddyAllocator,
}

impl VariableSizePageManager {
    // Allocate page of appropriate size
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

**Why This Matters:**
- 40-60% of execution time in traditional systems is buffer management
- Pointer swizzling eliminates hash table lookups on hot path
- Variable-size pages reduce fragmentation and memory waste

---

#### Component 2: Index Structures

**Option A: B+-Tree with Optimistic Lock Coupling**

```rust
struct BPlusTree {
    root: NodeID,
    node_manager: NodeManager,
}

struct Node {
    version: AtomicU64,      // For optimistic validation
    keys: Vec<Key>,
    children: Vec<NodeID>,   // Or values for leaf nodes
}

impl BPlusTree {
    fn lookup(&self, key: &Key) -> Option<Value> {
        loop {
            // Optimistic: read without locks
            let (node, version) = self.read_node_optimistic(self.root);
            
            // Traverse to leaf
            let leaf = self.traverse_to_leaf(node, key);
            let (leaf_node, leaf_version) = self.read_node_optimistic(leaf);
            
            // Search in leaf
            let value = leaf_node.search(key);
            
            // Validate versions haven't changed
            if self.validate_versions(node, version, leaf_node, leaf_version) {
                return value;
            }
            // Retry if validation failed
        }
    }
    
    fn insert(&mut self, key: Key, value: Value) {
        // Optimistic traversal, CAS for modifications
        // Only hold latches during structure modifications (splits)
    }
}
```

**Option B: ART (Adaptive Radix Tree)**

Better for string keys, space-efficient, cache-friendly. Used by Umbra for superior string indexing performance.

**Option C: Skip List**

Lock-free, simpler than B-tree, good for write-heavy workloads. Used by LevelDB/RocksDB.

---

#### Component 3: Storage Layout

**For General-Purpose: PAX (Partition Attributes Across)**

Hybrid between row and column storage:

```
Traditional Row Store (NSM):
┌─────────────────────────────┐
│ [r1.a, r1.b, r1.c, r1.d]    │
│ [r2.a, r2.b, r2.c, r2.d]    │
│ [r3.a, r3.b, r3.c, r3.d]    │
└─────────────────────────────┘

Pure Column Store (DSM):
┌─────────────────────────────┐
│ [r1.a, r2.a, r3.a, ...]     │
│ [r1.b, r2.b, r3.b, ...]     │
│ [r1.c, r2.c, r3.c, ...]     │
└─────────────────────────────┘

PAX (within each page):
┌─────────────────────────────┐
│ Mini-page A: [r1.a, r2.a, r3.a] │
│ Mini-page B: [r1.b, r2.b, r3.b] │
│ Mini-page C: [r1.c, r2.c, r3.c] │
└─────────────────────────────┘
```

**Benefits:**
- Sequential scans on single column (good cache locality)
- Can reconstruct full tuple from mini-pages
- Better compression than pure row store
- Reasonable update performance

**For Vector Workloads (seerdb-specific):**

Dimension-aware layouts:

```rust
struct VectorPage {
    dimension: u16,           // 128, 384, 1536
    count: u32,               // # vectors in page
    
    // SIMD-aligned vector data
    vectors: AlignedVectors,  // Aligned to 64-byte cache line
    
    // Optional: quantized/compressed format
    compressed: Option<RaBitQData>,
}

impl VectorPage {
    // Page size adapts to dimension
    fn optimal_page_size(dimension: u16) -> usize {
        match dimension {
            128 => 64 * 1024,    // 64KB for 128D
            384 => 128 * 1024,   // 128KB for 384D
            1536 => 256 * 1024,  // 256KB for 1536D
            _ => 128 * 1024,     // Default
        }
    }
}
```

---

#### Component 4: Concurrency Control

**Minimum: Optimistic Lock Coupling (OLC)**

No locks on read path, locks only for structure modifications:

```rust
struct OptimisticLatch {
    version_lock: AtomicU64,
}

impl OptimisticLatch {
    // Read: just read version
    fn read_version(&self) -> u64 {
        self.version_lock.load(Ordering::Acquire)
    }
    
    // Validate: check version hasn't changed
    fn validate(&self, old_version: u64) -> bool {
        let current = self.version_lock.load(Ordering::Acquire);
        current == old_version && !is_locked(current)
    }
    
    // Write: CAS to acquire lock
    fn try_lock(&self) -> Option<u64> {
        let version = self.read_version();
        if is_locked(version) {
            return None;
        }
        
        let locked = version | LOCK_BIT;
        if self.version_lock.compare_exchange(version, locked, ...).is_ok() {
            Some(version)
        } else {
            None
        }
    }
    
    // Release: increment version
    fn unlock(&self, old_version: u64) {
        self.version_lock.store(old_version + 1, Ordering::Release);
    }
}
```

**Better: Memory-Optimized MVCC (Umbra-style)**

For transaction isolation:

```rust
struct Transaction {
    read_ts: u64,                          // Read timestamp
    write_set: HashMap<Key, Version>,      // Buffered writes
}

struct Version {
    data: Value,
    begin_ts: u64,     // When version became valid
    end_ts: u64,       // When version became invalid
    next: Option<Box<Version>>,  // Version chain
}

impl Transaction {
    fn read(&self, key: &Key) -> Option<Value> {
        // Check write set first
        if let Some(version) = self.write_set.get(key) {
            return Some(version.data.clone());
        }
        
        // Read from version chain
        let mut version = self.get_version_chain(key);
        while let Some(v) = version {
            if v.begin_ts <= self.read_ts && self.read_ts < v.end_ts {
                return Some(v.data.clone());
            }
            version = v.next.as_deref();
        }
        None
    }
    
    fn commit(&mut self) -> bool {
        // Optimistic validation
        for (key, version) in &self.write_set {
            if !self.validate_no_conflicts(key) {
                return false; // Abort
            }
        }
        
        // Install writes
        let commit_ts = self.allocate_commit_timestamp();
        for (key, version) in self.write_set.drain() {
            self.install_version(key, version, commit_ts);
        }
        true
    }
}
```

---

#### Component 5: Logging & Recovery

**Write-Ahead Log (WAL) with Autonomous Commits**

Traditional group commit:
```
Buffer writes → Wait for batch → Flush all at once
(High throughput, but adds latency)
```

Autonomous commit (LeanStore 2025):
```
Each transaction commits independently as soon as ready
(High throughput AND low latency on NVMe)
```

**Implementation:**

```rust
struct WAL {
    log_file: File,
    log_buffer: Vec<LogRecord>,
    lsn: AtomicU64,  // Log Sequence Number
}

struct LogRecord {
    lsn: u64,
    txn_id: u64,
    record_type: RecordType,  // INSERT, UPDATE, DELETE, COMMIT
    data: Vec<u8>,
}

impl WAL {
    // Append log record (autonomous commit)
    async fn append(&mut self, record: LogRecord) -> u64 {
        let lsn = self.lsn.fetch_add(1, Ordering::SeqCst);
        
        // Write directly to NVMe (no batching)
        self.log_file.write_all(&record.serialize()).await;
        
        // Force to disk immediately for durability
        self.log_file.sync_data().await;
        
        lsn
    }
    
    // Recovery: replay log from last checkpoint
    fn recover(&mut self) -> Result<()> {
        let last_checkpoint = self.find_last_checkpoint();
        
        for record in self.read_from(last_checkpoint) {
            match record.record_type {
                RecordType::Insert => self.redo_insert(record),
                RecordType::Update => self.redo_update(record),
                RecordType::Delete => self.redo_delete(record),
                _ => {}
            }
        }
        Ok(())
    }
}
```

**Checkpointing:**

Periodically flush all dirty pages to disk to reduce recovery time:

```rust
impl BufferManager {
    fn checkpoint(&mut self) {
        // Flush all dirty pages
        for page in self.dirty_pages() {
            self.flush_to_disk(page);
        }
        
        // Write checkpoint record to WAL
        self.wal.append(LogRecord::checkpoint());
    }
}
```

---

#### Component 6: I/O Subsystem

**Linux: io_uring (Mandatory for SOTA)**

```rust
use io_uring::{IoUring, opcode, types};

struct AsyncIO {
    ring: IoUring,
}

impl AsyncIO {
    async fn read_page(&mut self, offset: u64) -> Result<Page> {
        let mut buffer = vec![0u8; PAGE_SIZE];
        
        // Submit read operation
        let read_op = opcode::Read::new(
            types::Fd(self.fd),
            buffer.as_mut_ptr(),
            PAGE_SIZE as u32
        ).offset(offset);
        
        unsafe {
            self.ring.submission()
                .push(&read_op.build())
                .expect("submission queue full");
        }
        
        // Submit to kernel
        self.ring.submit_and_wait(1)?;
        
        // Get completion
        let cqe = self.ring.completion().next().expect("completion queue empty");
        
        Ok(Page::from_bytes(&buffer))
    }
    
    // Batch multiple I/O operations
    async fn batch_read(&mut self, pages: &[PageID]) -> Vec<Page> {
        for pid in pages {
            // Submit all reads
            self.submit_read(pid);
        }
        
        self.ring.submit()?;
        
        // Collect all completions
        self.collect_completions(pages.len())
    }
}
```

**macOS: POSIX AIO or synchronous I/O**

```rust
// Fallback for macOS (no io_uring)
impl AsyncIO {
    fn read_page_sync(&self, offset: u64) -> Result<Page> {
        let mut buffer = vec![0u8; PAGE_SIZE];
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buffer)?;
        Ok(Page::from_bytes(&buffer))
    }
}
```

**Direct I/O (bypass kernel page cache):**

```rust
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;

let file = OpenOptions::new()
    .read(true)
    .write(true)
    .custom_flags(libc::O_DIRECT)  // Direct I/O
    .open(path)?;
```

---

## 4. Recommendations for seerdb

### Current State
- Unreleased, free to refactor
- Will be used as storage engine for omendb (cloud vector DB with LSM-VEC)
- Focus: Vector-optimized storage

### Recommended Architecture

**Build as "Vector-Optimized LeanStore"**

```
┌─────────────────────────────────────────┐
│     LeanStore-style Buffer Manager       │
│  - Pointer swizzling (no hash tables)    │
│  - Variable-size pages (64KB - 256KB)    │
│  - Optimistic lock coupling              │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│      Vector-Specific Optimizations       │
│  - Dimension-aware page layouts          │
│  - SIMD-aligned vector storage           │
│  - Hot/cold tiering with RaBitQ          │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│          I/O Layer (NVMe)                │
│  - io_uring for async I/O                │
│  - Direct I/O to bypass kernel cache     │
│  - Autonomous commits (no group commit)  │
└─────────────────────────────────────────┘
```

### Implementation Timeline (6 weeks)

**Weeks 1-2: Core Buffer Manager**
```rust
// Phase 1: Basic infrastructure
- PageID, Swip enum
- Pointer swizzling logic
- Page allocation/deallocation
- Simple eviction policy (CLOCK)

// Deliverable: Can load/evict pages from disk
```

**Week 3: I/O Layer**
```rust
// Phase 2: Async I/O
- io_uring integration (Linux)
- Fallback to sync I/O (macOS)
- Page flush on eviction
- Basic WAL for durability

// Deliverable: Persistent storage with recovery
```

**Weeks 4-5: Vector Optimizations**
```rust
// Phase 3: Vector-specific features
- Variable-size pages (dimension-aware)
- SIMD-aligned vector layouts
- Hot tier: In-memory HNSW graph
- Cold tier: RaBitQ compressed storage

// Deliverable: Efficient vector storage
```

**Week 6: Integration & Testing**
```rust
// Phase 4: Polish and integration
- Connect to existing HNSW implementation
- LSM-VEC algorithm integration
- Benchmarking against omendb's current storage
- Performance tuning

// Deliverable: Production-ready storage engine
```

### Key Design Decisions

**1. Page Sizes by Dimension**
```rust
fn page_size_for_dimension(dim: u16) -> usize {
    match dim {
        128 => 64 * 1024,      // 512 vectors per page
        384 => 128 * 1024,     // 341 vectors per page  
        768 => 128 * 1024,     // 170 vectors per page
        1536 => 256 * 1024,    // 170 vectors per page
        _ => 128 * 1024,       // Default
    }
}
```

**2. Hot/Cold Tiering**
```
Recently inserted/updated:
  ↓
HOT TIER (in-memory, HNSW graph)
  - Fast searches
  - Uncompressed vectors
  - Optimized for latency
  
  ↓ (after aging)
  
COLD TIER (on NVMe, compressed)
  - RaBitQ 8x compression
  - DiskANN-style index
  - Optimized for cost
```

**3. Integration with LSM-VEC**

LSM-VEC requires efficient writes and range scans. seerdb should:
- Batch writes to SSTables (like LSM-tree)
- Support bloom filters for fast negative lookups
- Efficient compaction for space reclamation

```rust
struct SeerDB {
    // Memory component (C0)
    memtable: MemTable<VectorKey, VectorValue>,
    
    // Disk components (C1, C2, ...)
    sstables: Vec<SSTable>,
    
    // Buffer manager
    buffer_manager: BufferManager,
    
    // Vector index
    vector_index: HNSWGraph,
}
```

### What to Skip (For Now)

❌ **Full MVCC**: Not needed for vector inserts/updates  
❌ **Complex query optimization**: Queries are mostly vector similarity searches  
❌ **Distributed transactions**: Single-node focus initially  
❌ **SQL parser**: Not a relational database  

### What You Must Have

✅ **Pointer swizzling**: Core performance win  
✅ **Variable-size pages**: Matches vector dimensions  
✅ **Async I/O**: Exploit NVMe bandwidth  
✅ **WAL + recovery**: Durability guarantee  
✅ **Hot/cold tiering**: Cost optimization  

---

## 5. General-Purpose Storage Engine in Mojo

### Motivation

**Why Mojo?**
- Combines Python usability with C++ performance
- SIMD built-in (perfect for data structures)
- Borrow checker (memory safety)
- Auto-tuning capabilities
- 500-2000x speedups reported

**Why Experiment?**
- Learn what a storage engine really needs
- Validate Mojo's performance claims for systems programming
- Generate ideas to backport to seerdb (Rust)

### Minimal SOTA Engine Components

**Must Have (for MVP):**

1. **Buffer Manager** (2-3 weeks)
   - Pointer swizzling
   - Simple eviction (CLOCK)
   - Page loading/flushing

2. **One Index Structure** (1-2 weeks)
   - B+-tree with OLC, OR
   - Skip list (simpler, lock-free)

3. **Storage Layout** (1-2 weeks)
   - PAX for hybrid OLTP/OLAP
   - Fixed-size pages initially

4. **Concurrency** (1 week)
   - Optimistic lock coupling OR
   - Simple MVCC

5. **Logging & Recovery** (1 week)
   - Basic WAL
   - Simple checkpointing

6. **I/O** (integrated throughout)
   - Async I/O (if Mojo supports it)
   - Otherwise, synchronous

**Total Estimate: 6-8 weeks for minimal working engine**

### Mojo Code Structure

```mojo
# Core types
struct PageID:
    var id: UInt64

struct Swip:
    var hot: Pointer[Page]  # In-memory pointer
    var cold: UInt64         # Disk offset

# Buffer manager
struct BufferManager:
    var pages: DynamicVector[Page]
    var mapping: Dict[PageID, Swip]
    
    fn get_page(self, pid: PageID) -> Pointer[Page]:
        let swip = self.mapping[pid]
        if swip.is_hot():
            return swip.hot
        else:
            return self.load_from_disk(swip.cold)
    
    fn evict_page(self):
        # CLOCK eviction
        let victim = self.select_victim()
        self.flush_to_disk(victim)

# B+-tree with OLC
struct BPlusTree:
    var root: NodeID
    
    fn insert(self, key: Key, value: Value):
        # Optimistic traversal
        # CAS for modifications
        pass
    
    fn lookup(self, key: Key) -> Optional[Value]:
        # Lock-free reads
        pass

# Simple transaction
struct Transaction:
    var read_ts: UInt64
    var write_set: Dict[Key, Value]
    
    fn commit(self) -> Bool:
        # Validate and commit
        pass

# WAL
struct WAL:
    var log_file: FileHandle
    var lsn: Atomic[UInt64]
    
    async fn append(self, record: LogRecord) -> UInt64:
        # Autonomous commit
        pass
```

### Approach

**Phase 1: Core (Weeks 1-3)**
- Implement buffer manager with pointer swizzling
- Basic page allocation/deallocation
- Simple B+-tree (no concurrency yet)

**Phase 2: Concurrency (Week 4)**
- Add OLC to B+-tree
- Basic transaction support

**Phase 3: Persistence (Week 5)**
- WAL implementation
- Recovery logic

**Phase 4: I/O (Week 6)**
- Async I/O (if available in Mojo)
- Performance testing

**Phase 5: Evaluation (Week 7-8)**
- Benchmark against equivalent Rust implementation
- Decide: continue in Mojo or port to Rust?

### Success Criteria

**Minimum:**
- Working B+-tree with concurrent reads/writes
- Crash recovery via WAL
- Basic benchmarks (insert, lookup, scan)

**Stretch Goals:**
- Performance competitive with Rust
- Proof that Mojo is viable for systems programming
- Publish findings/blog post

### Mojo-Specific Considerations

**Advantages:**
- SIMD built-in: Fast vector operations for index structures
- Auto-tuning: Can optimize page sizes, cache sizes automatically
- Memory control: Manual allocation like C/C++
- Borrow checker: Prevents memory leaks

**Challenges:**
- Immature ecosystem: You'll write a lot from scratch
- No robust I/O libraries: Need to wrap C libraries or use basic primitives
- Debugging: Tools still developing
- Documentation: Limited compared to Rust

**Risk Mitigation:**
- Timebox to 2 months maximum
- Have Rust fallback ready
- Focus on learning, not production code
- Document everything for future reference

### Decision Point (After 2 Months)

**If Mojo is faster and easier:**
→ Continue development, consider for oadb (embedded DB)

**If Mojo is slower or too difficult:**
→ Port key learnings to Rust
→ Use as research artifact
→ Contribute findings back to Mojo community

---

## 6. Key Takeaways

### For seerdb (Vector-Optimized Storage)

1. **Don't use Bw-trees** - They're outdated, complex, and offer no advantage on modern NVMe
2. **Adopt LeanStore architecture** - Pointer swizzling is the biggest performance win
3. **Add variable-size pages** - Match page sizes to vector dimensions
4. **Implement hot/cold tiering** - Keep recent data in memory, compress cold data with RaBitQ
5. **Use io_uring on Linux** - Essential for saturating NVMe bandwidth
6. **Timeline: 6 weeks** to production-ready storage engine

### For General-Purpose Engine Experiment

1. **Mojo is worth experimenting with** - But keep expectations realistic
2. **Build minimal but complete** - Buffer manager + one index + WAL + recovery
3. **Timebox to 2 months** - Don't get stuck on a research project
4. **Goal is learning** - Not production code (yet)
5. **Have Rust fallback** - Don't bet everything on immature tech

### Comparison Matrix

| Feature | Bw-Tree | LeanStore | Umbra | Colibri | Recommended |
|---------|---------|-----------|--------|---------|-------------|
| Buffer Management | Mapping table | Pointer swizzling | Var-size pages | Hybrid storage | LeanStore base |
| Concurrency | Lock-free deltas | OLC | Memory MVCC | OLC + MVCC | OLC for start |
| Storage | Log-structured | Direct NVMe | Direct NVMe | Row+Column | Vector-aware |
| Complexity | High | Medium | Medium-High | High | Start simple |
| Production Use | Microsoft only | Research | Research | Research | Build from papers |

### Next Steps

**Immediate (This Week):**
1. Review seerdb codebase
2. Identify components to refactor vs. rebuild
3. Set up benchmarking infrastructure

**Phase 1 (Weeks 1-2):**
1. Implement basic buffer manager with pointer swizzling
2. Add page allocation/eviction
3. Create simple test suite

**Parallel Track (Mojo Experiment):**
1. Set up Mojo development environment
2. Implement basic buffer manager
3. Compare with Rust equivalent

**Decision Point (2 months):**
1. Evaluate Mojo experiment results
2. Complete seerdb core implementation
3. Begin integration with omendb

---

## 7. References & Resources

### Papers

**Bw-Tree:**
- Levandoski et al., "The Bw-Tree: A B-tree for New Hardware Platforms", ICDE 2013
- https://www.microsoft.com/en-us/research/publication/the-bw-tree-a-b-tree-for-new-hardware-platforms/

**LeanStore:**
- Leis et al., "LeanStore: In-Memory Data Management Beyond Main Memory", ICDE 2018
- Alhomssi & Leis, "Scalable and Robust Snapshot Isolation", VLDB 2023
- Nguyen et al., "Autonomous Commit for High Throughput", PACMMOD 2025
- Source: https://github.com/leanstore/leanstore

**Umbra:**
- Neumann & Freitag, "Umbra: A Disk-Based System with In-Memory Performance", CIDR 2020
- Freitag et al., "Memory-Optimized Multi-Version Concurrency Control", VLDB 2022
- Website: https://umbra-db.com

**Colibri:**
- Schmidt et al., "Two Birds With One Stone: Designing a Hybrid Cloud Storage Engine for HTAP", VLDB 2024
- Source: https://github.com/umbra-db/colibri-vldb2024

**Other:**
- "BonsaiKV: Tiered, Heterogeneous Memory Systems", VLDB 2024
- Graefe, "Modern B-Tree Techniques", Foundations and Trends in Databases 2011

### Source Code

- LeanStore: https://github.com/leanstore/leanstore (MIT)
- Colibri: https://github.com/umbra-db/colibri-vldb2024
- Bw-Tree (CMU): https://github.com/wangziqi2013/BwTree
- DuckDB: https://github.com/duckdb/duckdb (hybrid engine, good reference)

### Mojo Resources

- Official Site: https://www.modular.com/mojo
- Documentation: https://docs.modular.com/mojo/
- Playground: https://playground.modular.com/
- Community: https://github.com/modularml/mojo

### Your Projects

- omendb: https://github.com/omendb/omendb
- seerdb: https://github.com/omendb/seerdb
- bw-tree experiment: https://github.com/nijaru/bw-tree

---

## 8. Questions for Further Research

**For Claude Code to investigate:**

1. **LSM-VEC Integration**: How exactly does LSM-VEC work, and what storage engine requirements does it impose?

2. **RaBitQ Compression**: Can we do better than 8x? What's the trade-off between compression ratio and search accuracy?

3. **Autonomous Commits**: What's the exact protocol in LeanStore 2025? Can we implement it in seerdb?

4. **Variable-Size Pages**: What's the optimal size progression? 64KB → 128KB → 256KB, or something else?

5. **Mojo I/O**: Does Mojo have native async I/O, or do we need to wrap io_uring via FFI?

6. **Tiered Memory**: Is CXL actually available on common cloud instances? Should we design for it now?

7. **Vector Index Persistence**: What's the best way to checkpoint HNSW graphs to disk?

8. **Benchmarking**: What's the standard benchmark suite for storage engines? How do we measure fairly against RocksDB, WiredTiger?

---

**End of Document**

For Claude Code: This document provides foundation for next phase of research and implementation. Please:
1. Validate technical details against latest research
2. Propose specific APIs for seerdb components
3. Evaluate Mojo feasibility with code samples
4. Create detailed implementation roadmap
