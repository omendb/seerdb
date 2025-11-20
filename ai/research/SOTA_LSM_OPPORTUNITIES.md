# State-of-the-Art (SOTA) Opportunities for LSM Engines (2024/2025)

**Goal**: Identify next-generation improvements for SeerDB beyond standard RocksDB features.

## 1. Compaction Algorithms
*   **Fragmented LSM (PebblesDB)**:
    *   *Idea*: Instead of rewriting files during compaction, use "guards" to logically segment them. Drastically reduces write amplification.
    *   *Impact*: 2-3x write throughput improvement.
    *   *Applicability*: High. Rust's `fs` handling makes this viable.
*   **Silk (Compaction Scheduling)**:
    *   *Idea*: Prioritize I/O for user queries over compaction, but burst compaction when load is low.
    *   *Applicability*: High. We currently have basic `slowdown_writes` triggers.

## 2. Filter Structures
*   **Ribbon Filters (RocksDB)**:
    *   *Idea*: Boolean Gaussian Elimination based filter. 30% smaller than Bloom Filters for same false positive rate.
    *   *Applicability*: Medium. Complex to implement, but space savings are real.
*   **Learned Bloom Filters**:
    *   *Idea*: Use ML model to predict non-existence.
    *   *Applicability*: Already explored (see `ai/research/archive/learned_bloom_analysis.md`). Mixed results.

## 3. Storage & I/O
*   **NVMe Arrays (2025 Paper)**:
    *   *Finding*: File systems (ext4/xfs) are the bottleneck for 10+ NVMe drives.
    *   *Solution*: Userspace I/O (SPDK/io_uring) or careful file system tuning (block alignment).
    *   *Relevance*: High for `omendb` scale-out.
*   **io_uring (Linux)**:
    *   *Idea*: True async I/O ring buffer. 
    *   *Applicability*: High for Linux builds. Essential for >1M IOPS.

## 4. Concurrency & Transactions
*   **PhoebeDB Parallel WAL (EDBT 2025)**:
    *   *Idea*: Remote Flush Avoidance (RFA). Threads flush their own buffers to pre-allocated file offsets (`pwrite`) to avoid global locks.
    *   *Impact*: 27x throughput vs PostgreSQL.
    *   *Relevance*: Future optimization for SeerDB WAL.
*   **Optimistic Concurrency Control (OCC)**:
    *   *Idea*: Validate read set at commit time. No locks for readers.
    *   *Applicability*: Essential for implementing ACID transactions (0.2.0 goal).

## 5. Compaction: Fjall 2.3 (Nov 2024)
*   **Random Write Scaling**:
    *   *Idea*: Optimize L0->L1 compaction picker to minimize overlap.
    *   *Impact*: Prevents write stalls under random UUID workloads.
    *   *Action*: Audit `compaction/mod.rs` against this pattern.

## 6. Buffer Management: Umbra (The "Lipah" Connection)
*   **Umbra Variable-Size Buffer Manager (2020)**:
    *   *Evolution*: Built by LeanStore authors (Viktor Leis).
    *   *Key Feature*: **Variable-Size Pages**.
    *   *Why it fits SeerDB*: Our SSTable blocks are compressed (variable size). LeanStore (fixed pages) struggles here. Umbra manages variable segments efficiently.
    *   *Mechanism*: Uses virtual memory (`mmap`) for large allocations, optimistically swizzles pointers.

## 7. omendb-Specific Optimizations (Graph/Vector)
*   **Merge Operator (Associative updates)**:
    *   *Problem*: Updating a graph adjacency list (e.g., adding an edge) usually requires Read-Modify-Write (RMW). This is slow and creates write amp.
    *   *Solution*: `Merge(key, partial_value)`. The engine merges them during compaction.
    *   *Impact*: O(1) write for edge addition. Critical for graph ingest.
*   **Prefix Bloom Filters**:
    *   *Problem*: Graph traversal = tons of `seek(node_id)`.
    *   *Solution*: **Prefix Bloom Filters**. Filter SSTables by prefix to avoid seeks in files that don't contain the node.
*   **Row-Cache / Point-Lookup Cache**:
    *   *Idea*: Cache individual K/V pairs for hot nodes/vectors, bypassing block decompression.

## Proposal for SeerDB (Revised)
1.  **Merge Operator**: Implement full `Merge` API (extends CompactionFilter).
2.  **Compaction Picking**: Audit `compaction/mod.rs` for random key scaling (Fjall 2.3).
3.  **Prefix Bloom Filters**: Optimize `prefix_seek`.
4.  **Parallel WAL**: Consider RFA (PhoebeDB) for >64 cores (Phase 3).

