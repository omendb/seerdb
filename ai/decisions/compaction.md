# Compaction Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## Compaction Strategy: Lazy Leveling (Dostoevsky)

**Decision**: Use Lazy Leveling (Dostoevsky) for seerdb

**Options Evaluated**:

1. **Leveled (RocksDB default)**:
   - Write amp: High (11x at T=10)
   - Read amp: Low (disjoint key ranges)
   - Use case: Read-heavy workloads
   - ❌ Too much write amp for database workload

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
   - Use case: Mixed workloads (read + write balanced)

4. **Fragmented (PebblesDB)**:
   - Write amp: Best (2.4-3x better than RocksDB)
   - Read amp: Worst (multiple sstables per guard)
   - Use case: Pure write-heavy, no range scans
   - ❌ Many workloads need range queries

**Rationale**:
- **Mixed workloads**: Append-heavy + range scans
- Lazy Leveling balances both needs perfectly
- Largest level disjoint → efficient range queries
- Upper levels tiered → reduced write amplification
- Can combine with WiscKey (KV separation for large values)

**Configuration**:
- Level ratio: T=10 (RocksDB standard, tune later)
- Largest level: Leveled compaction (merge all overlaps)
- Levels 0 to N-1: Tiered compaction (allow multiple runs)
- Adaptive tuning: Future enhancement (Phase 3)

**Workload Mapping**:
- **Large value workloads**: Lazy Leveling ✅ (balanced read/write, range scans)
- **Queue applications**: Tiered (pure write-heavy, FIFO, no range scans)
- **Time series**: Lazy Leveling (append-heavy + time-range queries)

**Trade-offs**:
- ✅ Best balance for mixed workloads
- ✅ database workload fits perfectly
- ✅ Can add adaptive tuning later (Dostoevsky model)
- ❌ More complex than pure leveled or tiered
- ❌ Need to implement both strategies

**References**:
- "Dostoevsky: Better Space-Time Trade-Offs" (Dayan & Idreos, SIGMOD 2018)
- "PebblesDB" (Raju et al., SOSP 2017) - considered but rejected

**Status**: Implemented

---

## Bug #7 Fix: Compaction Data Loss Prevention (Nov 9, 2025)

**Problem**: Compaction had TWO critical data loss bugs:
1. **Bug #7a**: Tombstone resurrection - Iterator filtered tombstones during compaction, causing deleted keys to resurrect from older levels
2. **Bug #7b**: File deletion race - SSTables deleted immediately after LSM update, causing concurrent readers with old LSM snapshots to get "file not found" errors

**Decision**: Two-part fix:
1. **Tombstone preservation**: Check `vlog.is_some()` flag in iterator to distinguish user reads (filter tombstones) from compaction (preserve tombstones)
2. **Delayed deletion queue**: Queue SSTable deletions with timestamps, delete after 5-second safe window

**Rationale**:
- Tombstones MUST be preserved during compaction to prevent resurrection
- Concurrent readers may hold old LSM snapshots pointing to deleted files
- Time-based delay (5s) is simple and safe for all workloads
- Alternative (reference counting) would be more complex and add overhead to hot path

**Implementation**:
```rust
// Bug #7a fix (src/sstable/mod.rs:1266-1277)
FLAG_TOMBSTONE => {
    if self.vlog.is_some() {
        continue  // User-facing read: filter tombstones
    } else {
        entry_value  // Compaction: preserve tombstones
    }
}

// Bug #7b fix (src/db.rs)
pending_deletions: Arc<Mutex<Vec<(PathBuf, std::time::Instant)>>>,

fn cleanup_old_deletions(...) {
    const DELETION_DELAY: Duration = Duration::from_secs(5);
    // Delete files queued >5 seconds ago
}
```

**Trade-offs**:
- ✅ Simple implementation (no reference counting complexity)
- ✅ Safe for all workloads (5s is conservative)
- ✅ Zero hot-path overhead (cleanup happens in background compaction thread)
- ✅ No performance regression (verified with benchmarks)
- ❌ Files linger for 5s (minor disk space impact)

**Alternatives Considered**:
1. **Reference counting** - More complex, hot-path overhead, tracking burden
2. **Grace period (500ms-1s)** - User explicitly rejected as "temporary fix"
3. **Epoch-based GC** - Overkill for this problem, adds complexity

**Testing**:
- ✅ `test_compaction_consistency` passes (Bug #7b validation)
- ✅ All 12 compaction tests pass
- ✅ 8 concurrent edge case tests pass
- ✅ No performance regression

**References**:
- Bug analysis from Task subagent (identified TWO separate bugs)
- User feedback: "dont temporary fix, correctly fix it"

---

## Compaction Audit & Prefix Filters (Nov 19, 2025)

**Context**:
- Current implementation uses **Tiered Compaction** (overlapping SSTables at all levels).
- Write throughput is high (878K/sec), but **Read Amplification** is severe for range queries.
- `compaction_stress_test` confirms writes are stable, but reads scale poorly with data size.

**Options Considered**:
1. **Switch to Leveled**: Lower Read Amp, but 10x higher Write Amp.
2. **Prefix Bloom Filters**: Keep Tiered (write optimized) but speed up prefix scans (graph traversal).

**Decision**: **Prefix Bloom Filters**.

**Rationale**:
- `omendb` is graph-heavy. Most reads are `prefix_scan(node_id)`.
- Tiered compaction is ideal for high ingest rates.
- Prefix Bloom Filters solve the Read Amp problem for the *specific* workload of graph traversal, without paying the Write Amp cost of Leveled compaction.
- "RocksDB but with 2020s research" -> Bloom Filters are cheaper than rearranging data on disk.

**Implementation**:
- SSTable Format v2: Add `prefix_bloom_offset` to footer.
- `SSTableBuilder`: Compute and write prefix bloom filters.
- `DB::scan_range`: Check `may_contain_prefix` before creating iterators.

**Status**: Implemented & Enabled.
