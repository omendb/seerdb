# seerdb-core Architecture Specification

**Repository**: https://github.com/omendb/seerdb
**License**: Apache 2.0 (open source)
**Purpose**: General-purpose LSM storage engine with SOTA buffer management
**Status**: Design specification (implementation pending)
**Last Updated**: November 14, 2025

**Research Foundation**: See `ai/research/lsm_engines_sota.md` (Phase 1)

---

## Executive Summary

**seerdb-core** is a production-grade LSM storage engine that:
- **Beats RocksDB**: Already demonstrated 2.47× faster writes, 2.07× faster reads
- **Uses modern techniques**: Pointer swizzling (LeanStore), variable-size pages (Umbra), autonomous commits
- **General-purpose**: Foundation for vector, time-series, and embedded applications

**Key Design Principles**:
1. **Performance first**: Adopt proven SOTA techniques (LeanStore buffer manager)
2. **Production-ready**: WAL, crash recovery, ACID guarantees
3. **Open source**: Apache 2.0 license, clean codebase, well-documented
4. **Extensible**: Modular design for workload-specific optimizations

---

## System Architecture

```
┌──────────────────────────────────────────────┐
│         Client API (Rust)                     │
│  - put(key, value)                            │
│  - get(key) → value                           │
│  - delete(key)                                │
│  - scan(start, end) → iterator                │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│         MemTable (L0, in-memory)              │
│  - SkipList or B-tree                         │
│  - Write buffer (configurable size: 64-256MB) │
│  - Flush to L1 when full                      │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│    LeanStore Buffer Manager (Core Innovation)│
│  - Pointer swizzling (SWIP: Hot/Cold)         │
│  - Variable-size pages (64KB-512KB)           │
│  - Optimistic Lock Coupling (OLC)             │
│  - CLOCK or 2Q eviction policy                │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│         LSM-Tree Levels (L1-L6)               │
│  - L1-L2: SSD (hot data, 10% of total)        │
│  - L3-L6: HDD/S3 (cold data, 90% of total)    │
│  - Bloom filters per SSTable                  │
│  - Configurable compaction (Leveling/Tiering) │
└──────────────────────────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│       I/O Layer (io_uring + WAL)              │
│  - io_uring (Linux) for async I/O             │
│  - POSIX AIO fallback (macOS)                 │
│  - Autonomous commits (no group commit)       │
│  - Crash recovery via WAL replay              │
└──────────────────────────────────────────────┘
```

---

## Core Components

### 1. Buffer Manager (LeanStore-based)

**Why**: 40-60% performance win over hash table-based buffer managers.

**Design**:
```rust
/// Page identifier (logical)
type PageID = u64;

/// Swizzled pointer (hot = RAM, cold = disk)
enum Swip {
    Hot(*mut Page),   // Direct pointer (O(1) access)
    Cold(u64),        // Disk offset (needs I/O)
}

struct BufferManager {
    /// PID → SWIP mapping
    page_table: HashMap<PageID, Swip>,

    /// In-memory page pool
    page_pool: Vec<Page>,

    /// Eviction policy (CLOCK or 2Q)
    evictor: EvictionPolicy,

    /// Max memory usage (configurable)
    max_memory: usize,
}
```

**Key Operations**:
- `get_page(pid)` → Swizzle if cold (load from disk), return pointer
- `unswizzle(pid)` → Flush if dirty, update mapping (hot → cold)
- `evict()` → Select victim (CLOCK/2Q), unswizzle, free memory

**Reference**: `ai/research/lsm_engines_sota.md` § LeanStore, `ai/research/general_storage_engine_sota.md` § Buffer Manager

---

### 2. Variable-Size Pages (Umbra-inspired)

**Why**: Reduce fragmentation, optimize for heterogeneous data (small keys, large values).

**Size Classes**:
- **64 KB**: Small records (keys + small values)
- **128 KB**: Medium records
- **256 KB**: Large records (values > 64 KB)
- **512 KB**: Very large records (metadata, blobs)

**Allocation**:
```rust
impl BufferManager {
    fn alloc_page(&mut self, data_size: usize) -> PageID {
        let size_class = match data_size {
            0..=65536 => PageSize::KB64,
            65537..=131072 => PageSize::KB128,
            131073..=262144 => PageSize::KB256,
            _ => PageSize::KB512,
        };

        self.allocate_from_size_class(size_class)
    }
}
```

**Reference**: `ai/research/lsm_engines_sota.md` § Umbra

---

### 3. Compaction Strategies (Configurable)

**Default**: **Leveling** (greedy, low read amplification)

**Available Policies**:

| Policy | Write Amp | Read Amp | Use Case |
|--------|-----------|----------|----------|
| Leveling | High | Low | Read-heavy workloads (default) |
| Tiering | Low | High | Write-heavy workloads |
| LSM-Bush | Very Low | Medium | Massive scale (1B+ records) |

**Configuration**:
```rust
struct CompactionConfig {
    policy: CompactionPolicy,  // Leveling, Tiering, or LSMBush
    size_ratio: usize,         // Level size ratio (default: 10)
    max_levels: usize,         // Max LSM levels (default: 6)
    bloom_bits: usize,         // Bits per key for bloom filters (default: 10)
}
```

**Reference**: `ai/research/lsm_engines_sota.md` § LSM-Bush & Wacky Continuum

---

### 4. I/O Subsystem

**Linux (Primary)**: **io_uring** for async I/O
- Saturates NVMe bandwidth (5+ GB/s)
- Batch I/O operations for efficiency
- Direct I/O (bypass kernel page cache)

**macOS (Fallback)**: POSIX AIO
- Synchronous I/O acceptable (development only)
- Production deployments use Linux

**Implementation**:
```rust
#[cfg(target_os = "linux")]
struct IOLayer {
    ring: io_uring::IoUring,
    fd: RawFd,
}

#[cfg(target_os = "macos")]
struct IOLayer {
    fd: RawFd,
    // Synchronous fallback
}
```

**Reference**: `ai/research/lsm_engines_sota.md` § LeanStore, `ai/research/general_storage_engine_sota.md` § I/O Subsystem

---

### 5. WAL & Crash Recovery

**WAL Design**: **Autonomous Commits** (no group commit batching)
- Each transaction commits immediately (low latency)
- Exploits NVMe parallelism (high throughput)
- WAL file: `<dbname>.wal`

**Recovery**:
1. Find last checkpoint in WAL
2. Replay log records from checkpoint
3. Reconstruct MemTable state
4. Database ready for use

**Checkpointing**:
- Periodic flush of dirty pages (every 10K writes or 5 minutes)
- Truncate WAL after successful checkpoint
- Crash recovery time: O(records since last checkpoint)

**Reference**: `ai/research/lsm_engines_sota.md` § Autonomous Commits

---

### 6. Bloom Filters

**Purpose**: Reduce disk I/O for non-existent keys (~90% reduction)

**Configuration**:
- **10 bits per key** (default) = 1% false positive rate
- **Configurable**: 8 bits (2%), 12 bits (0.5%), 16 bits (0.01%)

**Per-SSTable**:
- Each SSTable has own bloom filter
- Stored in SSTable metadata section
- Loaded into memory on SSTable open

**Reference**: `ai/research/lsm_engines_sota.md` § RocksDB Optimizations

---

## API Design

### Public API (Rust)

```rust
/// Open or create database
pub fn open(path: &Path) -> Result<Database>;

/// Get value for key
pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;

/// Put key-value pair
pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()>;

/// Delete key
pub fn delete(&mut self, key: &[u8]) -> Result<()>;

/// Scan range [start, end)
pub fn scan(&self, start: &[u8], end: &[u8]) -> Result<Iterator<Item = (Vec<u8>, Vec<u8>)>>;

/// Flush MemTable to disk (checkpoint)
pub fn flush(&mut self) -> Result<()>;

/// Close database (flushes, syncs WAL)
pub fn close(self) -> Result<()>;
```

### Configuration API

```rust
pub struct Config {
    /// Compaction policy
    pub compaction: CompactionConfig,

    /// Buffer pool size (default: 1 GB)
    pub buffer_pool_size: usize,

    /// MemTable size (default: 64 MB)
    pub memtable_size: usize,

    /// Bloom filter bits per key (default: 10)
    pub bloom_bits: usize,

    /// I/O threads (default: num_cpus)
    pub io_threads: usize,
}
```

---

## Implementation Roadmap

### Phase 1: Core LSM (Weeks 1-2)
- MemTable (SkipList)
- SSTable format (header + bloom + data + index)
- Leveling compaction
- Basic I/O (synchronous)

**Deliverable**: Working LSM-tree, no durability

### Phase 2: Buffer Manager (Weeks 3-4)
- Page table with SWIP
- Pointer swizzling logic
- CLOCK eviction policy
- Variable-size pages (64KB, 128KB, 256KB)

**Deliverable**: LeanStore-style buffer manager

### Phase 3: Durability (Week 5)
- WAL implementation
- Autonomous commits
- Crash recovery
- io_uring integration (Linux)

**Deliverable**: Production-ready durability

### Phase 4: Optimizations (Week 6)
- Bloom filters per SSTable
- Tiering & LSM-Bush compaction policies
- Benchmarking vs RocksDB
- Performance tuning

**Deliverable**: SOTA performance, ready for public release

---

## Performance Targets

| Metric | Target | Baseline (RocksDB) | Status |
|--------|--------|-------------------|--------|
| Sequential writes | > 2.0× faster | 1.0× (baseline) | ✅ 2.47× (achieved) |
| Random reads | > 1.5× faster | 1.0× (baseline) | ✅ 2.07× (achieved) |
| Write amplification | < 15× | 20-30× | ⏭️ Measure after compaction tuning |
| Memory overhead | < 1.5× data size | 2-3× | ⏭️ Measure with buffer manager |

**Note**: Existing seerdb already beats RocksDB. This design refactors for SOTA techniques.

---

## Open Questions

1. **MVCC for seerdb-core**:
   - Do we need full transactional support?
   - **Lean towards**: Crash recovery only initially, add MVCC if users request

2. **Value separation threshold** (WiscKey):
   - What size triggers vLog separation?
   - **Lean towards**: 1 KB threshold (metadata > 1KB goes to vLog), add if needed

3. **Eviction policy**:
   - CLOCK (simple) vs 2Q (better)?
   - **Lean towards**: CLOCK first, upgrade to 2Q if benchmarks show benefit

4. **Public release timeline**:
   - After Phase 4 complete? Or earlier alpha?
   - **Lean towards**: Alpha after Phase 2, Beta after Phase 4

---

## References

**Primary Research**: `ai/research/lsm_engines_sota.md` (Phase 1)
- LeanStore papers (ICDE 2018, VLDB 2023, PACMMOD 2025)
- Umbra (CIDR 2020)
- LSM-Bush (SIGMOD 2019)
- RocksDB documentation

**Implementation Guide**: `ai/research/general_storage_engine_sota.md` (Phase 4)
- Buffer manager code examples
- OLC implementation
- io_uring patterns
- WAL & recovery

**Source Code References**:
- LeanStore: https://github.com/leanstore/leanstore (MIT)
- RocksDB: https://github.com/facebook/rocksdb (Apache 2.0)

---

**Last Updated**: November 14, 2025
**Status**: Design complete, ready for implementation
