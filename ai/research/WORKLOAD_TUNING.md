# Workload Tuning: Balancing Read vs. Write Performance

**Date**: November 19, 2025
**Status**: Research Note

## The Trade-off Spectrum
LSM-trees are fundamentally flexible. We can shift the performance curve significantly between Writes (Ingest) and Reads (Query) by tweaking compaction and memory settings.

### 1. Compaction Style (The Big Knob)

| Style | Write Amp | Read Amp | Space Amp | Best For |
|-------|-----------|----------|-----------|----------|
| **Leveled** (Current) | High (10-30x) | Low (1-5 files) | Low (10%) | **Reads**, Stable Latency, Space |
| **Tiered** (Universal) | Low (3-5x) | High (10-50 files) | High (50%) | **Writes**, Bulk Load |
| **Dostoevsky** (Hybrid) | Tunable | Tunable | Tunable | **Configurable Balance** |

*   **Current State**: SeerDB implements **Leveled** compaction (via `compaction/mod.rs` logic). This optimizes for Reads and Space.
*   **Optimization Opportunity**: Implement **Universal Compaction** (RocksDB style) as a configurable option `DBOptions::compaction_style`.
    *   *How it works*: Instead of merging L0 into L1 eagerly, we just "tier" L0 runs until we have N runs, then merge them all at once. Huge write boost, but reads check N files.

### 2. Memtable Size (Write Buffer)
*   **Larger Memtable** (e.g., 128MB - 256MB):
    *   **Writes**: Better. Delays compaction, batches more updates.
    *   **Reads**: Slightly worse (searching large skiplist is slower than bloom-filtered SSTable), but generally neutral.
    *   **Recovery**: Slower (larger WAL to replay).

### 3. Block Size & Cache
*   **Small Blocks** (4KB):
    *   **Reads**: Better for point lookups (less waste).
    *   **Writes**: Worse (more index overhead).
*   **Large Blocks** (32KB+):
    *   **Writes**: Better compression, less metadata.
    *   **Reads**: Better for scans, worse for point lookups (read amplification).

### 4. Durability (WAL)
*   `SyncPolicy::SyncAll` / `SyncData`: Safe, but limits writes to disk latency (IOPS).
*   `SyncPolicy::None` (Group Commit): Buffers writes. Throughput becomes CPU/Memory bound.

## Proposal: `OptimizationMode` Enum
We can expose a high-level enum in `DBOptions` that pre-configures these detailed knobs.

```rust
pub enum OptimizationMode {
    /// Default. Balanced for general use.
    Balanced,
    /// Optimizes for heavy ingestion.
    /// - Increases memtable size (128MB)
    /// - Uses Tiered compaction (future)
    /// - Larger blocks (16KB)
    WriteHeavy,
    /// Optimizes for low-latency point lookups.
    /// - Leveled compaction (aggressive)
    /// - Smaller blocks (4KB)
    /// - Maximizes block cache
    ReadHeavy,
    /// Optimizes for disk space usage.
    /// - Strongest compression (Zstd)
    /// - Leveled compaction
    SpaceEfficient,
}
```

## Next Steps for Research
1.  **Implement Universal/Tiered Compaction**: This is the single biggest change for Write performance (could boost writes 2-5x).
2.  **Benchmark OptimizationMode**: Verify the presets actually deliver the expected trade-offs.
