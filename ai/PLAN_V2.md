# PLAN V2 - SeerDB: The "omendb" Engine

**Goal**: Optimize SeerDB specifically for `omendb` (Vector/Graph workloads) while maintaining general-purpose excellence.

**Status**: DRAFT (Nov 19, 2025)

## 1. The "omendb" Workload
*   **Data Model**: Graphs (Adjacency Lists) + Vectors (Blobs).
*   **Access Pattern**:
    *   **Ingest**: Massive edge addition (appending to lists).
    *   **Query**: Prefix scans (`node_id:*`) and Point lookups (`node_id:property`).
*   **Constraints**: Latency sensitive, high write throughput required.

## 2. Critical Features (Priority Order)

### A. Merge Operator (The "Graph Killer" Feature)
*   **Why**: Currently, adding an edge requires `Get` -> `Deserialize` -> `Append` -> `Serialize` -> `Put`. This is slow (RMW).
*   **Solution**: `db.merge(key, new_edge)`.
    *   Writes are O(1) (append to WAL/Memtable).
    *   Read pays the cost (merging on the fly).
    *   Compaction merges permanently.
*   **Status**: `CompactionFilter` exists but `Merge` API is missing.
*   **Action**: Implement `MergeOperator` trait and `db.merge()`.

### B. Prefix Bloom Filters
*   **Why**: `omendb` relies heavily on `prefix_scan(node_id)`.
*   **Problem**: Standard Bloom Filters only hash the full key. A scan for `prefix:` has to check every SSTable unless we index prefixes.
*   **Solution**: Create a separate Bloom Filter for key prefixes (e.g., fixed length or separator based).
*   **Action**: Add `prefix_extractor` to `DBOptions`.

### C. Umbra-style Buffer Manager (LeanStore Phase 2)
*   **Why**: SSTable blocks are compressed (variable size). Fixed 16KB pages waste memory.
*   **Solution**: Adapt `BufferPool` to manage variable-size frames (using `mmap` logic or buddy allocator).
*   **Action**: Research "Variable-Size Buffer Management".

### D. MVCC Transactions (Snapshot Isolation)
*   **Why**: Graph updates often span multiple keys (e.g., Node A -> Node B requires updating both adj lists).
*   **Solution**: Optimistic Concurrency Control (OCC).
*   **Action**: Implement `Transaction` API.

## 3. Why NOT io_uring (Yet)?
*   **Complexity**: Requires rewriting the entire I/O stack to be async (`tokio-uring`).
*   **Benefit**: Only visible at >500k IOPS per core. Standard `pread` with `BufferPool` (cached) is sufficient for now.
*   **Context**: RocksDB is sync. We can beat RocksDB by being *smarter* (better indexing, less amplification), not just "more async".

## 4. Execution Plan

1.  **Merge Operator**: Immediate high impact for `omendb`.
2.  **Prefix Bloom**: Optimization for graph traversal.
3.  **MVCC**: Correctness for multi-key graph updates.
4.  **Umbra Buffer**: Long-term memory efficiency.
