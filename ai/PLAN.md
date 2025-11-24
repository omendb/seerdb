# PLAN - SeerDB: The "omendb" Engine

**Goal**: Optimize SeerDB for `omendb` (Vector/Graph) development.
**Strategy**: Hold public release. Focus on internal features required by the graph layer.

**Status**: ACTIVE (Nov 23, 2025) - **Omendb Integration Phase**

## 1. Active Priorities (Omendb Requirements)

### A. MVCC Transactions (Snapshot Isolation) 🔄 **NEXT**
*   **Why**: Graph updates (e.g., `addEdge(A, B)`) touch multiple keys (`A->B`, `B->A`). They must be atomic and consistent.
*   **Requirement**: `db.begin_transaction()` -> `txn.get/set` -> `txn.commit()`.
*   **Implementation**:
    *   **Reads**: Use existing Snapshot mechanism (already fixed).
    *   **Writes**: Buffer in `Transaction` object.
    *   **Commit**: Atomic batch write (already supported).
    *   **Conflict Detection**: Optimistic Concurrency Control (OCC) - fail if keys modified since snapshot.

### B. Reverse Iteration 🔄 **NEXT**
*   **Why**: Time-series graph edges (e.g., "User X's recent posts") are stored as `Key: <Timestamp>`, requiring `iter_rev()` to scan newest-first.
*   **Gap**: `SSTable` and `Memtable` iterators currently only go forward.
*   **Action**: Implement `DoubleEndedIterator` for:
    1.  `MemtableIterator` (Skiplist supports this).
    2.  `SSTableIterator` (Block-based, requires efficient block caching/jumping).
    3.  `KWayMergeIterator` (Requires max-heap for reverse merge).

### C. Omendb Workload Simulation 🔄 **NEXT**
*   **Why**: Verify `seerdb` performance on the exact `omendb` access pattern before integration.
*   **Pattern**:
    *   **Write**: Massive Merge Operator usage (appending to edge lists).
    *   **Read**: High-volume `prefix_scan` (finding edges).
*   **Action**: Create `benches/omendb_simulation.rs`.

## 2. Backlog (Post-Integration)

### A. Async I/O (io_uring) 
*   **Status**: Defer. `std::fs` is fast enough for development (4.65M ops/sec).
*   **Trigger**: When Linux CPU profiling shows syscall overhead as primary bottleneck.

### B. Cloud Native Storage (S3)
*   **Status**: Feature complete for v1. Harden during scaling phase.

### C. Vector Index (HNSW)
*   **Status**: Handled in `omendb` repo (`seerdb-vector` crate).

## 3. Architecture Decisions

### Buffer Management
*   **Decision**: Stick to **Software Buffer Pool** (LeanStore-lite).
*   **Status**: Working. No major changes needed for now.

### Compaction
*   **Decision**: Tiered Compaction (Write Optimized).
*   **Reason**: Graph ingestion is write-heavy. Leveled compaction write stalls would be fatal.

## 4. Execution Plan

1.  **Tag v0.0.1-alpha** (Done) - Stable Checkpoint.
2.  **Omendb Simulation** - Establish baseline.
3.  **Reverse Iteration** - Enable time-series queries.
4.  **MVCC** - Enable safe graph updates.
5.  **Integration** - Move to `omendb` repo for full integration.