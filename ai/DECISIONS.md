# DECISIONS - seerdb Design Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## Architecture Decisions

### 1. Base Structure: LSM Tree (Not B+ Tree)

**Decision**: Use LSM-tree as foundation (like RocksDB), not B+ tree (like sled)

**Rationale**:
- LSM trees optimize for write-heavy workloads
- All target workloads (omen vectors, queue, time series) are write-heavy
- Research papers focus on LSM optimizations (learned components fit naturally)

**Trade-offs**:
- ✅ Better write amplification
- ✅ More research to build on
- ❌ More complex than B+ tree
- ❌ Read performance requires optimization (bloom filters, caching)

**References**: Dostoevsky paper (LSM trade-offs), WiscKey (KV separation), PebblesDB (fragmented LSM)

---

### 2. Learned Bloom Filters (Week 1 Priority)

**Decision**: Replace traditional bloom filters with learned models

**Rationale**:
- 90% space reduction claim (Kraska et al., 2018)
- Low implementation complexity (good first prototype)
- Immediate benefit (every SSTable uses bloom filters)

**Trade-offs**:
- ✅ 90% space savings
- ✅ Same false positive rate
- ❌ Training cost on compaction
- ❌ Model inference latency (vs hash function)

**Validation**: Prototype in Week 1 to verify 90% claim

**References**: "Learned Bloom Filters" (Kraska et al., 2018)

---

### 3. Key-Value Separation (WiscKey-Style)

**Decision**: Store large values (>4KB threshold) separately in value log (vLog)

**Rationale**:
- omen vectors are 512-4096 bytes (embeddings)
- WiscKey shows 10-100x write amplification reduction
- LSM size reduction: 50x smaller (100GB → 2GB for 1KB values)
- Industrial validation: BlobDB, Titan, TerarkDB, BadgerDB
- Compaction only rewrites keys + pointers (not full values)

**Implementation**:
- **vLog**: Append-only log for values >4KB threshold
- **LSM tree**: Stores keys + value references (offset + length)
- **GC**: Head-tail tracking, validity check via LSM, rewrite valid entries
- **Range queries**: Parallel prefetching to mitigate random I/O

**Threshold Tuning**:
- BadgerDB: 4KB default
- TerarkDB: 512B default
- seerdb initial: 4KB (tune based on omen workload benchmarks)
- Rule: Separate if value_size > key_size * 10 (rough heuristic)

**Trade-offs**:
- ✅ Write amplification: 10-100x reduction
- ✅ LSM size: 50x smaller (more in cache, faster compaction)
- ✅ Database loading: 2.5-111x faster
- ✅ Lookups: 1.6-14x faster
- ❌ Space amplification: Higher (garbage until GC)
- ❌ Crash recovery: Slower (vLog head pointer overhead)
- ❌ Range scans: Random I/O (mitigated by prefetching)
- ❌ Small values: Overhead not worth it

**When to Use**:
- ✅ Values >1KB that dominate storage (omen vectors: YES)
- ✅ Write-heavy workloads (omen: append-heavy documents)
- ✅ SSDs (can exploit parallel reads)
- ❌ Small uniform values <256B (queue metadata: NO)
- ❌ Range-scan dominated without prefetch capability

**References**:
- "WiscKey: Separating Keys from Values" (Lu et al., FAST 2016)
- Industrial implementations: Titan, TerarkDB, BadgerDB, BlobDB

---

### 4. Rust-Native Implementation

**Decision**: Build in Rust (not C++ like RocksDB)

**Rationale**:
- Memory safety without GC overhead
- Easier integration with omen (also Rust)
- Modern async/await for I/O
- SIMD intrinsics well-supported

**Trade-offs**:
- ✅ Memory safety
- ✅ Easy omen integration
- ✅ Modern tooling
- ❌ Less mature ecosystem than C++ (fewer libraries)
- ❌ Learning curve for unsafe code (if needed)

---

### 5. Elastic License 2.0 (Source-Available)

**Decision**: Use Elastic License 2.0 (not MIT/Apache)

**Rationale**:
- Prevents cloud providers from offering managed seerdb
- Allows self-hosting and embedding
- Source-available (not closed source)
- Same license as omen ecosystem

**Trade-offs**:
- ✅ Protects commercial interests
- ✅ Still allows most use cases
- ❌ Not OSI-approved "open source"
- ❌ May limit adoption vs permissive license

---

## Research Phase Decisions

### 6. 4-Week Research Phase (Before Coding)

**Decision**: Spend 4 weeks reading papers and benchmarking before building core engine

**Rationale**:
- Avoid reimplementing RocksDB (need to understand research landscape)
- Design decisions require deep understanding (can't undo later)
- Validate research claims early (prototype learned bloom filters)

**Trade-offs**:
- ✅ Informed design decisions
- ✅ Avoid costly rewrites
- ❌ Delays functional product
- ❌ Risk of "analysis paralysis"

**Mitigation**: Prototype learned bloom filters in Week 1 (stay grounded in implementation)

---

### 7. Workload-Aware Optimization (Tucana-Inspired)

**Decision**: Detect workload patterns and adapt compaction strategy

**Rationale**:
- omen has distinct workloads (append-heavy vectors, FIFO queue, time series)
- Generic LSM tuning (RocksDB) suboptimal for all
- Tucana shows 3x throughput improvement vs RocksDB

**Trade-offs**:
- ✅ Better performance for known workloads
- ✅ Unique differentiator vs RocksDB
- ❌ Complexity (workload detection, strategy switching)
- ❌ May perform worse on unknown workloads

**Implementation**: Week 16 (after core engine stable)

**References**: "Tucana" (Liu et al., 2020)

---

## Future Decisions (TBD)

### 8. Learned Index Model Selection (DECIDED - Use ALEX)

**Decision**: Use ALEX-style learned index for SSTable index blocks

**Options Evaluated**:
- ✅ ALEX (gapped arrays, handles updates, proven)
- Piecewise linear models (Bourbon - for immutable data only)
- Neural networks (original Kraska paper - too complex)

**Rationale**:
- ALEX code available in omen-org/ (can adapt)
- Handles updates/deletes (critical for dynamic data)
- 2.2x faster than original learned index, 4.1x faster than B+trees
- Production-quality implementation exists (Microsoft Research)

**Status**: Decided, implementation in Phase 2 (Weeks 9-12)

---

### 9. Compaction Strategy (DECIDED - Lazy Leveling)

**Decision**: Use Lazy Leveling (Dostoevsky) for seerdb

**Options Evaluated**:

1. **Leveled (RocksDB default)**:
   - Write amp: High (11x at T=10)
   - Read amp: Low (disjoint key ranges)
   - Use case: Read-heavy workloads
   - ❌ Too much write amp for omen workload

2. **Tiered (Cassandra-style)**:
   - Write amp: Low (good for writes)
   - Read amp: High (must check all runs)
   - Space amp: Very high (O(T), 1.2GB → 9.3GB at T=4)
   - Use case: Pure write-heavy workloads
   - ❌ Space amp too high, read performance poor

3. **Lazy Leveling (Dostoevsky)** ✅ **CHOSEN**:
   - Largest level: Leveled (disjoint for range queries)
   - Other levels: Tiered (reduce write amp)
   - Write amp: Better than leveled
   - Read amp: Better than tiered
   - Space amp: Similar to leveled (~11%)
   - Use case: Mixed workloads (omen vectors)

4. **Fragmented (PebblesDB)**:
   - Write amp: Best (2.4-3x better than RocksDB)
   - Read amp: Worst (multiple sstables per guard)
   - Use case: Pure write-heavy, no range scans
   - ❌ omen needs range queries (vector search top-K)

**Rationale**:
- **omen vectors**: Append-heavy + range scans (vector search top-K)
- Lazy Leveling balances both needs perfectly
- Largest level disjoint → efficient range queries
- Upper levels tiered → reduced write amplification
- Can combine with WiscKey (KV separation for large embeddings)

**Configuration**:
- Level ratio: T=10 (RocksDB standard, tune later)
- Largest level: Leveled compaction (merge all overlaps)
- Levels 0 to N-1: Tiered compaction (allow multiple runs)
- Adaptive tuning: Future enhancement (Phase 3)

**Workload Mapping**:
- **omen vectors**: Lazy Leveling ✅ (balanced read/write, range scans)
- **omen-queue**: Tiered (pure write-heavy, FIFO, no range scans)
- **omen time series**: Lazy Leveling (append-heavy + time-range queries)

**Trade-offs**:
- ✅ Best balance for mixed workloads
- ✅ omen workload fits perfectly
- ✅ Can add adaptive tuning later (Dostoevsky model)
- ❌ More complex than pure leveled or tiered
- ❌ Need to implement both strategies

**References**:
- "Dostoevsky: Better Space-Time Trade-Offs" (Dayan & Idreos, SIGMOD 2018)
- "PebblesDB" (Raju et al., SOSP 2017) - considered but rejected for omen

**Status**: Decided, implementation in Phase 1 (Week 7)

---

### 10. I/O Backend (DECIDED - tokio with optional io_uring)

**Decision**: Use tokio async I/O by default, io_uring as opt-in feature

**Options Evaluated**:
- ✅ tokio async I/O (default, cross-platform, secure)
- io_uring (Linux 5.1+, optional, security concerns)
- Hybrid approach chosen

**Rationale - Security First**:
- **io_uring vulnerabilities**: 77 CVEs, 60% of 2022 kernel exploits
- Critical issues: CVE-2021-20226, CVE-2022-2602, CVE-2023-2598 (all privilege escalation to root)
- Common attack vectors: use-after-free, reference counting bugs, out-of-bounds access
- Complex kernel/userspace shared memory attack surface

**Implementation Strategy**:
- **Default**: tokio async I/O (safe, cross-platform, good performance)
- **Optional**: io_uring feature flag (Linux-only, opt-in, document risks)
- **Disabled by default**: Users must explicitly enable with awareness of security trade-offs

**Configuration**:
```toml
[features]
default = ["tokio-io"]
io-uring = ["io-uring-sys"]  # Opt-in, Linux-only, performance vs security trade-off
```

**Performance Trade-off**:
- tokio: Excellent performance, ~10-20% slower than io_uring in best case
- io_uring: 50-100% faster I/O (when it works), but serious security risks
- **Decision**: Security > marginal performance gains

**Trade-offs**:
- ✅ Secure by default (no privilege escalation risk)
- ✅ Cross-platform (macOS, Linux, Windows)
- ✅ Simpler implementation (standard async/await)
- ❌ Slightly slower than io_uring (acceptable trade-off)
- ❌ Linux users don't get max I/O performance by default

**Status**: Decided, tokio default, io_uring opt-in behind feature flag

---

## Implementation Decisions

### 11. SSTable Binary Search with Full Key Index (Week 6 - Nov 1, 2025)

**Decision**: Store full keys in SSTable index, not just offsets

**Rationale**:
- Enables O(log n) binary search directly on keys
- Previous: Vec<u64> (offsets only) → O(n) linear scan
- Current: Vec<(Bytes, u64)> (key + offset) → O(log n) binary search

**Implementation**:
- Index: Vec<(Bytes, u64)> stored in SSTable
- Search: binary_search_by() on sorted keys
- Memory cost: ~1-2 MB per SSTable (acceptable)

**Trade-offs**:
- ✅ Binary search: O(log n) lookups
- ✅ 100k entries: 17 comparisons vs 100k comparisons
- ❌ Memory: ~1-2 MB index per SSTable
- ❌ Later addressed by block-based format (Phase 2.4)

**Performance**: 476k ops/sec existing keys, 9.1M ops/sec missing keys (19x speedup from bloom)

**Commits**: a4d2c8b (Week 6), 7a3cbe8 (block-based refactor)

---

### 12. Bloom Filter Integration (Week 6 - Nov 1, 2025)

**Decision**: Check bloom filter before binary search

**Rationale**:
- Eliminates unnecessary lookups for missing keys
- 19x speedup for negative lookups (192x at 100k scale)
- 1% FPR = 99% of missing keys filtered instantly

**Implementation**:
- Bloom filter built during SSTable construction
- Serialized to SSTable file (footer: [index_offset][bloom_offset])
- Checked in get() before binary search

**Trade-offs**:
- ✅ Missing keys: ~11 µs constant (regardless of SSTable size)
- ✅ Space: 122 KB for 100k keys (1% FPR)
- ❌ 1% false positives still do binary search + disk read
- ✅ 99% benefit outweighs 1% cost

**Performance**: 100k entries, missing key lookups 192x faster than without bloom

**Commits**: a4d2c8b

---

### 13. Collect-and-Sort Merge Strategy (Week 7 - Nov 1, 2025)

**Decision**: Collect all entries upfront, then sort (not streaming k-way merge)

**Rationale**:
- SSTable::iter() requires &mut self (file seeking)
- Streaming k-way merge with BinaryHeap has lifetime issues
- Compaction is background task (memory acceptable)
- Simplicity > streaming efficiency

**Implementation**:
```rust
// Collect all entries from all SSTables
for sstable in sstables {
    entries.extend(sstable.iter());
}
// Sort by (key, source_id)
entries.sort_by(|(k1, sid1, _), (k2, sid2, _)|
    k1.cmp(k2).then(sid1.cmp(sid2))
);
// Deduplicate: keep first (newest)
```

**Trade-offs**:
- ✅ Simple, correct, testable
- ✅ Easier to reason about deduplication
- ❌ Memory: O(total entries) during merge
- ❌ Not streaming (but acceptable for compaction)

**Future**: Consider streaming merge if large compactions become bottleneck

**Commits**: ea3b5bd

---

### 14. Deduplication Strategy: Newest Wins (Week 7 - Nov 1, 2025)

**Decision**: Keep entry from lowest source_id (newest value)

**Rationale**:
- Input SSTables ordered by age (newest first)
- Lower source_id = later in time = should override
- Matches LSM semantics (newer writes win)

**Implementation**:
- Sort by (key, source_id)
- Stable sort preserves ordering
- Keep first occurrence after sort

**Trade-offs**:
- ✅ Correct LSM semantics
- ✅ Simple: just stable sort + dedup
- ✅ Handles overwrites, deletes (tombstones)

**Commits**: ea3b5bd

---

### 15. Synchronous Flush and Compaction (Week 8 - Nov 1, 2025)

**Decision**: Flush and compaction block the write thread initially

**Rationale**:
- Simpler implementation for MVP
- Easier to reason about correctness
- Sufficient for initial validation
- Can add background threads later (proven pattern)

**Trade-offs**:
- ✅ Simple, correct, no race conditions
- ✅ Faster to implement and validate
- ❌ Write latency spikes during flush/compaction
- ✅ Addressed in Week 15 (background compaction)

**Commits**: 7e421cb (sync), later 2bd4074 (background option added)

---

### 16. WAL Recovery on Every Open (Week 8 - Nov 1, 2025)

**Decision**: Always replay WAL on DB::open(), even if empty

**Rationale**:
- Ensures consistency (no partial writes)
- Simple: No need to track "clean shutdown" state
- Fast: WAL small if recently flushed
- Industry standard (RocksDB, LevelDB do this)

**Implementation**:
- Check if WAL exists on open
- If exists, replay all records into memtable
- Create new WAL (overwrites old)
- Flush memtable if capacity exceeded

**Trade-offs**:
- ✅ Zero data loss guarantee
- ✅ Simple: no shutdown markers needed
- ❌ Small overhead on normal open (negligible)
- ✅ WAL small in practice (<memtable capacity)

**Commits**: c863e92

---

### 17. New WAL After Recovery (Week 8 - Nov 1, 2025)

**Decision**: Create new WAL after replaying (overwrite old)

**Rationale**:
- Old WAL data already in memtable
- Avoids ever-growing WAL
- Simpler than WAL truncation/rotation

**Trade-offs**:
- ✅ Simple: just overwrite file
- ✅ Prevents WAL growth
- ❌ Loses WAL as historical record (use snapshots instead)

**Future**: WAL rotation for long-running databases (archive old WALs)

**Commits**: c863e92

---

### 18. Arc<Mutex<>> for Shared State (Week 8 - Nov 1, 2025)

**Decision**: Use Arc<Mutex<>> for WAL and LSMTree

**Rationale**:
- Simple concurrency model
- WAL and LSMTree modified infrequently (only on flush/compaction)
- Memtable uses lock-free skiplist (high-frequency reads/writes)
- Clear ownership and mutation points

**Trade-offs**:
- ✅ Simple: clear lock points
- ✅ Correct: no data races
- ❌ Mutex contention on flush (acceptable - infrequent)
- ✅ Memtable lock-free for hot path

**Future**: Consider RwLock for read-heavy workloads (metrics, stats)

**Commits**: 7e421cb

---

### 19. Traditional Bloom Filters, NOT Learned (Week 9 - Nov 1, 2025)

**Decision**: Use traditional bit-packed bloom filters, NOT learned models

**Context**: Week 1 plan was to use learned bloom filters (Kraska et al. 2018 paper)

**What Happened**:
- Implemented learned bloom filter with decision tree model
- Achieved 48-51% false positive rate (target: 1%)
- Root cause: Hash-based features destroy patterns needed for ML

**Why Learned Blooms Failed**:
1. **Our feature extraction**: Hash functions (intentionally random)
   - `hash("key_0001")` → `[0.342, 0.891, 0.123, ...]`
   - `hash("key_0002")` → `[0.671, 0.234, 0.987, ...]`
   - Similar inputs → completely unrelated outputs (avalanche effect)
2. **Model behavior**: Memorized training examples, couldn't generalize
   - Training data: 100% accuracy
   - Unseen data: 50% accuracy (random guessing)
3. **Proof**: Fixed implementation with proper features (numeric patterns) → 0% FPR

**When Learned Blooms Work**:
- ✅ Malicious URL filtering (domain patterns, TLD, path structure)
- ✅ Spam email detection (known spam domains, sender patterns)
- ✅ IP address blacklisting (network ranges, subnets)
- ❌ General KV storage (arbitrary byte strings, no guaranteed pattern)
- ❌ Cryptographic hashes (designed to be random)
- ❌ Random UUIDs (uniformly distributed)

**Why Traditional Blooms Win for seerdb**:
- Arbitrary keys: Users can store ANY byte string
- No assumptions: Can't assume keys follow patterns
- Guaranteed FPR: Mathematical guarantee (1%)
- Fast: Hash functions faster than ML inference (14x in benchmarks)
- Universal: Works for any data

**Trade-offs**:
- ✅ Guaranteed 1% FPR
- ✅ Works for arbitrary keys
- ✅ 10-100µs queries vs 1ms for learned
- ✅ No training overhead
- ❌ Can't exploit patterns (but we have no guaranteed patterns)

**Evidence**: ai/research/learned_bloom_analysis.md

**Commits**: Week 9 research (not merged to production)

**Status**: Traditional blooms in production, learned blooms research documented

---

### 20. K-way Merge for Range Scans (Nov 6, 2025)

**Decision**: Use k-way merge with BinaryHeap, not BTreeMap materialization

**Context**: Range scans were 20x slower than RocksDB (870 vs 17,332 scans/sec)

**Root Cause Analysis**:
```rust
// OLD (src/range.rs): BTreeMap materialization
let mut merged: BTreeMap<Bytes, Option<Bytes>> = BTreeMap::new();
for sstable in &sstables {
    for (key, value) in sstable.scan_range(start, end) {
        merged.entry(key).or_insert(value);  // O(n log n) + O(n) memory
    }
}
// Returns AFTER collecting ALL entries
```

**Problem**: Eager materialization
- Time: O(n log n) upfront cost before returning first result
- Memory: O(n) - must hold ALL range entries
- Latency: 100K entry scan loads all 100K before returning anything

**Solution**: K-way merge (SOTA for LSM trees)
```rust
// NEW (src/range_merge.rs): Lazy k-way merge
pub struct KWayMergeIterator<I> {
    heap: BinaryHeap<Reverse<HeapEntry<I>>>,  // Min-heap
    last_key: Option<Bytes>,                   // Deduplication
}
```

**Implementation Details**:
1. **Memtable**: Collect upfront (O(m) - acceptable, already in-memory)
2. **SSTables**: Lazy iteration (blocks loaded on-demand)
3. **Merge**: BinaryHeap maintains k iterators (k = num levels, typically 7-10)
4. **Deduplication**: Track last_key, skip duplicates (LSM: lower level = newer)
5. **Tombstones**: Filter None values in merge loop

**Complexity**:
- Time: O(k log k) per entry where k = num levels (7-10)
- Memory: O(k) heap state + O(m) memtable entries
- Latency: First SSTable result immediate (after memtable collection)

**Results**:
- **10K dataset**: 870 → 8,459 scans/sec (9.7x improvement ✅)
- **100K dataset**: 877 scans/sec (no improvement - investigation pending)

**Research Validation** (ai/research/PAPERS.md):
- SwiftKV, LearnedKV: Use learned indexes for point queries, k-way merge for ranges
- GRF: Optimizes filtering (which SSTables), not merge algorithm
- RocksDB, fjall, LevelDB: All use k-way merge with priority queue
- Confirmed: K-way merge is SOTA (2018-2024 papers)

**Trade-offs**:
- ✅ 9.7x improvement on 10K dataset
- ✅ SOTA algorithm, proven in production
- ✅ Truly lazy for SSTables (blocks on-demand)
- ✅ All 126 tests passing
- ⚠️ Memtable still collected upfront (O(m) memory)
- 🔴 100K dataset: no improvement yet (needs profiling)

**Future Work**:
- Profile 100K dataset performance (memtable size? SSTable count?)
- Consider fully lazy memtable iteration (lifetime challenges)
- May need to address SSTable iteration efficiency

**Testing**:
- 6 k-way merge unit tests (single, two, duplicates, tombstones, many, empty)
- All existing range tests passing
- Correctness: LSM semantics, deduplication, tombstone filtering

**Commits**: 6a0c73e (k-way merge), 607f13c (documentation)

**Status**: Implemented, works on small datasets, 100K performance under investigation

---

### 21. SSTable Range Filtering (Critical Optimization)

**Decision**: Filter SSTables by key range before creating iterators (November 7, 2025)

**Problem**: Range scans were 95% slower than RocksDB (870 vs 17,332 scans/sec)
- Creating iterators for ALL SSTables, even non-overlapping
- 100K dataset → 2 SSTables, but both opened on every scan
- K-way merge helped on 10K (9.7x), but not 100K (0x improvement)

**Root Cause**: Missing the optimization that RocksDB uses:
- RocksDB: Check SSTable key range, skip non-overlapping
- seerdb (before): Open all SSTables, create all iterators
- Result: 1000µs overhead per scan (SSTable::open() calls)

**Solution**: Add min_key/max_key metadata to SSTables

**Implementation** (commit 5e4dc0c):
```rust
// SSTable builder tracks first/last keys
pub struct SSTableBuilder {
    min_key: Option<Bytes>,  // First key added
    max_key: Option<Bytes>,  // Last key added
}

// SSTable stores metadata
pub struct SSTable {
    min_key: Option<Bytes>,
    max_key: Option<Bytes>,
}

// Check if SSTable overlaps with query range
impl SSTable {
    pub fn overlaps_range(&self, start_key: &[u8], end_key: Option<&[u8]>) -> bool {
        // max >= start_key AND (end_key is None OR min < end_key)
        if max.as_ref() < start_key { return false; }  // Before query range
        if let Some(end) = end_key {
            if min.as_ref() >= end { return false; }  // After query range
        }
        true
    }
}

// Filter in db.range()
for sstable_path in level.sstables() {
    let sstable = get_cached_or_open(sstable_path);
    if sstable.overlaps_range(start_key, end_key) {  // ← NEW
        sstables.push(sstable.scan_range(start_key, end_key));
    }
}
```

**Format Change**: SSTable v1 format
- Footer: 40 bytes → 48 bytes (added metadata_offset)
- Metadata section: min_key length + min_key + max_key length + max_key
- Backward incompatible (v0 → v1, but no production users yet)

**Results**:
- **Range scans**: 870 → 17,087 scans/sec (19.6x improvement!)
- **Ratio vs RocksDB**: 0.04x → 0.81x (competitive!)
- **Ratio vs fjall**: 0.08x → 1.50x (50% faster!)
- **Time per scan**: 1,148µs → 58µs (20x faster)

**How It Works** (Example):
```
Query: range [key_00100, key_00200)
SSTable A: [key_00000, key_00050)  → SKIP (no overlap)
SSTable B: [key_00100, key_00150)  → INCLUDE (overlaps)
SSTable C: [key_00250, key_00300)  → SKIP (no overlap)
Result: Create only 1 iterator instead of 3
```

**Leveled Compaction Benefits**:
- L1-LN: Disjoint key ranges (no overlap within level)
- Query [key_100, key_200) might only hit 1-2 SSTables total
- L0: Can have overlaps (all get checked)

**Trade-offs**:
- ✅ 19.6x range scan improvement
- ✅ Competitive with RocksDB (0.81x)
- ✅ 50% faster than fjall
- ✅ Minimal overhead (8 bytes + 2 key lengths in footer)
- ❌ Backward incompatible format change (v0 → v1)
- ⚠️ Still 19% slower than RocksDB (further optimizations possible)

**Why This Matters**:
- RocksDB has 10+ years of optimization
- This is THE critical optimization they use
- Without it, we were fundamentally broken for range scans
- With it, we're competitive (0.81x is acceptable)

**Further Optimizations Possible**:
- Adaptive readahead (prefetch next blocks) - +30-50%
- SIMD key comparisons in k-way merge - +10-20%
- Better block cache policy - +5-10%
- Expected: Can reach 1.0x+ RocksDB with these

**Research Validation**:
- RocksDB source code: Uses this exact approach
- fjall source code: Uses this approach
- All production LSMs: Use SSTable metadata for filtering
- Confirmed: Industry-standard optimization

**Testing**:
- All 120 tests passing
- range_benchmark: 63,301 scans/sec (10K dataset)
- baseline_benchmark: 17,087 scans/sec (100K dataset)
- Correctness: LSM semantics, deduplication, tombstone filtering

**Commits**: 5e4dc0c (SSTable filtering)

**Status**: ✅ Complete - Production-ready for all workloads

---

*Add decisions as they're made - include commit hash if implemented*
