# ai/ Directory - Development Documentation

**Purpose**: Session context for seerdb development
**Last Updated**: November 24, 2025
**Files**: 15 (cleaned from 68)

---

## Quick Start

**New to the project?** Read in this order:
1. `STATUS.md` - Current state, feature status, metrics
2. `TODO.md` - Active tasks and priorities
3. `PLAN.md` - Roadmap

---

## Directory Structure

```
ai/
├── STATUS.md           # Current state, feature matrix
├── TODO.md             # Active tasks
├── PLAN.md             # Roadmap
├── README.md           # This file
├── OMENDB_INSTRUCTIONS.md  # Omendb integration context
│
├── decisions/          # Design decisions
│   ├── storage_mvcc.md     # Current: MVCC implementation
│   └── superseded-2025-11.md   # Historical decisions
│
├── design/             # Design specifications
│   ├── seerdb_core_architecture.md  # Core architecture
│   ├── API_DESIGN.md               # API design
│   ├── BLOCK_SSTABLE_FORMAT.md     # SSTable format spec
│   ├── COMPACTION_FILTERS.md       # Compaction filters
│   ├── SIMD_DECISION.md            # SIMD implementation
│   └── TESTING_STRATEGY.md         # Testing approach
│
└── research/           # Research references
    ├── BENCHMARKS.md       # Performance data
    └── PAPERS.md           # Academic references
```

---

## Session Workflow

### Starting
1. Read `STATUS.md` - Current state
2. Read `TODO.md` - Active tasks

### During Work
- Update `TODO.md` as tasks progress
- Consult `design/` or `research/` as needed

### Ending
- Update `STATUS.md` with progress
- Mark tasks complete in `TODO.md`

---

## File Purposes

| File | When to Read | When to Update |
|------|--------------|----------------|
| `STATUS.md` | Every session | After significant work |
| `TODO.md` | Every session | When tasks change |
| `PLAN.md` | Strategic planning | Phase transitions |
| `OMENDB_INSTRUCTIONS.md` | Omendb integration | Rarely |
| `decisions/*.md` | Design questions | New decisions |
| `design/*.md` | Implementation | Design changes |
| `research/*.md` | Performance work | New benchmarks |

---

## Maintenance

- Keep session files current (<500 lines)
- Delete stale files (git preserves history)
- Archive superseded decisions to `decisions/superseded-*.md`
