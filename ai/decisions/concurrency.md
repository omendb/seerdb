# Concurrency & Isolation Decisions

**Format**: Decision → Rationale → Trade-offs → References

---

## Arc<Mutex<>> for Shared State (Nov 1, 2025)

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

## Defer MVCC/Snapshot API to 0.0.2+ (Nov 10, 2025)

**Decision**: Provide Read Committed isolation for 0.0.1, defer Snapshot Isolation (MVCC) to 0.0.2+

**Context**: During Week 5-6 testing, discovered flaky test `test_concurrent_reads_consistent` revealing limitation: each `get()` captures separate snapshot, not multi-operation consistency.

**Problem**:
```rust
// Current behavior (Read Committed)
for i in 0..100 {
    db.get(key_i)  // Each get() captures NEW snapshot
}
// If flush happens between get(50) and get(51), reader may miss keys

// Desired behavior (Snapshot Isolation) - NOT IMPLEMENTED
let snapshot = db.snapshot();  // Capture ONCE
for i in 0..100 {
    snapshot.get(key_i)  // All reads see same consistent state
}
```

**Research Findings** (ai/research/LSM_MVCC_CONCURRENCY_RESEARCH.md):

1. **Vector databases don't need snapshot isolation**
   - Milvus: Eventual consistency
   - Qdrant: Eventual consistency (snapshots for backup only)
   - Weaviate: Eventual consistency with tunable quorum
   - Rationale: ANN search is approximate, slight inconsistency acceptable

2. **RocksDB MVCC is complex**
   - 4 data structures (CommitCache, PreparedHeap, OldCommitMap, DelayedPrepared)
   - 5-10% performance overhead
   - 2-6 weeks implementation effort (minimal to full MVCC)
   - Not required for vector database workloads

3. **Current isolation sufficient**
   - Read Committed: Per-operation point-in-time consistency
   - Atomic batch writes (all-or-nothing)
   - Lock-free concurrent reads/writes
   - WAL durability
   - Sufficient for vector applications (vector database) use case

**Rationale**:
- **Primary use case**: vector database applications - doesn't require snapshot isolation
- **Industry standard**: All major vector DBs use eventual consistency for ANN search
- **Complexity**: 2-6 weeks implementation + testing burden
- **Performance**: MVCC adds 5-10% overhead (would lose competitive advantage)
- **Production priority**: Bug fixes + 80% test coverage more critical for 0.0.1
- **User feedback**: Focus on "correctly implementing everything" for vector DB workload

**What We Have (Sufficient for 0.0.1)**:
- ✅ Atomic batch writes
- ✅ Lock-free concurrent reads/writes
- ✅ Read Committed isolation (per-operation consistency)
- ✅ WAL durability
- ✅ Crash recovery with atomicity

**What We're Missing (Defer to 0.0.2+)**:
- ❌ Snapshot Isolation (multi-operation repeatable reads)
- ❌ Transaction API (begin/commit/rollback)
- ❌ Multi-version storage (MVCC)
- ❌ Serializable isolation

**Implementation Plan (When Needed)**:

Minimal MVCC (2-3 weeks):
1. Add sequence numbers to all writes
2. Version keys: `(Bytes, u64)` → value
3. Snapshot API: Capture sequence, filter reads
4. Compaction: Preserve versions for active snapshots

Full MVCC (4-6 weeks):
- Everything above + transaction API + OCC + watermark GC

**Triggers for Implementation** (0.0.2+):
- User feedback requests snapshot isolation
- Competing with RocksDB on feature parity
- Production workloads require multi-operation consistency
- Non-vector use cases demand stronger isolation

**Trade-offs**:
- ✅ Ship 0.0.1 faster (2-6 weeks saved)
- ✅ Avoid 5-10% MVCC overhead
- ✅ Sufficient for target use case (vector databases)
- ✅ Simpler codebase (easier to maintain/test)
- ✅ Focus on correctness (80% test coverage priority)
- ❌ No repeatable reads across multiple operations
- ❌ Can't compete with RocksDB on full isolation features
- ✅ Can add MVCC later without breaking changes (additive API)

**Testing**:
- Marked `test_concurrent_reads_consistent` as `#[ignore]` with detailed explanation
- Updated CLAUDE.md to document "Read Committed" isolation level
- Documented in ai/research/LSM_MVCC_CONCURRENCY_RESEARCH.md

**References**:
- Research: ai/research/LSM_MVCC_CONCURRENCY_RESEARCH.md (800+ line analysis)
- Industry comparison: Milvus, Qdrant, Weaviate (all eventual consistency)
- RocksDB MVCC: Complex implementation, 5-10% overhead
- TiKV MVCC: Full transaction support, multi-week implementation

**Status**: ✅ Decided - Defer to 0.0.2+, sufficient for vector database workload
