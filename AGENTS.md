# seerdb - Research-Grade Storage Engine

**Repository**: seerdb (Storage Engine with Learned Data Structures)
**Last Updated**: November 18, 2025
**License**: Apache-2.0
**Status**: ALPHA - Feature complete for core operations

---

## Product Overview

**seerdb**: Modern embedded storage engine implementing 2018-2024 research

**What It Is**:
- LSM-tree based storage engine (like RocksDB)
- Learned data structures (ALEX) replace traditional components
- WiscKey separation for minimized write amplification
- Optimized for graph workloads and massive scale

**Positioning**: "RocksDB but with 2020s research - 4.82x better write amplification (validated)"

**Key Features**:
- ✅ **Performance**: 878K writes/sec (2.47x RocksDB)
- ✅ **Snapshots**: Point-in-time consistent views
- ✅ **Range Iterators**: `range()`, `prefix()`
- ✅ **Observability**: Detailed metrics & health checks

---

## Quick Start for AI Agents

**→ First time?** Load these in order:
1. This file (AGENTS.md / CLAUDE.md symlink) - Project overview
2. `ai/STATUS.md` - Current state (read FIRST)
3. `ai/TODO.md` - Active tasks and priorities
4. `ai/PLAN.md` - Strategic roadmap (Massive scale)
5. `ai/design/seerdb_core_architecture.md` - Core Architecture

**→ Deep Context?** See `ai/research/PROJECT_CONTEXT.md` for competitive analysis and workload details.

**→ API research?** See `ai/design/API_COMPARISON_TABLE.md` for gap analysis

**→ Full documentation guide**: See `ai/README.md` for all available docs

---

## Environment
- **Mac (M3 Max)**: Primary Development. Tests `tokio` + `LocalFileSystem` object store.
- **Fedora (i9-13900KF)**: Performance & Linux SOTA. Tests `io_uring` + `io_uring` backend.

## Workflow Rules
- NO AI attribution in commits/PRs (strip manually)
- Ask before: PRs, publishing packages, force ops, resource deletion
- Commit frequently, push regularly (no ask needed)
- Never force push to main/master
- Delete files directly (no archiving)
- `/tmp`: ephemeral test artifacts only. `ai/`: context/state across sessions
  - Delete temp artifacts after use, never commit

### ai/ Directory
**AI session context** - workspace for tracking project state across sessions. Read first, update on exit.

| File | Purpose |
|------|---------|
| `AGENTS.md` | Project overview (symlink: `CLAUDE.md` → `AGENTS.md`) |
| `ai/STATUS.md` | Current state, blockers (read FIRST) |
| `ai/TODO.md` | Active tasks only |
| `ai/PLAN.md` | Strategic roadmap |
| `ai/research/PROJECT_CONTEXT.md` | Detailed context, research, & competitive analysis |
| `ai/design/` | Design specifications |

**Format:** All ai/ files - tables/lists/structured (not prose). Answer first, evidence second.

---

## Repository Structure

```
seerdb/
├── AGENTS.md              # Primary AI agent entry point
├── CLAUDE.md → AGENTS.md  # Symlink for Claude Code compatibility
├── README.md              # Public documentation
├── LICENSE                # Elastic License 2.0
├── Cargo.toml             # Rust package manifest
├── src/
│   ├── lib.rs             # Public API
│   ├── wal/               # Write-ahead log
│   ├── memtable/          # In-memory buffer (partitioned skiplist)
│   ├── sstable/           # SSTable format
│   ├── compaction/        # Compaction strategies
│   ├── vlog/              # Value log (WiscKey)
│   ├── cache/             # Block cache (quick_cache)
│   └── simd/              # SIMD optimizations
├── examples/              # Usage examples + benchmarks
├── benches/               # Performance benchmarks
├── tests/                 # Integration tests
└── ai/                    # AI session context
```
