# Handoff — seerdb

## What Happened This Session

Complete architecture pivot. seerdb was a 23K-line LSM storage engine (Rust, nightly). After extensive SOTA research, we decided LSM's compaction problem (10-30x write amplification) is architectural, not tunable. Replaced everything with an out-of-place B-tree design inspired by LeanStore (VLDB 2024, 2026).

**Deleted:** 51K lines. All LSM code, tests, benches, fuzz targets, old dependencies.
**Created:** Clean skeleton on `dev` branch. Module stubs. Complete spec and testing plan.

## Key Files (all local, not in git — in ai/)

| File | Read This |
|------|-----------|
| `ai/STATUS.md` | Current state |
| `ai/design/engine_spec.md` | **Complete engine spec** — node format, PMT, buffer manager, blob, WAL, concurrency, phases |
| `ai/DECISIONS.md` | 9 ADRs with rationale |
| `ai/TODO.md` | Phase 1-6 tasks |
| `ai/design/testing_plan.md` | Testing layers, benchmarks |

## Decisions Made

1. **Out-of-place B-tree** over LSM (6-10x less flash writes, better reads, simpler)
2. **Rust stable** (no nightly)
3. **Minimal deps** (<15 crates: thiserror, bytes, crc32c, lz4_flex, zstd, tracing, parking_lot)
4. **KV separation** for large values (>1KB → blob files, like WiredTiger/InnoDB overflow pages)
5. **Testing-first** (property-based tests for every invariant, crash recovery tests)
6. **FDP/ZNS deferred to V2** (standard I/O for V1)
7. **io_uring deferred to V2** (standard fs for V1)
8. **Mojo revisit in 2027+** (Rust for now)

## References

- LeanStore: https://github.com/leanstore/leanstore (~5K lines C++ core)
- ZLeanStore: https://github.com/LeeBohyun/ZLeanStore (~7.4K lines C++, has blobs)
- Tidehunter: https://github.com/MystenLabs/tidehunter (~30K lines Rust, WAL-as-store)
- "How to Write to SSDs" (VLDB 2026): out-of-place B-tree, NoWA pattern, FDP/ZNS
- redb: https://github.com/cberner/redb (Rust CoW B-tree, competitive benchmark baseline)

## Next Steps

1. Read `ai/design/engine_spec.md` for the full spec
2. `tk ls` to see tasks
3. Start `tk-gmc9`: B-tree node format (prefix compression, variable-length keys, 4KB pages)
4. Then `tk-3ro3`: B-tree operations (insert with split, lookup, delete with merge, range scan)

## Environment

- Branch: `dev`
- Rust: stable (edition 2024)
- Compiles: `cargo check` passes, `cargo clippy --all-features -- -D warnings` clean
- No tests yet (skeleton only)
