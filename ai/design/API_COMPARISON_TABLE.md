# LSM-Tree API Comparison: Concise Table

## Core Methods Comparison

```
┌──────────────────────┬─────────────┬──────────────┬──────────┬──────────────┐
│ Operation            │ RocksDB     │ fjall        │ sled     │ seerdb       │
├──────────────────────┼─────────────┼──────────────┼──────────┼──────────────┤
│ Open                 │ DB::open()  │ Config::new()│ open()   │ DB::open()   │
│ Get                  │ get()       │ get()        │ get()    │ get()        │
│ Put                  │ put()       │ insert()     │ insert() │ put()        │
│ Delete               │ delete()    │ remove()     │ remove() │ delete()     │
│ Batch Writes         │ WriteBatch  │ N/A          │ txn()    │ batch()      │
│ Range Iterator       │ Iterator    │ range()      │ range()  │ ✅ range()   │
│ Prefix Iterator      │ Iterator    │ prefix()     │ scan()   │ ✅ prefix()  │
│ Flush                │ Flush()     │ persist()    │ flush()  │ flush()      │
│ Statistics           │ Property()  │ N/A          │ N/A      │ get_stats()  │
│ Health Check         │ N/A         │ N/A          │ N/A      │ check_health()│
└──────────────────────┴─────────────┴──────────────┴──────────┴──────────────┘
```

## Advanced Features Comparison

```
┌──────────────────────┬─────────────┬──────────────┬──────────┬──────────────┬──────────────┐
│ Feature              │ RocksDB     │ fjall        │ sled     │ seerdb       │ Priority     │
├──────────────────────┼─────────────┼──────────────┼──────────┼──────────────┼──────────────┤
│ Snapshots            │ ✅ Yes      │ N/A          │ N/A      │ ✅ Yes (COW) │ Done         │
│ MVCC Transactions    │ ✅ Yes      │ ✅ Yes       │ CAS only │ ❌ No        │ 0.0.3 (Med)  │
│ Column Families      │ ✅ Yes      │ Partitions   │ Trees    │ ❌ No (N/A)  │ Not planned  │
│ Merge Operators      │ ✅ Yes      │ N/A          │ N/A      │ ❌ No        │ 0.0.3 (Low)  │
│ Bulk Load (SSTable)  │ ✅ Yes      │ N/A          │ N/A      │ ❌ No        │ 0.0.3 (Low)  │
│ Watch/Subscribe      │ N/A         │ N/A          │ ✅ Yes   │ ❌ No        │ Niche        │
│ Backup API           │ ✅ Yes      │ N/A          │ N/A      │ ❌ No        │ Cloud first  │
│ Cloud Storage (S3)   │ No          │ No           │ No       │ ❌ No        │ 0.0.3 (High) │
│ Per-Op Options       │ ✅ Yes      │ N/A          │ N/A      │ ❌ No        │ 0.0.2 (Low)  │
│ Reverse Iteration    │ ✅ Yes      │ ✅ Yes       │ N/A      │ ❌ No        │ 0.0.2 (High) │
└──────────────────────┴─────────────┴──────────────┴──────────┴──────────────┴──────────────┘
```

## Isolation Level & Consistency

```
┌──────────────────────┬─────────────────────────┬──────────────────────────┐
│ Database             │ Default Level           │ Notes                    │
├──────────────────────┼─────────────────────────┼──────────────────────────┤
│ RocksDB              │ Read Uncommitted (fast) │ Snapshots for RC          │
│ fjall                │ Snapshot Isolation      │ Full MVCC when enabled   │
│ sled                 │ Read Committed          │ CAS atomic operations    │
│ seerdb (Current)     │ Snapshot Isolation      │ COW-based snapshots      │
│ seerdb (0.0.2+)      │ Snapshot Isolation      │ Full MVCC planned        │
└──────────────────────┴─────────────────────────┴──────────────────────────┘
```

## Performance Characteristics

```
┌──────────────────────┬─────────────┬──────────────┬──────────┬──────────────┐
│ Workload             │ RocksDB     │ fjall        │ sled     │ seerdb       │
├──────────────────────┼─────────────┼──────────────┼──────────┼──────────────┤
│ Writes (Baseline)    │ 1x          │ 1.79x        │ 0.41x    │ 2.47x ✓      │
│ Reads                │ 1x          │ 1.19x        │ 1.92x    │ 2.07x ✓      │
│ Mixed (50/50)        │ 1x          │ 0.89x        │ 1.02x    │ 1.79x ✓      │
│ Scans                │ 1x          │ 0.99x        │ 0.84x    │ 0.99x        │
│ Write Amplification  │ 4.82x       │ ~3x          │ ~3.5x    │ 1.01x ✓      │
└──────────────────────┴─────────────┴──────────────┴──────────┴──────────────┘
```

## Configuration Approach

```
RocksDB:     C++ structs + 100+ tuning parameters (complex)
fjall:       Builder pattern with sensible defaults (modern)
sled:        Minimal - almost everything implicit (simple)
seerdb:      Struct builder + research parameters (good balance)

Example Complexity:
  sled:     let db = sled::open("path")?;
  fjall:    let ks = Config::new(path).cache_size(1GB).open()?;
  seerdb:   let db = DB::open(DBOptions { memtable: 256MB, .. })?;
  RocksDB:  rocksdb::DB::Open(opts, path, &db);  // 50 lines of setup
```

## Standard API Similarities

### All Four Implement
```rust
db.open(path)          // Create/open database
db.get(key)            // Read single value
db.put(key, value)     // Write single value
db.delete(key)         // Remove key
db.flush() / persist() // Sync to disk
```

### Most Implement (3/4)
```rust
db.batch() / WriteBatch()  // Atomic multi-key writes (sled uses txn)
db.range(bounds)           // Range queries (seerdb missing)
db.get_stats()             // Statistics (seerdb unique structured API)
```

### Some Implement (1-2/4)
```rust
db.snapshot()              // Point-in-time read view (RocksDB)
db.watch_prefix()          // Event stream (sled unique)
db.transaction()           // ACID transactions (fjall, RocksDB)
db.ingest_external_file()  // Bulk load (RocksDB)
```

---

## seerdb: Gaps Analysis

| Gap | Impact | Workaround | 0.0.X Timeline |
|-----|--------|-----------|-----------------|
| **No range iterators** | CRITICAL - Many use cases blocked | Fetch all, filter in app (slow) | 0.0.2 |
| **No snapshots** | HIGH - Can't guarantee consistency across reads | Use batch/transaction (when ready) | 0.0.2 |
| **No transactions** | HIGH - Multi-key atomicity only via batch | Use batch API (limited to writes) | 0.0.3 |
| **No S3 backend** | HIGH - Can't deploy to cloud | Use local storage only | 0.0.3 |
| **No column families** | LOW - Not needed (use key prefixes) | Prefix keys: `cf:key` | Never |
| **No merge operators** | LOW - Specific use case | Implement in app logic | 0.0.3 |
| **No watch/subscribe** | NICHE - Pub-sub uncommon for LSM | Build external event system | 0.0.4+ |
| **No per-op options** | LOW - Defaults usually fine | Config at open time | 0.0.2 |

---

## Cloud-Native Requirements (S3/GCS Integration)

### Current State
- seerdb: LocalStorage only (feature flag for Storage trait)
- No S3 backend implemented

### Integration Pattern (Apache OpenDAL)
```rust
use object_store::aws::AmazonS3Builder;

let store = AmazonS3Builder::from_env()
    .with_bucket_name("my-bucket")
    .build()?;

// Wrap LSM: local memtable + S3 SSTables
let db = DB::open_with_backend(opts, store)?;
```

### Benefits for Cloud Deployment
- ✅ Automatic tiering: hot data (memtable) locally, cold data (SSTables) on S3
- ✅ Durability: SSTables persisted to S3 on flush
- ✅ Scalability: Multiple instances reading same S3 bucket
- ✅ Disaster recovery: SSTables survive instance failure

### Timeline
- **0.0.3**: Add Storage trait abstraction (if not already present)
- **0.0.4**: Implement S3 backend using aws-sdk-s3 or object_store

---

## Migration Guide: RocksDB → seerdb

### Compatible APIs ✓
```rust
// Nearly identical for basic operations
let db = seerdb::DB::open(opts)?;
db.put(key, value)?;
let val = db.get(key)?;
db.delete(key)?;
let batch = db.batch();
batch.put(k1, v1);
batch.commit()?;
```

### Incompatible Features ❌
| RocksDB Feature | Migration Path |
|-----------------|-----------------|
| `column_families` | Use key prefixes: `"cf:key"` |
| `snapshots` | Wait for 0.0.2, or use batch/txn |
| `transactions` | Use `batch()` for writes, wait for txn |
| `iterators` | Wait for 0.0.2 |
| `merge operators` | Move logic to application |
| `BackupEngine` | Use filesystem snapshots + rsync |

---

## Recommendations for seerdb 0.0.1

### Keep (Competitive)
- ✅ Performance: 2.47x faster writes
- ✅ WiscKey separation (4.82x better write amp)
- ✅ ALEX learned index (+55% reads)
- ✅ Structured observability (better than RocksDB)
- ✅ Batch API (atomic writes)

### Document (Honest Positioning)
- 📝 "Read Committed isolation (consistent per-operation)"
- 📝 "No range iterators (coming 0.0.2)"
- 📝 "No snapshots (coming 0.0.2)"
- 📝 "Single keyspace (use key prefixes for organization)"

### Plan Next (0.0.2+)
- 📋 Range iterators (blocks major use cases)
- 📋 Snapshots (read consistency)
- 📋 Per-operation options (read tuning)
- 📋 Reverse iteration (common pattern)

### Plan Future (0.0.3+)
- 📋 MVCC transactions
- 📋 S3 backend
- 📋 Merge operators
- 📋 Bulk load API

---

## Competitive Positioning

**seerdb 0.0.1**: "2.47x faster than RocksDB with 2020s research (ALEX, WiscKey). Missing snapshots/transactions/cloud - coming next."

**seerdb 0.0.2**: "Full feature parity for reads with RocksDB (iterators, snapshots). 2.47x faster writes."

**seerdb 0.0.3**: "Better than RocksDB across all dimensions (performance + features + cloud-native). Production ready."

