# seerdb

High-performance out-of-place B-tree storage engine for NVMe SSDs. Written in Rust.

## Project Structure

| Directory | Purpose |
|---|---|
| `ai/` | AI session context (local only, not tracked in git) |
| `src/` | Core implementation |
| `tests/` | Integration tests |
| `benches/` | Criterion benchmarks |

### Source Modules

| Module | Responsibility |
|---|---|
| `btree/` | B-tree data structure, node format, operations |
| `buffer/` | Buffer pool, page guards, eviction |
| `space/` | File management, page allocation, FDP/ZNS |
| `blob/` | Blob file management, KV separation, GC |
| `concurrency/` | Latches, optimistic lock coupling |
| `recovery/` | WAL, crash recovery, log manager |
| `mvcc/` | Transaction management, snapshots, PMT |

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (stable, edition 2024) |
| Compression | LZ4 (fast), ZSTD (high ratio) |
| Testing | proptest, criterion, cargo-fuzz |

## Commands

```bash
# Build
cargo build --release

# Test
cargo test --lib              # Unit tests
cargo test                    # All tests
cargo test --features zns     # With ZNS support

# Bench
cargo bench

# Lint
cargo clippy --all-features -- -D warnings
```

## Verification Steps

- Build: `cargo build --release` (zero errors)
- Tests: `cargo test --lib` (all pass)
- Clippy: `cargo clippy --all-features -- -D warnings` (zero warnings)
- Docs: `cargo doc --no-deps` (zero warnings)

## Architecture

See `ai/design/engine_spec.md` for the complete specification.

- **Out-of-place B-tree**: Pages never updated in place. PMT tracks locations.
- **KV separation**: Large values (>1KB) stored in append-only blob files.
- **SSD-native**: O_DIRECT for predictable latency, FDP/ZNS for minimal write amp.
- **MVCC**: Copy-on-write provides natural snapshot isolation.

## API (planned)

```rust
use seerdb::{DB, Options};

let db = DB::open("./my_db", Options::default())?;
db.put(b"key", b"value")?;
let val = db.get(b"key")?;
db.delete(b"key")?;
```

## Current Focus

See `ai/STATUS.md` for current state, `ai/TODO.md` for tasks, `tk ls` for priorities.
