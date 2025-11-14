# Superseded & Completed Decisions (November 2025)

**Purpose**: Historical archive of early decisions that have been completed, changed, or deferred

**Status**: These decisions were valuable during research/implementation but are no longer actively referenced

---

## Research Phase Decisions (Completed)

### 2. Learned Bloom Filters (Week 1 Priority) - SUPERSEDED by Traditional Blooms

**Original Decision**: Replace traditional bloom filters with learned models

**Rationale**:
- 90% space reduction claim (Kraska et al., 2018)
- Low implementation complexity (good first prototype)
- Immediate benefit (every SSTable uses bloom filters)

**What Happened**:
- Implemented but achieved 48-51% FPR (target: 1%)
- Hash-based features destroyed ML patterns
- **Superseded by**: Decision #19 (Traditional Bloom Filters)
- **Reason**: Arbitrary keys don't have learnable patterns

**Status**: Research documented in ai/research/learned_bloom_analysis.md

---

### 6. 4-Week Research Phase (Before Coding) - COMPLETED

**Decision**: Spend 4 weeks reading papers and benchmarking before building core engine

**Rationale**:
- Avoid reimplementing RocksDB (need to understand research landscape)
- Design decisions require deep understanding (can't undo later)
- Validate research claims early (prototype learned bloom filters)

**Outcome**: ✅ Completed successfully
- Research phase validated key architectural decisions
- Learned bloom filters prototyped (informed Decision #19)
- LSM tree design informed by Dostoevsky, WiscKey, ALEX papers
- Foundation for 2.5x RocksDB performance

**Status**: COMPLETED - Informed all subsequent decisions

---

### 7. Workload-Aware Optimization (Tucana-Inspired) - DEFERRED

**Decision**: Detect workload patterns and adapt compaction strategy

**Rationale**:
- database has distinct workloads (append-heavy vectors, FIFO queue, time series)
- Generic LSM tuning (RocksDB) suboptimal for all
- Tucana shows 3x throughput improvement vs RocksDB

**Status**: DEFERRED to 0.0.2+
- **Current**: Fixed Lazy Leveling strategy performs well across workloads
- **Reason**: Beat RocksDB by 2.5x without adaptive tuning
- **Future**: May add if user feedback shows need for workload-specific optimization

**References**: "Tucana" (Liu et al., 2020)

---

### 8. Learned Index Model Selection - DECIDED (Use ALEX)

**Decision**: Use ALEX-style learned index for SSTable index blocks

**Options Evaluated**:
- ✅ ALEX (gapped arrays, handles updates, proven)
- Piecewise linear models (Bourbon - for immutable data only)
- Neural networks (original Kraska paper - too complex)

**Outcome**: ✅ **ALEX implemented and validated**
- 2.2x faster than original learned index, 4.1x faster than B+trees
- **Actual results**: +55% read performance improvement
- Production-quality implementation exists (Microsoft Research)

**Status**: COMPLETED - ALEX is core component of seerdb

---

### 10. I/O Backend (DECIDED - tokio) - COMPLETED

**Decision**: Use tokio async I/O by default, io_uring as opt-in feature

**Rationale - Security First**:
- **io_uring vulnerabilities**: 77 CVEs, 60% of 2022 kernel exploits
- **Default**: tokio async I/O (safe, cross-platform, good performance)
- **Optional**: io_uring feature flag (Linux-only, opt-in, document risks)

**Status**: COMPLETED - tokio is default I/O backend
- Performance: Excellent (~10-20% slower than io_uring, but secure)
- Security: No privilege escalation risk
- Cross-platform: macOS, Linux, Windows

---

## Implementation Decisions (Completed)

### 15. Synchronous Flush and Compaction - SUPERSEDED

**Original Decision**: Flush and compaction block the write thread initially

**Rationale**:
- Simpler implementation for MVP
- Easier to reason about correctness
- Sufficient for initial validation

**What Happened**:
- Implemented background compaction (commit 2bd4074)
- Background flush implemented as opt-in (Decision #22)
- **Superseded by**: Background workers (now standard)

**Status**: COMPLETED - Background workers now standard

---

### 16. WAL Recovery on Every Open - IMPLEMENTED

**Decision**: Always replay WAL on DB::open(), even if empty

**Rationale**:
- Ensures consistency (no partial writes)
- Simple: No need to track "clean shutdown" state
- Fast: WAL small if recently flushed
- Industry standard (RocksDB, LevelDB do this)

**Status**: ✅ IMPLEMENTED - Core feature of crash recovery

---

### 17. New WAL After Recovery - IMPLEMENTED

**Decision**: Create new WAL after replaying (overwrite old)

**Rationale**:
- Old WAL data already in memtable
- Avoids ever-growing WAL
- Simpler than WAL truncation/rotation

**Status**: ✅ IMPLEMENTED - Standard WAL behavior

---

## Future Decisions (Not Yet Needed)

### 26. Pluggable Compaction Strategy - DEFERRED to Post-0.0.1

**Decision**: Add trait-based pluggable compaction to enable custom key ordering

**Use Cases**:
1. **Graph databases**: Co-locate connected nodes (2-3x traversal speedup)
2. **Time-series**: Group by time windows
3. **Spatial data**: Sort by Z-order curve or Hilbert curve
4. **Document stores**: Co-locate related documents

**Proposed API**:
```rust
pub trait CompactionStrategy: Send + Sync {
    fn reorder_for_locality(
        &self,
        keys: Vec<Vec<u8>>,
        values: Vec<Vec<u8>>
    ) -> (Vec<Vec<u8>>, Vec<Vec<u8>>);
}
```

**Status**: DEFERRED - Design phase, not yet implemented
- **Reason**: Current default compaction performs well
- **Timeline**: Post-0.0.1 (after production hardening complete)
- **Triggers**: User feedback requesting custom compaction strategies

---

## Summary

**Completed & Implemented**: #6, 8, 10, 15, 16, 17
- All core architectural decisions validated and implemented

**Superseded**: #2 (learned blooms), #15 (sync flush)
- Replaced by better alternatives based on research/profiling

**Deferred to 0.0.2+**: #7 (workload-aware), #26 (pluggable compaction)
- Not needed for initial production release
- May add based on user feedback

**Key Lesson**: Research phase successfully validated architecture
- Beat RocksDB by 2.5x without needing all planned optimizations
- Profiling-driven development more valuable than speculative optimization
- Defer complex features until proven need
