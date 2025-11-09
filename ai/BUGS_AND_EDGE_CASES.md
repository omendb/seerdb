# Bugs and Edge Cases Review - seerdb 0.0.1 Readiness

**Date**: November 8, 2025
**Status**: 🚨 **PRE-ALPHA** - Multiple critical issues found
**Goal**: Identify ALL bugs and edge cases before 0.0.1 release

---

## Executive Summary

**Critical Issues**: 8 found 🚨
**High Priority**: 12 found ⚠️
**Medium Priority**: 15 found ⚠️
**Low Priority**: 8 found 📝

**Estimated Fix Time**: 3-4 weeks for critical + high priority issues

**Recommendation**: DO NOT release 0.0.1 until critical issues fixed

---

## 1. Critical Issues 🚨 (Blockers for 0.0.1)

### 1.1 Batch API Non-Atomicity 🚨

**Issue**: Batch commit is NOT atomic (WAL write separate from memtable apply)

**Impact**: Data corruption, inconsistent state on crash

**Severity**: **CRITICAL** - Can cause data loss

**Test**: None (missing!)

**Fix**: Implement single WAL batch write
```rust
// Add to wal/mod.rs
pub enum Record {
    Put { key: Bytes, value: Bytes },
    Delete { key: Bytes },
    Batch { operations: Vec<Operation> },  // ← NEW
}

// Modify batch.rs commit()
pub fn commit(self) -> Result<()> {
    // Single atomic WAL write
    let batch_record = Record::Batch {
        operations: self.operations.clone()
    };
    self.db.wal_tx.send(batch_record)?;

    // Then apply to memtable
    for op in self.operations {
        // ...
    }
}
```

**Estimated Time**: 2-3 days (includes testing)

---

### 1.2 WAL Recovery Race Condition ✅ FIXED

**Status**: Already fixed in current code

**Issue**: WAL recovery could happen after background threads start

**Location**: `src/db.rs::open()` (lines 463-699)

**Current Implementation** (correct order):
```rust
pub fn open(opts: DBOptions) -> Result<Self> {
    // 1. FIRST: Recover from WAL (line 467)
    Self::recover_partitioned(&wal_path, &memtables_vec)?;

    // 2. Create new WAL (line 476)
    let wal = WAL::create(&wal_path, options.wal_sync_policy)?;

    // 3. Wrap in Arc/ArcSwap (lines 524-538)
    let memtables = Arc::new(memtables_array);
    let wal = Arc::new(Mutex::new(wal));

    // 4. THEN: Start background threads (lines 578-699)
    // - Compaction worker (line 589)
    // - Flush worker (line 637)
    // - WAL writer (line 674)
}
```

**Verification**: Code already has correct order - WAL recovery completes before any background threads start

**Fixed By**: Already correct in initial implementation

---

### 1.3 Memtable Partition Key Distribution Skew 🚨

**Issue**: Hash-based partitioning can cause uneven distribution, leading to single partition bottleneck

**Location**: `src/db.rs::partition_for_key()`

**Problem**:
```rust
fn partition_for_key(key: &[u8]) -> usize {
    let hash = foldhash::fast::FoldHasher::default().hash_one(key);
    (hash as usize) % NUM_PARTITIONS  // ← Can skew!
}
```

**Example Failure**:
```rust
// Sequential keys: key_0000, key_0001, key_0002...
// All hash to same partition if hash function has bad distribution
// Result: 15/16 partitions idle, 1 partition overloaded!
```

**Impact**: Performance degradation, nullifies partitioning benefits

**Test**: None (missing!)

**Fix**: Use better hash distribution OR monitor partition sizes
```rust
// Option 1: Use higher-quality hash
fn partition_for_key(key: &[u8]) -> usize {
    use std::hash::{Hash, Hasher};
    let mut hasher = foldhash::fast::FoldHasher::default();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    // Use higher bits for better distribution
    ((hash >> 32) as usize) % NUM_PARTITIONS
}

// Option 2: Monitor and rebalance
struct DB {
    partition_sizes: Arc<[AtomicUsize; NUM_PARTITIONS]>,
}

fn check_skew(&self) -> f64 {
    let sizes: Vec<_> = self.partition_sizes.iter()
        .map(|s| s.load(Ordering::Relaxed))
        .collect();
    let max = sizes.iter().max().unwrap();
    let min = sizes.iter().min().unwrap();
    *max as f64 / *min as f64  // Skew ratio
}
```

**Estimated Time**: 1-2 days (includes skew monitoring)

---

### 1.4 Compaction Can Delete Live Keys 🚨

**Issue**: If compaction runs while flush is in progress, it may compact away keys that are still in immutable memtables

**Location**: `src/compaction/mod.rs`

**Problem**:
```rust
// Thread 1: Flush in progress
// - Memtable frozen at sequence #1000
// - Flush hasn't started yet

// Thread 2: Compaction starts
// - Compacts L0 → L1
// - Removes keys < sequence #1000
// - Completes and deletes SSTables

// Thread 1: Flush completes
// - Writes SSTable with sequence #1000
// - But compaction already deleted overlapping keys!
// - Data loss!
```

**Impact**: **DATA LOSS** under concurrent flush + compaction

**Test**: None (missing!)

**Fix**: Use proper sequence number coordination
```rust
// Track max flushed sequence
struct DB {
    max_flushed_seq: Arc<AtomicU64>,
}

// Compaction: Only compact up to max flushed sequence
fn compact(&self) -> Result<()> {
    let safe_seq = self.max_flushed_seq.load(Ordering::SeqCst);
    // Only compact SSTables with max_seq <= safe_seq
}

// Flush: Update after SSTable written
fn flush(&self) -> Result<()> {
    let sstable = self.build_sstable()?;
    self.max_flushed_seq.store(sstable.max_seq, Ordering::SeqCst);
}
```

**Estimated Time**: 2-3 days (complex, needs careful testing)

---

### 1.5 Block Cache Missing Concurrent Access Protection 🚨

**Issue**: Block cache uses `Arc<DashMap>` but no protection against concurrent eviction + read

**Location**: `src/db.rs::block_cache`

**Problem**:
```rust
// Thread 1: Reading block
let block = self.block_cache.get(&key)?;  // Got Arc<Block>

// Thread 2: Cache eviction (if we add LRU later)
self.block_cache.remove(&key);  // Removes entry

// Thread 1: Uses block
block.decompress()?;  // ← May use freed memory if no Arc!
```

**Current State**: DashMap holds `Arc<Vec<u8>>` so this is actually safe, but:
- If we change to raw pointers or unsafe later: UB
- If we add LRU eviction: need careful arc management

**Impact**: Potential use-after-free (currently safe due to Arc, but fragile)

**Test**: None (missing!)

**Fix**: Document Arc requirement, add debug asserts
```rust
pub struct BlockCache {
    cache: Arc<DashMap<BlockKey, Arc<Vec<u8>>>>,  // ← Arc required!
}

impl BlockCache {
    pub fn insert(&self, key: BlockKey, block: Vec<u8>) {
        debug_assert!(block.len() > 0, "Empty block inserted");
        self.cache.insert(key, Arc::new(block));
    }
}
```

**Estimated Time**: 4 hours (documentation + asserts)

---

### 1.6 VLog GC Can Cause Read Errors ⏸️ DEFERRED

**Status**: Not implemented yet (no GC code exists)

**Issue**: VLog garbage collection could delete values still referenced by LSM tree

**Location**: `src/vlog/mod.rs` (GC not implemented)

**Current State**: VLog has basic operations (append, read, sync) but NO gc() method

**Problem** (when GC is implemented):
```rust
// Thread 1: Reading value
let ptr = lsm.get(key)?;  // Got ValuePointer { offset: 1000, len: 100 }

// Thread 2: VLog GC runs
vlog.gc()?;  // Rewrites valid values, updates tail pointer
// Offset 1000 is now garbage (value moved to offset 5000)

// Thread 1: Reads value
let value = vlog.read(ptr.offset, ptr.len)?;  // ← Reads garbage!
```

**Impact**: **DATA CORRUPTION** - returns wrong value or garbage

**Test**: None (missing!)

**Fix**: Atomic pointer updates OR GC coordination
```rust
// Option 1: Atomic GC with LSM update
fn gc(&self) -> Result<()> {
    // 1. Scan LSM for live pointers
    let live_pointers = self.lsm.scan_value_pointers()?;

    // 2. Rewrite valid values
    let new_pointers = self.vlog.rewrite_valid(live_pointers)?;

    // 3. Atomically update ALL pointers in LSM
    self.lsm.batch_update_pointers(new_pointers)?;

    // 4. THEN truncate vlog
    self.vlog.truncate()?;
}

// Option 2: Copy-on-read during GC
fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
    let ptr = self.lsm.get_pointer(key)?;

    // Check if pointer is in GC range
    if self.vlog.is_in_gc_range(ptr.offset) {
        // Copy value to new location
        let value = self.vlog.read(ptr)?;
        let new_ptr = self.vlog.append(value.clone())?;
        self.lsm.update_pointer(key, new_ptr)?;
        return Ok(Some(value));
    }

    Ok(self.vlog.read(ptr)?)
}
```

**Estimated Time**: 3-4 days (when GC is implemented)

**Resolution**: Deferred to 0.0.2+ (VLog GC not needed for 0.0.1)
- VLog works without GC (append-only is fine for initial release)
- GC implementation will include proper coordination from the start
- No immediate risk since GC doesn't exist

---

### 1.7 Range Scan Iterator Invalidation ✅ FIXED

**Status**: Fixed (commit e78d6c0)

**Issue**: Range scan iterator could miss keys if flush happens during collection

**Location**: `src/db.rs::range()` (lines 2417-2494)

**Problem** (before fix):
```rust
// Thread 1: range() collects SSTables
let lsm_arc = self.lsm.load();
for level in lsm_arc { ... }  // Capture SSTables
drop(lsm_arc);  // LSM dropped!

// Thread 2: FLUSH HAPPENS HERE
// - Memtable → SSTable (new_file.sst)
// - Memtable cleared
// - LSM updated to include new_file.sst

// Thread 1: range() collects memtables
let memtables = ...;  // Now EMPTY!

// Result: MISSING KEYS in new_file.sst
```

**Solution** (commit e78d6c0):
```rust
// Collect memtables FIRST
let memtables = self.memtables.iter()...;  // Capture keys

// Flush can happen here:
// - Keys seen in memtables (captured above)
// - Keys also in new SSTable (will be captured below)

// THEN collect SSTables
let lsm_arc = self.lsm.load();
for level in lsm_arc { ... }  // Includes new SSTable if flush happened

// Result: Keys seen twice, but k-way merge deduplicates ✅
```

**Impact**: Prevents missing keys during concurrent flushes
- K-way merge already handles deduplication
- Memtable priority ensures correct values
- No performance impact (same operations, different order)

**Fixed By**: Reversing collection order (memtables → SSTables)

---

### 1.8 File Format Magic Numbers ✅ FIXED

**Status**: Fixed for all file formats (SSTable already had it, WAL/VLog added in commit 02c0c68)

**Issue**: Need magic numbers to detect corruption and version mismatches

**Current State**:
- ✅ SSTable: Has magic "SSTB" (0x53535442) + version in header and footer (already implemented)
- ✅ WAL: Has magic "WLOG" (0x574C4F47) + version in 8-byte header (commit 02c0c68)
- ✅ VLog: Has magic "VLOG" (0x564C4F47) + version in 8-byte header (commit 02c0c68)

**Previous Problem** (WAL/VLog before fix):
```rust
// WAL/VLog had no header:
// [record1][record2][record3]...
//
// Could not:
// - Detect file corruption (random file could be parsed as WAL)
// - Version detection
// - Format validation
```

**Solution** (commit 02c0c68):
```rust
// WAL header (8 bytes at file start):
const MAGIC: u32 = 0x574C4F47;  // "WLOG"
const VERSION: u32 = 0x00000001;
// Format: [magic: u32][version: u32][record1][record2]...

// VLog header (8 bytes at file start):
const MAGIC: u32 = 0x564C4F47;  // "VLOG"
const VERSION: u32 = 0x00000001;
// Format: [magic: u32][version: u32][record1][record2]...

// create() writes header:
file.write_all(&MAGIC.to_le_bytes())?;
file.write_all(&VERSION.to_le_bytes())?;

// open() validates header:
let mut header = [0u8; 8];
file.read_exact(&mut header)?;
let magic = u32::from_le_bytes([header[0..4]]);
let version = u32::from_le_bytes([header[4..8]]);
if magic != MAGIC || version != VERSION {
    return Err(InvalidFormat);
}
```

**Impact**: Format validation prevents reading garbage files
- Detects corrupted/wrong files immediately on open
- Enables future format upgrades (version checking)
- Minimal overhead (single 8-byte header per file)

**Testing**: All WAL and VLog tests updated and passing ✅

---

## 2. High Priority Issues ⚠️ (Important for 0.0.1)

### 2.1 No Fsync on SSTable Creation ⚠️

**Issue**: SSTable files not fsynced after creation

**Impact**: Data loss on crash (unflushed OS buffers)

**Fix**: Add fsync after SSTable write
```rust
pub fn finish(self) -> Result<()> {
    let mut file = File::create(&self.path)?;
    self.write_to(&mut file)?;
    file.sync_all()?;  // ← ADD THIS
    Ok(())
}
```

**Estimated Time**: 2 hours

---

### 2.2 Compaction Deletes SSTables While Readers Active ⚠️

**Issue**: Compaction can delete SSTable files while iterators are reading them

**Impact**: Read errors, crashes

**Fix**: Reference counting for SSTables (see 1.7)

**Estimated Time**: 2-3 days

---

### 2.3 Memtable Flush Threshold Check Race ⚠️

**Issue**: Multiple threads can trigger flush simultaneously

**Location**: `src/db.rs::put_internal()`

**Problem**:
```rust
pub fn put_internal(&self, key: Bytes, value: Bytes) -> Result<()> {
    // Thread 1: Check size
    if self.memtable_size.load() >= THRESHOLD {  // 64MB
        self.flush()?;  // ← Triggers flush
    }

    // Thread 2: Check size (simultaneously)
    if self.memtable_size.load() >= THRESHOLD {  // Still 64MB
        self.flush()?;  // ← Triggers SECOND flush!
    }
}
```

**Impact**: Multiple concurrent flushes, resource waste

**Fix**: Atomic compare-and-swap for flush trigger
```rust
pub fn put_internal(&self, key: Bytes, value: Bytes) -> Result<()> {
    let size = self.memtable_size.fetch_add(entry_size, Ordering::Relaxed);

    if size >= THRESHOLD {
        // Only ONE thread wins the flush race
        if self.flush_in_progress.compare_exchange(
            false, true,
            Ordering::SeqCst, Ordering::SeqCst
        ).is_ok() {
            self.flush()?;
            self.flush_in_progress.store(false, Ordering::SeqCst);
        }
    }
}
```

**Estimated Time**: 4 hours

---

### 2.4 No Disk Space Checks ⚠️

**Issue**: No checking if disk is full before writes

**Impact**: Partial writes, corruption

**Fix**: Check available space before large operations
```rust
use fs2::available_space;

fn check_disk_space(&self, required: u64) -> Result<()> {
    let available = available_space(&self.opts.data_dir)?;
    if available < required {
        return Err(DBError::DiskFull);
    }
    Ok(())
}

pub fn flush(&self) -> Result<()> {
    let estimated_size = self.memtable_size.load() * 2;  // With overhead
    self.check_disk_space(estimated_size)?;
    // ... flush ...
}
```

**Estimated Time**: 4 hours

---

### 2.5 No File Descriptor Limit Handling ⚠️

**Issue**: Can exceed OS file descriptor limit with many SSTables open

**Impact**: "Too many open files" errors, crashes

**Fix**: Limit concurrent open SSTables
```rust
const MAX_OPEN_SSTABLES: usize = 1000;

pub struct DB {
    open_sstable_limit: Arc<Semaphore>,
}

impl DB {
    pub fn open(opts: DBOptions) -> Result<Self> {
        Ok(Self {
            open_sstable_limit: Arc::new(Semaphore::new(MAX_OPEN_SSTABLES)),
        })
    }
}

impl SSTable {
    pub fn open(&self, path: &Path) -> Result<Self> {
        let _permit = self.open_limit.acquire()?;
        // ... open file ...
    }
}
```

**Estimated Time**: 1 day

---

### 2.6 Bloom Filter False Positive Rate Not Validated ⚠️

**Issue**: No runtime validation that bloom filter FPR matches expected 1%

**Impact**: Performance degradation if FPR higher than expected

**Fix**: Add FPR tracking and warnings
```rust
pub struct BloomFilterStats {
    total_queries: AtomicU64,
    false_positives: AtomicU64,
}

impl BloomFilter {
    pub fn contains(&self, key: &[u8]) -> bool {
        self.stats.total_queries.fetch_add(1, Ordering::Relaxed);

        let result = self.bits.contains(key);

        // Track FP rate (sampled)
        if result && self.stats.total_queries.load(Ordering::Relaxed) % 1000 == 0 {
            // Verify against actual data
            if !self.verify_actual_membership(key) {
                self.stats.false_positives.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    pub fn report_stats(&self) {
        let total = self.stats.total_queries.load(Ordering::Relaxed);
        let fp = self.stats.false_positives.load(Ordering::Relaxed);
        let fp_rate = fp as f64 / total as f64;

        if fp_rate > 0.02 {  // 2x expected
            warn!("Bloom filter FP rate {}% (expected 1%)", fp_rate * 100.0);
        }
    }
}
```

**Estimated Time**: 1 day

---

### 2.7 Missing Checksum Validation ⚠️

**Issue**: No checksums on SSTable blocks, bloom filters, or index

**Impact**: Silent data corruption undetected

**Fix**: Add CRC32 checksums
```rust
use crc32fast::Hasher;

// Block format:
// [data: N bytes][checksum: 4 bytes]

pub fn write_block(&mut self, data: &[u8]) -> Result<()> {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let checksum = hasher.finalize();

    self.writer.write_all(data)?;
    self.writer.write_u32::<LittleEndian>(checksum)?;
    Ok(())
}

pub fn read_block(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
    let mut data = vec![0u8; len];
    self.file.read_exact_at(&mut data, offset)?;

    let mut checksum_bytes = [0u8; 4];
    self.file.read_exact_at(&mut checksum_bytes, offset + len as u64)?;
    let expected = u32::from_le_bytes(checksum_bytes);

    let mut hasher = Hasher::new();
    hasher.update(&data);
    let actual = hasher.finalize();

    if actual != expected {
        return Err(DBError::ChecksumMismatch);
    }

    Ok(data)
}
```

**Estimated Time**: 2-3 days (all read/write paths affected)

---

### 2.8 ALEX Index Can Return Wrong Results After Many Inserts ⚠️

**Issue**: ALEX index expansion can fail, causing lower_bound to return incorrect position

**Location**: `src/alex/gapped_node.rs::insert()`

**Problem**: If expansion fails repeatedly, node becomes corrupted

**Test**: Partial (test_expansion_factors, but not comprehensive)

**Fix**: Add invariant checks and better error handling

**Estimated Time**: 1-2 days

---

### 2.9 No Transaction Isolation ⚠️

**Issue**: Reads can see partial writes from concurrent operations

**Impact**: Non-repeatable reads, phantom reads

**Fix**: Snapshot isolation (complex, may defer to 0.0.2)

**Estimated Time**: 1-2 weeks (large feature)

---

### 2.10 Background Thread Panic Handling ⚠️

**Issue**: If background threads (WAL writer, flush, compaction) panic, DB becomes unusable but doesn't propagate error

**Fix**: Catch panics and mark DB as poisoned
```rust
pub struct DB {
    poisoned: Arc<AtomicBool>,
}

// In WAL worker thread
std::panic::catch_unwind(|| {
    // WAL writer work
}).unwrap_or_else(|_| {
    db.poisoned.store(true, Ordering::SeqCst);
    error!("WAL writer thread panicked!");
});

pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
    if self.poisoned.load(Ordering::SeqCst) {
        return Err(DBError::Poisoned("Background thread panicked"));
    }
    // ...
}
```

**Estimated Time**: 1 day

---

### 2.11 Partition Count Not Configurable ⚠️

**Issue**: `NUM_PARTITIONS = 16` is hardcoded, may not suit all workloads

**Fix**: Make configurable via DBOptions

**Estimated Time**: 4 hours

---

### 2.12 No Metrics for Debugging ⚠️

**Issue**: Hard to debug performance issues without internal metrics

**Fix**: Add comprehensive metrics
```rust
pub struct InternalMetrics {
    // Memtable
    memtable_size: AtomicU64,
    memtable_flushes: AtomicU64,

    // WAL
    wal_writes: AtomicU64,
    wal_bytes: AtomicU64,
    wal_sync_time_us: AtomicU64,

    // Compaction
    compactions_started: AtomicU64,
    compactions_completed: AtomicU64,
    bytes_compacted: AtomicU64,

    // Cache
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,

    // Bloom filters
    bloom_queries: AtomicU64,
    bloom_false_positives: AtomicU64,
}
```

**Estimated Time**: 2-3 days

---

## 3. Medium Priority Issues ⚠️ (Can defer to 0.0.2)

### 3.1 No WAL Rotation

**Issue**: WAL grows unbounded until flush

**Fix**: Rotate WAL files periodically

**Estimated Time**: 2 days

---

### 3.2 No SSTable Size Limits

**Issue**: Flush can create huge SSTables if memtable is large

**Fix**: Split large SSTables

**Estimated Time**: 1 day

---

### 3.3 No Compaction Throttling

**Issue**: Compaction can use 100% CPU/IO

**Fix**: Add configurable throttling

**Estimated Time**: 2-3 days

---

### 3.4 No Graceful Shutdown

**Issue**: Dropping DB doesn't wait for background threads

**Fix**: Implement proper Drop with thread join

**Estimated Time**: 1 day

---

### 3.5 No Backup/Restore API

**Issue**: No way to backup/restore database

**Fix**: Add snapshot export

**Estimated Time**: 3-4 days

---

### 3.6 No Database Repair Tool

**Issue**: If DB corrupts, no way to recover

**Fix**: Add repair utility

**Estimated Time**: 1 week

---

### 3.7 No Performance Profiling Support

**Issue**: No built-in profiling hooks

**Fix**: Add `tracing` integration

**Estimated Time**: 1-2 days

---

### 3.8 No Key Size Limits

**Issue**: Can insert arbitrarily large keys (OOM)

**Fix**: Add MAX_KEY_SIZE limit

**Estimated Time**: 2 hours

---

### 3.9 No Value Size Limits (Non-VLog)

**Issue**: Can insert huge values in LSM tree

**Fix**: Enforce MAX_VALUE_SIZE or auto-promote to VLog

**Estimated Time**: 4 hours

---

### 3.10 No Directory Lock

**Issue**: Multiple DB instances can open same directory

**Fix**: Use file lock

**Estimated Time**: 4 hours

---

### 3.11 No Manifest File

**Issue**: LSM state not persisted across restarts

**Fix**: Add MANIFEST file (RocksDB-style)

**Estimated Time**: 1 week

---

### 3.12 No Statistic Histograms

**Issue**: Only average metrics, no p50/p99/p999

**Fix**: Use HdrHistogram

**Estimated Time**: 2-3 days

---

### 3.13 No Memory Budget Enforcement

**Issue**: Can exceed available RAM

**Fix**: Add memory limit checks

**Estimated Time**: 2-3 days

---

### 3.14 No IO Priority Control

**Issue**: Background compaction starves foreground reads

**Fix**: Use ionice on Linux

**Estimated Time**: 1 day

---

### 3.15 No Corruption Detection on Open

**Issue**: Don't verify DB integrity on open

**Fix**: Add fsck-style validation

**Estimated Time**: 3-4 days

---

## 4. Testing Gaps (Critical)

### Missing Test Categories

❌ **Crash recovery tests** (0/10)
- Crash during flush
- Crash during compaction
- Crash during batch commit
- Crash during WAL write

❌ **Concurrency tests** (0/15)
- Concurrent reads + writes
- Concurrent flushes
- Concurrent compactions
- Concurrent batch commits

❌ **Stress tests** (0/10)
- 10M+ operations
- 1000+ SSTables
- 100GB+ database
- 1000+ concurrent clients

❌ **Edge case tests** (3/50)
- Empty database
- Single key database
- Duplicate keys
- Large keys (>1MB)
- Large values (>100MB)
- Sequential vs random keys
- Hot key scenarios

❌ **Failure injection tests** (0/20)
- Disk full
- IO errors
- OOM
- Slow disk
- Network failures (for distributed later)

❌ **Correctness tests** (5/30)
- Linearizability
- Serializability
- Snapshot isolation
- Point-in-time consistency

### Test Coverage Estimate

**Current**: ~15% (basic happy path only)
**Required for 0.0.1**: 80%+

---

## 5. Action Plan for 0.0.1

### Phase 1: Critical Bugs (Week 1-2)

**Must fix before any release**:
1. ✅ Batch API atomicity
2. ✅ WAL recovery race condition
3. ✅ Compaction live key deletion
4. ✅ VLog GC corruption
5. ✅ Range scan invalidation
6. ✅ SSTable magic number
7. ✅ Memtable partition skew
8. ✅ Block cache safety

**Estimated**: 2 weeks, 1 engineer

---

### Phase 2: High Priority (Week 3-4)

**Important for production readiness**:
1. ✅ SSTable fsync
2. ✅ SSTable reference counting
3. ✅ Flush race condition
4. ✅ Disk space checks
5. ✅ File descriptor limits
6. ✅ Checksum validation
7. ✅ Background thread panic handling

**Estimated**: 2 weeks, 1 engineer

---

### Phase 3: Testing (Week 5-6)

**Achieve 80%+ coverage**:
1. ✅ Crash recovery tests
2. ✅ Concurrency tests
3. ✅ Edge case tests
4. ✅ Failure injection tests
5. ✅ Correctness tests

**Estimated**: 2 weeks, 1 engineer

---

### Phase 4: Documentation (Week 7)

**Complete before release**:
1. ✅ API documentation
2. ✅ Architecture guide
3. ✅ Performance tuning guide
4. ✅ Failure mode documentation
5. ✅ Migration guide

**Estimated**: 1 week, 1 engineer

---

## 6. Timeline to 0.0.1

**Total Estimated Time**: 7-8 weeks (full-time)

**Breakdown**:
- Critical bugs: 2 weeks
- High priority: 2 weeks
- Testing: 2 weeks
- Documentation: 1 week
- Buffer: 1-2 weeks

**Realistic Date**: ~Late December 2025 / Early January 2026

---

## 7. Recommendations

### DO NOT SHIP 0.0.1 UNTIL:

✅ All 8 critical issues fixed
✅ At least 7/12 high priority issues fixed
✅ Test coverage >80%
✅ All tests passing
✅ Fuzz testing completed
✅ Sanitizer runs clean (ASAN, MSAN, TSAN)
✅ Crash recovery validated
✅ Documentation complete

### CAN DEFER TO 0.0.2:

📅 Medium priority issues (15 items)
📅 Advanced features (snapshots, backups)
📅 Performance optimizations (rkyv, advanced caching)
📅 Distributed features

### CURRENT STATUS:

**0.0.0 (Current)**: ❌ Not safe for ANY use
**0.0.1 (Target)**: ✅ Safe for local testing, experimental use
**0.1.0 (Future)**: Production-ready for single-node
**1.0.0 (Future)**: Production-ready, stable API

---

**Updated**: November 8, 2025
**Next Review**: After critical bugs fixed
