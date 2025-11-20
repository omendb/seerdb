# Prefix Iteration Optimization - SOTA Research

**Date**: November 17, 2025
**Focus**: General LSM storage engine prefix scan performance
**Scope**: RocksDB, LevelDB, BadgerDB, Cassandra/ScyllaDB, Pebble, SlateDB

---

## Problem Statement

**Current seerdb performance**: 30,943 prefix scans/sec, 97.38% cache hit rate (excellent)
**Graph workloads @ 10K scale**: 1002ms per query (target: <200ms) - 11.4x read amplification

**Root cause**: Iterator creation overhead + sequential block access patterns

---

## SOTA Patterns Identified

### 1. Read-Ahead Prefetching (RocksDB, SlateDB)

**Pattern**: Prefetch next N blocks during sequential scans

**RocksDB implementation**:
- `ReadOptions::readahead_size` (default: 256KB for sequential scans)
- Prefetches blocks into OS page cache
- 2-3x improvement for sequential workloads

**SlateDB implementation**:
- `ScanOptions::read_ahead_bytes` parameter
- Explicit control over prefetch size
- Tunable per-query

**Key insight**: Sequential scans are predictable - load blocks before they're needed

### 2. Key-Only Iteration (BadgerDB, Pebble)

**Pattern**: Skip value reads when only keys are needed

**BadgerDB implementation**:
```go
opts := badger.DefaultIteratorOptions
opts.PrefetchValues = false  // Key-only mode
it := txn.NewIterator(opts)
```

**Pebble implementation**:
- Iterator with `Values: false` option
- 5-10x faster for count/exists operations
- Reduces I/O and deserialization overhead

**Use cases**:
- Count operations
- Key existence checks
- Prefix membership tests

### 3. Index-Only Scans (Cassandra/ScyllaDB)

**Pattern**: Binary search through promoted index without reading data blocks

**ScyllaDB SSTable v3**:
- Promoted index with offsets array
- O(log N) binary search instead of O(N) scan
- Reduces index traversal from linear to logarithmic

**Key insight**: For large partitions, index alone can answer range queries

### 4. Batch Operations (RocksDB MultiGet)

**Pattern**: Amortize lookup overhead across multiple keys

**RocksDB MultiGet()**:
- Batch multiple point lookups
- Single pass through index/cache
- Reduces per-key overhead

**Applicability to prefix scans**: Batch multiple sequential prefix scans

---

## Current seerdb Implementation Analysis

**What we have** ✅:
- Iterator-level index block caching (`current_index_block`)
- Global block cache (97.38% hit rate)
- Two-level index (top-level → index blocks → data blocks)
- Lazy loading (blocks loaded on-demand)

**What we're missing** ❌:
- No read-ahead prefetching
- No key-only iteration mode
- No batch prefix API
- No index-only scan optimization

---

## Recommendations (Priority Order)

### Priority 1: Read-Ahead Prefetching

**Complexity**: Low (100 lines)
**Expected impact**: 2-3x for sequential prefix scans
**Workloads helped**: All sequential scans (vector DB, time-series, graph)

**Implementation**:
1. Add `readahead_size` field to `SSTableRangeIterator` (default: 2 blocks)
2. In `advance_to_next_data_block()`, prefetch next N blocks
3. Load blocks into cache (cache hits on subsequent access)

**No threading needed** - inline prefetching sufficient (RocksDB approach)

### Priority 2: Key-Only Iteration

**Complexity**: Low (50 lines)
**Expected impact**: 5-10x for count/exists operations
**Workloads helped**: Aggregations, membership tests, cardinality queries

**Implementation**:
1. Add `IteratorOptions` struct with `values_only: bool`
2. In iterator, skip value decoding when `values_only == false`
3. Return `(key, None)` for key-only mode

### Priority 3: Batch Prefix API

**Complexity**: Medium (200 lines)
**Expected impact**: 3-5x for multiple small prefix scans
**Workloads helped**: HNSW graph traversal (graph specific)

**Implementation**:
1. `db.prefix_batch(&[prefix1, prefix2, ...])` API
2. Single iterator creation, multiple prefix ranges
3. Amortize index traversal overhead

### Priority 4: Index-Only Scans

**Complexity**: High (500+ lines, format change)
**Expected impact**: Logarithmic vs linear for large partitions
**Workloads helped**: Very large partitions only

**Deferred** - requires SSTable format changes, benefits limited to edge cases

---

## Validation Plan

**Benchmark**: Existing `examples/graph_prefix_scan_benchmark.rs`

**Metrics**:
- Cold cache: baseline (disk I/O bound)
- Hot cache: 2-3x improvement (read-ahead)
- Key-only: 5-10x improvement (skip values)
- Batch: 3-5x improvement (amortize overhead)

**Success criteria**:
- ✅ Read-ahead: >60,000 scans/sec hot cache
- ✅ Key-only: >150,000 scans/sec (count operations)
- ✅ No regressions on point get/put operations

---

## References

**RocksDB**:
- ReadOptions::readahead_size
- Iterator pinning of index blocks

**BadgerDB**:
- IteratorOptions::PrefetchValues
- Prefix scan with `ValidForPrefix()`

**Cassandra/ScyllaDB**:
- Promoted index with offsets array (SSTable v3)
- Binary search through index blocks

**SlateDB**:
- ScanOptions::read_ahead_bytes
- ScanOptions::cache_blocks

**Pebble**:
- Key-only iteration mode
- Block prefetching

---

## Decision

**Implement Priority 1 (Read-Ahead) + Priority 2 (Key-Only)** in this session.

**Rationale**:
- Low complexity, high impact
- General storage engine optimizations (not vector-specific)
- Battle-tested SOTA patterns
- Validate with existing benchmark

**Defer Priority 3 + 4** for future work (diminishing returns).
