# Strategic Plan: SOTA Vector Database Foundation

**Goal**: Build `seerdb` into a best-in-class storage engine capable of supporting massive vector scale.

**Testing**:
- **Mac (Dev)**: Use `tokio` (default) and `object_store` with Local/Memory backend.
- **Fedora (Perf)**: Use `ssh fedora` to test `io_uring` and Linux-specific optimizations.

## Phases

| Phase | Status | Deliverables | Success Criteria |
|-------|--------|--------------|------------------|
| **Phase 1: Core & Cloud** | 🟡 Active | Hybrid Storage (S3), Compaction Filters | 100GB+ scale, Local/S3 parity |
| **Phase 2: SOTA Pipeline** | 📅 Planned | WAL Pipelining, SIMD, Async Flush | >1M writes/sec, optimized CPU |
| **Phase 3: Linux & Scale** | 📅 Planned | `io_uring` (Fedora), LeanStore Buffer | Max single-node IOPS |
| **Phase 4: Release** | 📅 Planned | Fuzzing, Soak Tests, Documentation | Production Stability |

## Dependencies

| Must Complete | Before Starting | Why |
|---------------|-----------------|-----|
| **Object Store** | Phase 1 | Critical for scale. Test with `Local` backend first. |
| **Compaction Filters** | Phase 1 | `omendb` blocked without this. |
| **WAL Pipelining** | Phase 2 | Key bottleneck for concurrent writes. |
| **io_uring** | Phase 3 | SOTA I/O for Linux/Production (test on Fedora). |

## Technical Architecture

### I/O Layer
*   **Development (Mac)**: `tokio` with `std::fs` or `tokio::fs`.
*   **Production (Linux)**: `io_uring` for NVMe saturation.
*   **Object Storage**: `object_store` crate.
    *   *Dev*: `LocalFileSystem` or `InMemory`.
    *   *Prod*: `AmazonS3`, `GoogleCloudStorage`.

### Release Strategy
**Sequential Execution**: Complete `seerdb` fully (including Phase 3) before major `omendb` feature work. `omendb` requires a stable, high-performance foundation.
