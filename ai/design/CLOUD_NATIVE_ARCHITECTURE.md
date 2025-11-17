# seerdb Cloud-Native Architecture

**Status**: Design document for 0.0.3+
**Goal**: Enable S3/GCS/Azure deployment with hybrid storage model
**Timeline**: 3-4 weeks implementation (0.0.3)

---

## Problem Statement

**Current**: seerdb only supports LocalStorage
- ✅ Fast (local disk + memory)
- ✅ Simple
- ❌ Not cloud-deployable
- ❌ No disaster recovery (instance failure = data loss)

**Goal**: Cloud-native deployment with:
- Durability: SSTables on S3/GCS (survives instance failure)
- Performance: Memtable local (fast writes)
- Scalability: Multiple instances reading same bucket
- Cost-effective: Pay for what you use (no persistent instance)

---

## Architecture: Hybrid Local + Cloud

```
┌─────────────────────────────────────────────┐
│  Application (REST API, Lambda, etc.)       │
├─────────────────────────────────────────────┤
│  seerdb Engine                              │
├──────────────────┬──────────────────────────┤
│  Memtable        │  LSM Levels              │
│  (Local, Fast)   │  (S3/GCS, Durable)      │
├──────────────────┼──────────────────────────┤
│  WAL (Local SSD) │  SSTables → S3/GCS      │
│  (Safety)        │  (Persistent)           │
├──────────────────┴──────────────────────────┤
│  Local:              Cloud:                  │
│  - /tmp/wal/        - s3://bucket/sstables/ │
│  - Memtable         - s3://bucket/vlog/     │
│  - Block cache      - s3://bucket/meta/     │
└──────────────────────────────────────────────┘
```

### Hybrid Model Benefits

| Component | Storage | Why |
|-----------|---------|-----|
| **Memtable** | Local SSD | Fast writes (microseconds) |
| **Block Cache** | Local RAM | Fast reads (nanoseconds) |
| **WAL** | Local SSD | Durability guarantee |
| **SSTables** | S3/GCS | Persistent, cheap, shareable |
| **VLog** | S3/GCS | Large values, backup |
| **Metadata** | S3/GCS | Recovery, multi-instance |

### Write Path
```
1. Application puts key-value
2. WAL write → Local SSD (fast)
3. Memtable write → Local RAM (fast)
4. Return to app (microseconds)
5. [Background] Memtable full → flush to S3 (slow but async)
6. [Background] Compaction → merge SSTables on S3
```

### Read Path
```
1. Application gets key
2. Check memtable → found? return (fast)
3. Check block cache → found? return (fast)
4. S3 lookup → read SSTable header (slow)
5. Return value (milliseconds)
```

**Key insight**: Hot data stays local, cold data on S3. This is natural LSM behavior!

---

## Storage Abstraction (Trait)

### Current Code
```rust
// src/storage.rs
pub struct LocalStorage { /* ... */ }

impl Storage for LocalStorage {
    fn put(&self, path: &str, data: &[u8]) -> Result<()> { /* ... */ }
    fn get(&self, path: &str) -> Result<Vec<u8>> { /* ... */ }
    fn delete(&self, path: &str) -> Result<()> { /* ... */ }
    fn list(&self, prefix: &str) -> Result<Vec<String>> { /* ... */ }
    fn exists(&self, path: &str) -> Result<bool> { /* ... */ }
}
```

### Add Cloud Backends
```rust
// Leverage existing object_store crate (Apache OpenDAL compatible)
use object_store::{ObjectStore, path::Path};

pub struct S3Storage {
    store: Arc<dyn ObjectStore>,
    bucket: String,
    prefix: String,  // e.g., "seerdb/prod/"
}

impl Storage for S3Storage {
    fn put(&self, path: &str, data: &[u8]) -> Result<()> {
        let full_path = Path::from(format!("{}{}", self.prefix, path));
        self.store.put(&full_path, data.into())?;
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = Path::from(format!("{}{}", self.prefix, path));
        let bytes = self.store.get(&full_path).await?;
        Ok(bytes.to_vec())
    }

    // Similar for delete, list, exists
}

// Similar for GCSStorage, AzureStorage using object_store implementations
```

### Trait Design
```rust
pub trait Storage: Send + Sync {
    /// Write data to path
    fn put(&self, path: &str, data: &[u8]) -> Result<()>;

    /// Read data from path
    fn get(&self, path: &str) -> Result<Vec<u8>>;

    /// Delete object at path
    fn delete(&self, path: &str) -> Result<()>;

    /// Check if path exists
    fn exists(&self, path: &str) -> Result<bool>;

    /// List all objects with prefix
    fn list(&self, prefix: &str) -> Result<Vec<String>>;

    /// Get file size
    fn size(&self, path: &str) -> Result<u64>;

    /// Multi-part upload (for large files)
    fn put_chunked(&self, path: &str, chunks: impl Iterator<Item = &[u8]>) -> Result<()> {
        // Default: concatenate chunks
        let mut data = Vec::new();
        for chunk in chunks {
            data.extend_from_slice(chunk);
        }
        self.put(path, &data)
    }
}
```

---

## Implementation: Three Modes

### Mode 1: Pure Local (0.0.1 - Current)
```rust
let db = DB::open(DBOptions::default())?;
// Uses LocalStorage internally
// Data: /path/to/db/wal/, /path/to/db/sstables/
```

### Mode 2: Hybrid (0.0.3 - Proposed)
```rust
use seerdb::{DB, DBOptions, HybridStorage};
use object_store::aws::AmazonS3Builder;

// Create S3 backend
let store = AmazonS3Builder::from_env()
    .with_bucket_name("my-data-bucket")
    .build()?;

let opts = DBOptions {
    data_dir: PathBuf::from("/tmp/seerdb_local"),  // Memtable + WAL
    storage: Arc::new(HybridStorage {
        local: LocalStorage::new("/tmp/seerdb_local"),
        remote: S3Storage::new(store, "seerdb/prod/"),
    }),
    ..Default::default()
};

let db = DB::open(opts)?;

// Write: local memtable + WAL (fast)
db.put(b"key", b"value")?;

// Flush: memtable → S3 (slow but async)
db.flush()?;

// Read: memtable/cache → S3 if needed
let val = db.get(b"key")?;
```

### Mode 3: Pure Cloud (Future - 0.0.4+)
```rust
// Everything on S3 except memtable (requires local caching strategy)
// NOT recommended for 0.0.3 (too complex)
```

---

## File Organization on S3

```
s3://my-bucket/
├── seerdb/
│   ├── prod/                         # Prefix for this instance
│   │   ├── metadata/
│   │   │   ├── version.json          # Format version, features
│   │   │   ├── manifest.json         # Current SSTable list
│   │   │   └── recovery.log          # Recovery info
│   │   ├── sstables/
│   │   │   ├── 20251116_001.sst      # Level 0 SSTable
│   │   │   ├── 20251116_001.idx      # ALEX index
│   │   │   ├── 20251116_001.bf       # Bloom filter
│   │   │   └── ...
│   │   └── vlog/
│   │       ├── 20251116_001.vlog
│   │       ├── 20251116_001.offset
│   │       └── ...
│   └── staging/                      # Temporary files
│       └── temp_sstable_<id>.sst
```

### Naming Convention
```
YYYYMMDD_<sequence>.<ext>

Examples:
20251116_001.sst  - Level 0 SSTable created on Nov 16, 2025
20251116_002.idx  - Index for the above
20251117_045.sst  - Level 1 SSTable created on Nov 17, 2025

Why date? Sortable chronologically, easy to find old files for cleanup
```

### Manifest File (metadata/manifest.json)
```json
{
  "timestamp": 1731746400,
  "version": "0.0.3",
  "levels": [
    {
      "level": 0,
      "files": [
        {"name": "20251116_001.sst", "size": 10485760, "key_count": 100000},
        {"name": "20251116_002.sst", "size": 10485760, "key_count": 100000}
      ]
    },
    {
      "level": 1,
      "files": [
        {"name": "20251117_045.sst", "size": 104857600, "key_count": 1000000}
      ]
    }
  ],
  "vlog": {
    "segments": [
      {"name": "20251116_001.vlog", "size": 52428800}
    ]
  }
}
```

---

## Multi-Instance Safety

### Problem
Multiple instances reading same S3 bucket: race conditions?

### Solution
1. **Manifest versioning**: Include timestamp + UUID
2. **Atomic writes**: S3 PUT is atomic (fail or succeed, never partial)
3. **Read-only instances**: Share bucket safely
4. **Single writer**: Only one instance writes at a time

### Pattern: Read-Only Replica
```rust
// Instance A: Primary (writes)
let db_write = DB::open(DBOptions {
    read_only: false,
    ..
})?;

// Instance B: Read-only replica (shares S3)
let db_read = DB::open(DBOptions {
    read_only: true,  // Auto-refresh manifest periodically
    ..
})?;

// Instance B sees writes from A after manifest refresh
db_read.get(b"key")?;  // Reads from S3 SSTables
```

### Manifest Refresh
```rust
// Read-only instances periodically check manifest
// Default: Every 5 seconds (configurable)
while true {
    new_manifest = s3.get("metadata/manifest.json")?;
    if new_manifest.timestamp > current.timestamp {
        // Load new SSTables
        current = new_manifest;
        invalidate_cache();  // Force re-read from S3
    }
    sleep(5_seconds);
}
```

---

## Authentication & Credentials

### AWS S3 (Standard)
```rust
// Option 1: Environment variables (IAM role on EC2)
let store = AmazonS3Builder::from_env().build()?;

// Option 2: Explicit credentials
let store = AmazonS3Builder::new()
    .with_access_key_id("AKIAIOSFODNN7EXAMPLE")
    .with_secret_access_key("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
    .with_bucket_name("my-bucket")
    .build()?;

// Option 3: IAM role (recommended for production)
// Set IAM role on EC2 instance/ECS task, use from_env()
```

### GCS (Google Cloud Storage)
```rust
use object_store::gcp::GoogleCloudStorageBuilder;

let store = GoogleCloudStorageBuilder::from_env()
    .with_bucket_name("my-bucket")
    .build()?;
```

### Azure Blob Storage
```rust
use object_store::azure::MicrosoftAzureBuilder;

let store = MicrosoftAzureBuilder::from_env()
    .with_container_name("my-container")
    .build()?;
```

---

## Monitoring & Operations

### Metrics to Track
```rust
pub struct CloudMetrics {
    s3_put_count: u64,
    s3_get_count: u64,
    s3_put_latency_ms: Histogram,
    s3_get_latency_ms: Histogram,
    s3_errors: u64,
    cache_hit_rate: f64,
    manifest_refresh_count: u64,
}
```

### CloudWatch Integration (AWS)
```rust
use aws_sdk_cloudwatch::Client as CloudWatchClient;

// Publish metrics
cw.put_metric_data()
    .namespace("seerdb")
    .metric_data(
        MetricDatum::builder()
            .metric_name("s3_get_latency_ms")
            .value(42.5)
            .unit(StandardUnit::Milliseconds)
            .build()
    )
    .send()
    .await?;
```

---

## Cost Analysis

### Pricing (AWS S3, us-east-1)

| Operation | Cost | Notes |
|-----------|------|-------|
| PUT (write) | $0.005 per 1000 | Flush to S3: expensive but infrequent |
| GET (read) | $0.0004 per 1000 | Cached locally usually, rare S3 reads |
| Storage | $0.023 per GB/month | SSTables archived, not accessed daily |
| Transfer | $0.02 per GB (egress) | Only for cold data access |

### Typical Cost (1M ops/day)
```
Writes (flush every 1GB memtable): 50 flushes/day
  Cost: 50 * $0.000005 = $0.00025/day = $7.50/month

Reads (mostly cached, 1% from S3): 10k S3 reads/day
  Cost: 10k * $0.0000004 = $0.004/day = $120/month

Storage (500GB of SSTables)
  Cost: 500 * $0.023 = $11.50/month

TOTAL: ~$140/month for 1M ops/day
```

### Optimization
- Tiered storage: Move old SSTables to Glacier ($0.004/GB/month)
- Compression: LZ4 already enabled (25-30% space savings)
- Batch operations: Use S3 batch operations for cleanup

---

## Error Handling & Resilience

### Network Failures
```rust
// Retry policy for transient failures
const MAX_RETRIES: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 100;

fn put_with_retry(storage: &dyn Storage, path: &str, data: &[u8]) -> Result<()> {
    for attempt in 0..MAX_RETRIES {
        match storage.put(path, data) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => {
                // Retry with exponential backoff
                thread::sleep(Duration::from_millis(RETRY_BACKOFF_MS * 2_u64.pow(attempt)));
            }
            Err(e) => return Err(e),
        }
    }
    Err("Max retries exceeded".into())
}
```

### Missing SSTables
```rust
// Handle case where S3 SSTable was deleted/corrupted
fn get_with_fallback(key: &[u8]) -> Result<Option<Bytes>> {
    // Try current SSTables
    match try_sst(key) {
        Ok(val) => return Ok(val),
        Err(NotFound) => {
            // Check manifest for previous version
            let prev_manifest = manifest_history.get_previous()?;
            // Try old SSTables
            load_from_manifest(prev_manifest, key)
        }
        Err(e) => Err(e),
    }
}
```

### Connection Pooling
```rust
// Reuse S3 connections (avoid connection overhead)
let store = AmazonS3Builder::from_env()
    .with_bucket_name("bucket")
    .build()?;

// S3 client handles pooling internally
// Concurrent requests automatically load-balanced
```

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_s3_put_get() {
    // Use LocalStack or moto (local S3 mock)
}

#[test]
fn test_hybrid_manifest() {
    // Verify manifest updates work
}

#[test]
fn test_multi_instance_safety() {
    // Two instances, one writes while other reads
}
```

### Integration Tests
```rust
// Real S3 (with test bucket)
#[tokio::test]
async fn test_s3_end_to_end() {
    let db = DB::open_s3(test_bucket).await?;
    db.put(b"key", b"value")?;
    db.flush()?;

    // Verify data persisted to S3
    let manifest = s3.get("manifest.json")?;
    assert!(manifest.levels[0].files.len() > 0);
}
```

### Chaos Tests
```rust
// Kill connections, test retry logic
// S3 timeouts, test degradation
// Manifest corruption, test recovery
```

---

## Migration Path: Local → Cloud

### Step 1: Backup Local Data
```bash
# Export data from local instance
seerdb export --db /path/to/db --output backup.sst
```

### Step 2: Create S3 Bucket
```bash
aws s3 mb s3://my-seerdb-prod
aws s3api put-bucket-versioning --bucket my-seerdb-prod --versioning-configuration Status=Enabled
```

### Step 3: Upload SSTables
```bash
# Upload SSTables to S3
seerdb import --db /path/to/db --s3 s3://my-seerdb-prod
```

### Step 4: Switch to Hybrid Mode
```rust
// Update config
let opts = DBOptions {
    storage: Arc::new(HybridStorage {
        local: LocalStorage::new("/tmp/seerdb"),
        remote: S3Storage::from_bucket("my-seerdb-prod"),
    }),
    ..Default::default()
};

let db = DB::open(opts)?;
```

### Step 5: Verification
```bash
# Verify data reads correctly
seerdb verify --db /path/to/db --s3 s3://my-seerdb-prod
```

---

## Performance Expectations

### Local-Only vs. Hybrid

| Metric | Local | Hybrid | Notes |
|--------|-------|--------|-------|
| Write latency | <1ms | <1ms | Memtable local (same) |
| Read (hit) | <100µs | <100µs | Block cache local (same) |
| Read (miss) | 1-5ms | 10-50ms | S3 much slower |
| Flush latency | 1-10ms | 100-500ms | S3 write slower |
| Storage cost | N/A | $140/month (1M ops) | Cloud overhead |

### Optimization Tips
1. **Increase memtable size**: Reduce flush frequency
2. **Increase block cache**: More local caching
3. **Use ALEX index**: Faster SSTable seeks
4. **Enable compression**: Reduce S3 bandwidth
5. **Batch operations**: Reduce S3 API calls

---

## Conclusion

**Recommended S3 Implementation for 0.0.3**:

1. ✅ Abstract Storage trait (already partially there)
2. ✅ S3Storage implementation using object_store crate
3. ✅ Hybrid local memtable + S3 SSTables
4. ✅ Manifest-based multi-instance coordination
5. ✅ Read-only replica mode
6. ✅ Standard auth (AWS IAM, GCS service account, Azure managed identity)

**Benefits**:
- ✅ Cloud-deployable (AWS Lambda, Fargate, GKE)
- ✅ Disaster-recoverable (SSTables persist across failures)
- ✅ Cost-efficient (~$140/month for 1M ops/day)
- ✅ Competitive with RocksDB (which doesn't support this)

**Timeline**: 3-4 weeks (0.0.3)

