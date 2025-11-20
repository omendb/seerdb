# Snapshot Implementation: Memtable Switching (Copy-On-Write)

**Date**: November 18, 2025
**Status**: Implemented in 0.0.1-alpha

## Overview

`seerdb` implements Snapshot Isolation using a **Memtable Switching** (Copy-On-Write) approach. This ensures strong consistency for concurrent reads without blocking writes, but comes with specific performance trade-offs compared to MVCC-based snapshots (like RocksDB).

## Mechanism

When `db.snapshot()` is called:

1.  **Lock**: Acquires the flush mutex (briefly).
2.  **Swap**: Atomically replaces *all* active memtable partitions with new, empty ones.
3.  **Pin**: The old memtables are pinned in the `Snapshot` struct (preventing drop).
4.  **Flush**: The old memtables are queued for background flushing.
5.  **SSTables**: The current set of SSTables is captured and pinned.

## Trade-offs

### ✅ Pros (Why we did this for 0.0.1)
-   **Simplicity**: Avoids complex MVCC version management in the memtable (no sequence number checks per key during normal gets).
-   **Correctness**: Guarantees that a snapshot sees *exactly* the state at creation time, as subsequent writes go to entirely new memory structures.
-   **Read Performance**: Normal `get()` operations don't pay the penalty of skipping over newer versions in the memtable (unlike RocksDB which must scan past newer sequence numbers).

### ❌ Cons (The Cost)
-   **Write Amplification**: Forcing a memtable switch triggers a flush, even if the memtable is nearly empty. High-frequency snapshots will create many small L0 SSTables, increasing compaction pressure.
-   **Memory Usage**: Snapshots pin immutable memtables until they are dropped. Long-lived snapshots can retain significant memory.

## SOTA Context

-   **RocksDB/LevelDB**: Use **MVCC** (Multi-Version Concurrency Control). Memtables contain versioned keys (`Key + SeqNum`). Snapshots essentially pin a `SeqNum`, and reads ignore keys with `Seq > SnapshotSeq`. This avoids forcing flushes but adds overhead to every read and complexity to compaction/garbage collection.
-   **seerdb Approach**: Similar to **Redis BGSAVE** (fork/COW) concept but at the application level. It's valid for workloads where snapshots are occasional (e.g., backups, periodic analytics) rather than per-request.

## Recommendation

-   **Use Case**: Excellent for backups, "stop-the-world" consistent scans, and low-frequency analytic queries.
-   **Avoid**: Do not use `db.snapshot()` for every single read request in a high-throughput online transaction processing (OLTP) workload.
-   **Future**: For 0.0.3+, we may investigate full MVCC if high-frequency snapshots are required.
