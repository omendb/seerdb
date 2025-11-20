# Research: Fjall 2.3 (Rust LSM)

**Source**: Fjall 2.3 Release Notes (Nov 2024)
**Context**: Direct competitor analysis (Rust embedded LSM).

## Key Innovation: Write Scaling for Random Keys

### The Problem
*   **Leveled Compaction**: When inserting random keys (e.g., UUIDs), the L0 -> L1 compaction triggers constantly.
*   **Fragmentation**: If keys are uniformly distributed, they overlap with *all* L1 files.
*   **Stall**: Compaction can't keep up, causing write stalls.

### The Solution (Fjall 2.3)
*   **Smart Picker**: Instead of picking the "oldest" L0 file, pick the set of files that minimizes overlap with the next level.
*   **Tiering in L0**: Be more aggressive about delaying L0->L1 merge. Let L0 build up (Tiered) before merging to L1.
*   **Result**: Much higher write throughput for random workloads.

## Relevance to SeerDB
*   **Current**: SeerDB uses Dostoevsky (Lazy Leveling).
*   **Action**: Review `compaction/mod.rs`.
    *   Ensure we are not eagerly merging L0->L1 for random workloads.
    *   Implement a "overlap ratio" heuristic in the compaction picker.

## Other Notes
*   **LZ4 Unsafe**: Fjall moved to `unsafe` LZ4 for 5-15% speedup. We should consider this for `seerdb` (behind a feature flag).
