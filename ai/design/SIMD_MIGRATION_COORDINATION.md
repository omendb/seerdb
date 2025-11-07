# SIMD Migration Coordination: OmenDB ↔ SeerDB

**Date**: November 5, 2025
**Status**: Both projects migrating to std::simd

---

## Summary

**SeerDB**: ✅ Already using std::simd (behind `simd` feature flag)
**OmenDB**: 🔨 Currently migrating from hand-rolled intrinsics → std::simd

Both projects now use the same SIMD approach for consistency and maintainability.

---

## SeerDB Current State

**SIMD Implementation**: `std::simd` (portable SIMD)

**Files**:
- `src/alex/simd_search.rs` - SIMD-accelerated search for ALEX gapped arrays
- `src/bloom/simd.rs` - SIMD-accelerated Bloom filter
- `benches/simd_profiling.rs` - SIMD performance benchmarks
- `examples/bloom_simd_benchmark.rs` - Bloom filter SIMD examples

**Feature Flag**: `simd`
```toml
# Cargo.toml
[features]
simd = []  # Enables std::simd (requires nightly)
```

**Usage**:
```rust
#[cfg(feature = "simd")]
use std::simd::{cmp::SimdPartialEq, num::SimdInt, Simd};
```

**Performance**:
- AVX2 (8 lanes): 2-3x speedup over scalar
- ARM NEON (4 lanes): 1.5-2x speedup over scalar
- Fallback: Optimized scalar implementation

**Nightly Requirement**: Yes (requires `#![feature(portable_simd)]`)

---

## OmenDB Migration (In Progress)

**Previous**: Hand-rolled SIMD intrinsics (AVX2, SSE2, NEON)
**Migrating To**: `std::simd` (portable SIMD)

**File**: `src/vector/custom_hnsw/simd_distance.rs`

**Functions**:
- `l2_distance(a: &[f32], b: &[f32]) -> f32`
- `cosine_distance(a: &[f32], b: &[f32]) -> f32`
- `dot_product(a: &[f32], b: &[f32]) -> f32`

**Previous Performance** (hand-rolled intrinsics):
- 3.1-3.9x speedup vs scalar
- 7223 QPS @ 10K vectors (128D)
- Full platform support: AVX2, SSE2, NEON

**Expected Performance** (std::simd):
- 3-4x speedup vs scalar (similar to hand-rolled)
- Cleaner code (150 lines × 3 platforms → ~50 lines total)
- Easier optimization (tune once, all platforms benefit)

**Nightly Requirement**: Yes (same as SeerDB)

---

## Why std::simd for Both Projects?

**Decision Rationale**: See `/Users/nick/github/omendb/seerdb/SIMD_EVALUATION.md`

**Key Reasons**:
1. ✅ **Not in production yet** - both projects in development phase
2. ✅ **std::simd likely stabilizes before 1.0** (2025H1 goal, probably Q1-Q2 2026)
3. ✅ **Much cleaner code** - write once vs per-platform implementations
4. ✅ **Zero external dependencies** - standard library (just nightly temporarily)
5. ✅ **Easier maintenance** - tune one implementation, all platforms benefit
6. ✅ **Consistency** - both projects use same SIMD approach

---

## Nightly Rust Coordination

**Shared Requirement**: Both projects now require nightly Rust for development

**Setup**:
```bash
# Switch to nightly for both projects
rustup default nightly

# Verify
rustc --version  # Should show "1.93.0-nightly" or later
```

**CI/CD**: Both projects need to specify nightly in:
- `.github/workflows/*.yml` - use `toolchain: nightly`
- `rust-toolchain.toml` - specify `channel = "nightly"` (optional)

**When std::simd Stabilizes**:
1. Remove `#![feature(portable_simd)]` from both projects
2. Switch back to stable Rust (`rustup default stable`)
3. Update CI/CD to use stable toolchain
4. No code changes needed (just remove feature gate)

---

## Cross-Project Learnings

**SeerDB → OmenDB**:
- ✅ Feature flag pattern (`simd` feature) allows optional SIMD
- ✅ Scalar fallback ensures code works without SIMD
- ✅ Documentation comments explain SIMD benefits (see simd_search.rs)

**OmenDB → SeerDB**:
- ✅ Distance computation patterns (L2, cosine, dot product)
- ✅ Comprehensive benchmarking (see SIMD_EVALUATION.md)
- ✅ Performance targets (3-4x speedup baseline)

---

## Shared Best Practices

**1. Feature Flag Pattern** (from SeerDB):
```rust
#[cfg(feature = "simd")]
use std::simd::*;

#[cfg(feature = "simd")]
fn simd_implementation() { /* SIMD code */ }

#[cfg(not(feature = "simd"))]
fn scalar_implementation() { /* fallback */ }

pub fn public_api() {
    #[cfg(feature = "simd")]
    return simd_implementation();

    #[cfg(not(feature = "simd"))]
    return scalar_implementation();
}
```

**2. Documentation** (both projects):
- Document expected speedup (e.g., "2-3x on AVX2")
- Explain algorithm (e.g., "8 comparisons per iteration")
- Show feature flag requirements

**3. Benchmarking** (both projects):
- Always benchmark before/after
- Test multiple data sizes (10K, 100K, 1M)
- Validate on multiple platforms (x86, ARM)

---

## Migration Checklist

**SeerDB**: ✅ Already migrated
- [x] Using std::simd (simd_search.rs, bloom/simd.rs)
- [x] Feature flag configured (`simd`)
- [x] Scalar fallback implemented
- [x] Benchmarks available

**OmenDB**: 🔨 In progress (Nov 5, 2025)
- [x] Switch to nightly Rust
- [ ] Rewrite simd_distance.rs with std::simd
- [ ] Run benchmarks (must maintain 3.1-3.9x speedup)
- [ ] Validate on Mac (NEON) and Linux (AVX2)
- [ ] Update documentation
- [ ] Commit migration

---

## Performance Targets

**SeerDB**:
- ALEX search: 2-3x speedup (AVX2), 1.5-2x (NEON)
- Bloom filter: 2-4x speedup for `contains()` operations

**OmenDB**:
- L2 distance: ≥3.1x speedup (current hand-rolled baseline)
- Cosine distance: ≥3.5x speedup
- Dot product: ≥3.9x speedup
- Target: 7000+ QPS @ 10K vectors (128D)

---

## Future Coordination

**When std::simd Stabilizes** (expected Q1-Q2 2026):

**Both Projects**:
1. Remove `#![feature(portable_simd)]` from lib.rs
2. Switch to stable Rust (`rustup default stable`)
3. Update CI/CD workflows (nightly → stable)
4. Update README.md (remove "requires nightly" note)
5. Tag coordinated releases

**Timeline**:
- **Now (Nov 2025)**: Both use std::simd on nightly
- **Q1-Q2 2026**: std::simd stabilizes (Rust goal 2025H1)
- **Post-stabilization**: Both switch to stable, no code changes needed

---

## Contact & Coordination

**If SIMD changes needed in one project**, consider impact on the other:
- Performance regressions
- API changes
- Nightly requirement changes
- Feature flag changes

**Shared Documentation**:
- `/Users/nick/github/omendb/seerdb/SIMD_EVALUATION.md` - Full decision rationale
- This file - Cross-project coordination

---

## Testing Strategy

**Shared Across Projects**:

1. **Correctness** - SIMD must match scalar results exactly
2. **Performance** - Measure speedup vs scalar baseline
3. **Platforms** - Test on x86 (AVX2) and ARM (NEON)
4. **Fallback** - Verify scalar fallback works without `simd` feature

**SeerDB Tests**:
- ALEX search correctness (key lookup)
- Bloom filter false positive rate (should match scalar)
- Performance benchmarks (benches/simd_profiling.rs)

**OmenDB Tests**:
- Distance computation correctness (L2, cosine, dot)
- HNSW recall maintenance (must stay ≥99.5%)
- Query performance (QPS, latency p50/p95/p99)
- 1M scale validation

---

## Summary

**Status**: ✅ Both projects aligned on std::simd

**Benefits**:
1. Consistency across projects
2. Shared learnings and best practices
3. Easier cross-project collaboration
4. Future-proof (std::simd stabilization coming)

**Next Steps**:
1. OmenDB: Complete migration (in progress Nov 5, 2025)
2. Both: Monitor std::simd stabilization (quarterly checks)
3. Both: Update when stable (remove feature gate, switch to stable Rust)

**Last Updated**: November 5, 2025
