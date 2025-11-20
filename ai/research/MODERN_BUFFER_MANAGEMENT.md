# Modern Buffer Management Research (Post-LeanStore)

**Goal**: Identify the next generation of buffer management for SeerDB, moving beyond "LeanStore (2018)".

## Context
- **LeanStore (2018)**:
  - Key ideas: No double buffering, pointer swizzling, optimistic latches, decentralized eviction.
  - Status: We implemented Phase 1 (BufferPool + Clock).
  - Issue: Pointer swizzling is hard/unsafe in Rust.

- **"Lipah" (?)**:
  - User mentioned "LeanStore moved to Lipah".
  - Hypothesis: This might refer to **"LIP"** (Learned Index on Persistent) or a new system from TU Munich (Umbra team).
  - *Action*: Verify if this refers to **"LIP"** or **"LIPA"** or something else.

## Candidates for Research

### 1. Umbra Buffer Manager (2020+)
- Evolution of LeanStore.
- Variable-size pages? (Umbra supports variable-size segments).
- Virtual Memory usage (`mmap` + `madvise` tricks)?

### 2. VM-Based Buffer Management (Ravied et al.)
- Using `mmap` but controlling eviction via `userfaultfd` or `madvise(MADV_DONTNEED)`.
- "Pointer Swizzling" done by hardware (MMU).
- *Rust Friendly*: References are just pointers. OS handles validity.

### 3. `qpdb` (User Reference)
- Need to understand its approach.

## Decision (Nov 19, 2025)

**Verdict: Stick to Software Buffer Pool (Phase 1).**

### Rationale
1.  **Safety**: Pointer swizzling in Rust requires extensive `unsafe` code and fights the borrow checker. Risk of memory corruption is too high for the current phase.
2.  **Portability**: `vmcache` relies on Linux-specific features (`userfaultfd`). SeerDB must run on Mac (dev) and Linux (prod).
3.  **Priorities**: `omendb` (Graph/Vector) workloads are bottlenecked by **Write Amplification** (Edge updates) and **I/O** (Traversals), not CPU cache hit latency.
    *   *Merge Operator*: 10x-100x ingest improvement (Algorithm change).
    *   *Swizzling*: 2x-3x cache hit improvement (Micro-optimization).

**Next Steps**:
1.  Focus on **Merge Operator** and **Prefix Bloom Filters** for `omendb`.
2.  Revisit Swizzling only if CPU profiling shows `BufferPool::get` > 20% of runtime.

