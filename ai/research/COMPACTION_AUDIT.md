# Compaction Audit: Tiered vs Leveled

**Date**: November 20, 2025
**Status**: Completed
**Conclusion**: SeerDB uses **Tiered Compaction** (Size-Tiered), which is immune to the specific "Random Write Scaling" issue described in Fjall 2.3 analysis (which applies to Leveled Compaction).

## Analysis

### Current Architecture
*   **LSM Structure**: `LSMTree` manages levels L0..LN.
*   **L0 Flush**: Memtables flush to L0. L0 triggers compaction based on file count (`>= 4`).
*   **Compaction Logic**: `do_compact_level` takes overlapping files from Level N and merges them into Level N+1.
*   **L1+ Structure**: Since `do_compact_level` appends new files to the next level without reading/merging existing files in the target level, **all levels are Tiered** (overlapping sorted runs).

### Random Write Scaling
*   **Fjall 2.3 Issue**: In Leveled Compaction, writing random keys (UUIDs) causes new L0 files to overlap with the *entire* L1. Compacting L0->L1 requires rewriting all of L1. Write Amp = Size(L1)/Size(L0) (e.g., 10x).
*   **SeerDB Behavior**: In Tiered Compaction, L0 files are simply merged together and appended to L1. Existing L1 files are untouched. Write Amp = 1.0 (for that step).
*   **Verification**: `examples/compaction_stress_test.rs` showed stable throughput with random UUID writes. Compaction kept up with ingestion (L0 count stayed low).

### Trade-offs
| Feature | SeerDB (Current / Tiered) | Leveled (RocksDB/Fjall) |
| :--- | :--- | :--- |
| **Write Amp** | **Low** (Good) | High (Bad for random writes) |
| **Read Amp** | **High** (Bad) - Checks all files | Low (Good) - Checks 1 file per level |
| **Space Amp** | High (Temporary space during merge) | Low |

### Recommendations

1.  **Keep Current Tiered Approach for Now**: It favors write throughput, which fits the "high-performance ingestion" goal.
2.  **Future Optimization: Lazy Leveling**: To fix Read Amp, we should eventually move to **Lazy Leveling** (Tiered L0..LN-1, Leveled LN). This gives the best of both worlds.
3.  **Skip "Fjall 2.3 Smart Picker"**: This is specific to Leveled compaction (minimizing overlap). It does not apply to our current Tiered architecture.
4.  **Focus on Prefix Bloom Filters**: Since we are Tiered, we have many SSTables to check. Prefix Bloom Filters are **critical** to reduce Read Amp for graph traversals (`prefix()` queries).

## Next Steps
*   Mark "Compaction Optimization" as "Verified / Skipped (Architecture Mismatch)".
*   Start **Prefix Bloom Filters** implementation immediately to address the Read Amp weakness of the current Tiered architecture.
