# Research: PhoebeDB (EDBT 2025)

**Source**: EDBT 2025 Paper: "PhoebeDB: A Disk-Based RDBMS Kernel for High-Performance and Cost-Effective OLTP"
**Context**: Investigated as part of SOTA durability research for SeerDB.

## Key Innovations

### 1. Parallel WAL with Remote Flush Avoidance (RFA)
*   **Problem**: Traditional WAL flushing (group commit) often serializes on a mutex or channel, becoming a bottleneck at high core counts (e.g., >32 cores).
*   **Solution**:
    *   Multiple WAL buffers (per-thread or per-group).
    *   **Remote Flush Avoidance**: A thread only flushes its *own* buffer or a designated group buffer, avoiding global lock contention.
    *   Uses `pwrite` (parallel write) to the log file at pre-allocated offsets, rather than appending serially.
*   **Impact**: 27x throughput vs PostgreSQL (which uses serial WAL).

### 2. In-Place Updates + UNDO Log
*   **Approach**: Instead of Copy-on-Write (LSM/B-tree COW), PhoebeDB modifies pages in place.
*   **Durability**: Relies on an in-memory UNDO log for transaction rollback, and the WAL for crash recovery.
*   **Relevance to LSM**: Less relevant (LSM is append-only by definition), but the WAL parallelism is directly applicable.

## Applicability to SeerDB
*   **Current State**: SeerDB uses `PipelinedWAL` (Leader/Follower). This is better than serial, but still has a "Leader" bottleneck.
*   **Next Step**: Investigate **Decentralized WAL**.
    *   Allow multiple threads to write to the WAL file concurrently at different offsets (`pwrite`).
    *   Requires atomic reservation of file space (`fetch_add` on offset).
    *   Eliminates the "Leader" thread entirely.

## Recommendation
*   Stick with **Leader/Follower** (current) for now (30x scaling is good enough for <64 cores).
*   Move to **Decentralized WAL** (PhoebeDB style) if profiling shows `WAL::put` contention on 100+ core machines.
