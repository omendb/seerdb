# STATUS - seerdb

**Last Updated**: November 24, 2025
**Current Phase**: Omendb Development (Integration Prep)
**Version**: 0.0.1-alpha (Stable Checkpoint)
**Status**: Active Development

**Strategic Shift (Nov 23, 2025)**:
- **Decision**: Halted public release of v0.0.1-alpha to prioritize `omendb` integration.
- **New Focus**: MVCC Transactions (Snapshot Isolation) and Graph optimizations.
- **Architecture Decision**: Implementing Native MVCC via "Internal Keys" (RocksDB style) rather than external libraries.

**Recent Work (Nov 24, 2025)**:
- **MVCC Core**: ✅ **COMPLETE**
  - Created `InternalKey` (UserKey + SeqNum + Type) in `src/types.rs`
  - Refactored `Memtable` to use `InternalKey` (breaking change)
  - Versioned WAL `Record` to include sequence numbers
  - Updated `db.rs` to assign sequence numbers in `put()`, `delete()`, `merge()`
  - Fixed WAL writer to include length prefixes (reader/writer compatibility)
  - Fixed WAL reader to pass correct buffer to Record::decode
  - Fixed flaky crash recovery test (adjusted corruption offset for MVCC record format)
  - All 189 lib tests passing
  - All integration tests passing

**Current State**:
- **Compilation**: ✅ Clean (no errors, no warnings)
- **Tests**: ✅ All passing (189 lib + integration tests)
- **MVCC Foundation**: Complete - sequence numbers flowing through WAL → Memtable → SSTable
- **Uncommitted Changes**: 16 files modified, ready to commit

**Remaining MVCC Tasks**:
1. SSTable lookup with InternalKey (bloom filter query using user key)
2. Snapshot API for reads at specific sequence numbers
3. Garbage collection for old versions

**Metrics (v0.0.1-alpha baseline)**:
- **Writes**: 878K ops/sec
- **Reads**: 4.65M ops/sec
- **Graph Merge**: 230K ops/sec
