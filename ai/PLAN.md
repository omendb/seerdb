# PLAN - SeerDB: The "omendb" Engine

**Goal**: Optimize SeerDB specifically for `omendb` (Vector/Graph workloads) while maintaining general-purpose excellence.

**Status**: ACTIVE (Nov 20, 2025)

## 1. The "omendb" Workload
*   **Data Model**: Graphs (Adjacency Lists) + Vectors (Blobs).
*   **Access Pattern**:
    *   **Ingest**: Massive edge addition (appending to lists).
    *   **Query**: Prefix scans (`node_id:*`) and Point lookups (`node_id:property`).
*   **Constraints**: Latency sensitive, high write throughput required.

## 2. Critical Features (Priority Order)

### A. Merge Operator (The "Graph Killer" Feature) ✅ **COMPLETE**
*   **Why**: Currently, adding an edge requires `Get` -> `Deserialize` -> `Append` -> `Serialize` -> `Put`. This is slow (RMW).
*   **Solution**: `db.merge(key, new_edge)`.
    *   Writes are O(1) (append to WAL/Memtable).
    *   Read pays the cost (merging on the fly).
    *   Compaction merges permanently.
*   **Status**: Implemented & Merged.
*   **Action**: None (Maintenance).

### B. Prefix Bloom Filters ✅ **COMPLETE**
*   **Why**: `omendb` relies heavily on `prefix_scan(node_id)`.
*   **Problem**: Standard Bloom Filters only hash the full key. A scan for `prefix:` has to check every SSTable unless we index prefixes.
*   **Solution**: Create a separate Bloom Filter for key prefixes (e.g., fixed length or separator based).
*   **Status**: Implemented & Enabled.
*   **Action**: Benchmark performance gain.

### C. MVCC Transactions (Snapshot Isolation) 🔄 **NEXT**
*   **Why**: Graph updates often span multiple keys (e.g., Node A -> Node B requires updating both adj lists).
*   **Solution**: Optimistic Concurrency Control (OCC).
*   **Action**: Implement `Transaction` API.

## 3. Buffer Management Strategy
*   **Decision**: Stick to **Software Buffer Pool** (Safe Rust).
*   **Rejected**: Pointer Swizzling (Too unsafe/complex for now), `vmcache` (Linux only).
*   **Goal**: Optimize the existing `BufferPool` (e.g. lock contention) rather than rewriting architecture.
*   **Status**: LeanStore Phase 1 Integrated.

## 4. Execution Plan

1.  **Merge Operator**: Done.
2.  **Prefix Bloom**: Done.
3.  **Benchmarks**: Verify SOTA claims (Next).
4.  **MVCC**: Correctness for multi-key graph updates.
