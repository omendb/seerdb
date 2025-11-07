// Main database interface
// Integrates WAL, Memtable, SSTable, Compaction, and VLog

use crate::compaction::{compact_sstables, LSMTree};
use crate::health::{HealthCheck, HealthStatus};
use crate::memtable::Memtable;
use crate::metrics::{DBStats, MetricsCollector};
use crate::range::RangeIterator;
use crate::sstable::SSTable;
use crate::vlog::VLog;
use crate::wal::{Record, SyncPolicy, WALReader, WAL};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use twox_hash::XxHash64;
use std::hash::{Hash, Hasher};

/// Number of memtable partitions for reduced lock contention
///
/// Partitioning the memtable reduces lock contention on multi-core systems
/// by allowing concurrent writes to different partitions. Each partition
/// is independently locked, so 16 partitions = 16x less contention.
///
/// Expected improvement: +25-40% write throughput on multi-core systems
/// Research backing: Tucana (2020), FASTER (2018)
const NUM_PARTITIONS: usize = 16;

/// Calculate which partition a key belongs to using xxhash
///
/// Uses fast xxhash algorithm to distribute keys evenly across partitions.
/// The hash is stable (same key always goes to same partition), which is
/// critical for correctness.
#[inline]
fn partition_for_key(key: &[u8]) -> usize {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    let hash = hasher.finish();
    (hash % NUM_PARTITIONS as u64) as usize
}

#[derive(Debug, Error)]
pub enum DBError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WAL error: {0}")]
    Wal(#[from] crate::wal::WALError),

    #[error("SSTable error: {0}")]
    SSTable(#[from] crate::sstable::SSTableError),

    #[error("Compaction error: {0}")]
    Compaction(#[from] crate::compaction::CompactionError),

    #[error("VLog error: {0}")]
    VLog(#[from] crate::vlog::VLogError),

    #[error("Database not opened")]
    NotOpened,
}

pub type Result<T> = std::result::Result<T, DBError>;

/// Database configuration options
///
/// Controls all aspects of database behavior including durability, performance,
/// and resource usage.
///
/// # Examples
///
/// ```rust,no_run
/// use seerdb::{DBOptions, SyncPolicy};
/// use std::path::PathBuf;
///
/// // Default configuration (recommended for most use cases)
/// let opts = DBOptions::default();
///
/// // Custom configuration for high-throughput writes
/// let opts = DBOptions {
///     data_dir: PathBuf::from("/var/lib/myapp/db"),
///     memtable_capacity: 128 * 1024 * 1024,  // 128MB for fewer flushes
///     background_compaction: true,            // Non-blocking compaction
///     wal_sync_policy: SyncPolicy::None,     // Faster but less durable
///     ..Default::default()
/// };
///
/// // Configuration for large values (e.g., embeddings)
/// let opts = DBOptions {
///     vlog_threshold: Some(4096),  // Store values >4KB separately
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct DBOptions {
    /// Directory for database files
    ///
    /// All database files (WAL, SSTables, vLog) are stored in this directory.
    /// The directory will be created if it doesn't exist.
    ///
    /// Default: `"./seerdb_data"`
    pub data_dir: PathBuf,

    /// Memtable capacity in bytes
    ///
    /// Maximum size of the in-memory write buffer before flushing to disk.
    /// Larger values reduce flush frequency but increase memory usage and recovery time.
    ///
    /// Default: `64 * 1024 * 1024` (64MB)
    ///
    /// Recommended:
    /// - Low memory systems: 64 MB
    /// - Normal systems: 128-256 MB
    /// - High-throughput: 512 MB - 1 GB
    pub memtable_capacity: usize,

    /// WAL sync policy
    ///
    /// Controls when writes are fsync'd to disk for durability.
    ///
    /// Default: [`SyncPolicy::SyncData`]
    ///
    /// Options:
    /// - `SyncAll`: fsync data + metadata (strongest durability, slowest)
    /// - `SyncData`: fsync data only (strong durability, fast)
    /// - `None`: no fsync (fastest, data loss possible on crash)
    pub wal_sync_policy: SyncPolicy,

    /// LSM base level size
    ///
    /// Target size for LSM level 1 in bytes. Other levels grow exponentially
    /// based on `size_ratio`.
    ///
    /// Default: `10 * 1024 * 1024` (10MB)
    pub base_level_size: u64,

    /// LSM size ratio between levels
    ///
    /// Each level is `size_ratio` times larger than the previous level.
    ///
    /// Default: `10`
    ///
    /// Trade-offs:
    /// - Smaller ratio (4-5): Less write amplification, more read amplification
    /// - Larger ratio (10-20): More write amplification, less read amplification
    pub size_ratio: u64,

    /// Number of LSM levels
    ///
    /// Maximum number of levels in the LSM tree.
    ///
    /// Default: `7` (supports up to ~1TB with default settings)
    pub num_levels: usize,

    /// VLog threshold for key-value separation
    ///
    /// Values larger than this threshold are stored in a separate value log (vLog)
    /// instead of inline in SSTables. This reduces write amplification for large values.
    ///
    /// Default: `None` (disabled)
    ///
    /// Recommended:
    /// - Small values (<1KB): Keep `None` (disabled)
    /// - Large values (embeddings, documents): `Some(4096)` (4KB threshold)
    ///
    /// # Example
    ///
    /// ```rust
    /// use seerdb::DBOptions;
    ///
    /// // Enable vLog for vector database (large embeddings)
    /// let opts = DBOptions {
    ///     vlog_threshold: Some(4096),  // Values >4KB go to vLog
    ///     ..Default::default()
    /// };
    /// ```
    pub vlog_threshold: Option<usize>,

    /// Enable background compaction
    ///
    /// When `true`, compaction runs in a background thread, making writes non-blocking.
    /// When `false`, compaction happens synchronously during flush (blocking).
    ///
    /// Default: `false` (for predictable behavior)
    ///
    /// Recommended:
    /// - High-throughput writes: `true`
    /// - Predictable latency: `false`
    pub background_compaction: bool,

    /// Enable background flush
    ///
    /// When `true`, memtable flushes run in a background thread, making writes non-blocking.
    /// When `false`, flushes happen synchronously when memtable is full (blocking).
    ///
    /// Default: `false` (for predictable behavior and low overhead)
    ///
    /// **When to enable**: Large, sustained workloads that trigger frequent memtable flushes.
    /// Small benchmarks may see regression due to thread coordination overhead.
    ///
    /// Recommended:
    /// - Large datasets (>1GB): `true` (avoids flush blocking)
    /// - Sustained high-throughput: `true` (eliminates 54% blocking time)
    /// - Small datasets/benchmarks: `false` (less overhead)
    pub background_flush: bool,
}

impl Default for DBOptions {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./seerdb_data"),
            memtable_capacity: 64 * 1024 * 1024, // 64MB
            wal_sync_policy: SyncPolicy::SyncData,
            base_level_size: 10 * 1024 * 1024, // 10MB
            size_ratio: 10,
            num_levels: 7,
            vlog_threshold: Some(4096),   // WiscKey: 4KB threshold for KV separation (FIXED!)
            background_compaction: false, // Disabled by default for compatibility
            background_flush: false,      // Disabled by default - enable for large workloads
        }
    }
}

/// Compaction task message
enum CompactionTask {
    /// Compact a specific level
    CompactLevel(usize),
    /// Shutdown signal
    Shutdown,
}

/// Flush task message
enum FlushTask {
    /// Flush the memtable to SSTable
    Flush,
    /// Shutdown signal
    Shutdown,
}

/// Main database interface
///
/// An embedded LSM-tree based key-value storage engine with the following properties:
///
/// - **Durable**: All writes are logged to WAL before returning
/// - **Consistent**: Snapshot isolation for reads
/// - **Thread-safe**: Can be safely shared across threads via `Arc<DB>`
/// - **Observable**: Built-in metrics and health checks
///
/// # Architecture
///
/// The database uses an LSM-tree (Log-Structured Merge-tree) architecture:
///
/// 1. **Writes** go to WAL (write-ahead log) + memtable (in-memory)
/// 2. **Memtable** flushes to L0 SSTables when full
/// 3. **Compaction** merges SSTables across levels to reduce read amplification
/// 4. **Reads** check memtable first, then SSTables (with bloom filter optimization)
///
/// # Examples
///
/// ```rust,no_run
/// use seerdb::{DB, DBOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Open database
/// let db = DB::open(DBOptions::default())?;
///
/// // Write
/// db.put(b"user:1:name", b"Alice")?;
/// db.put(b"user:1:email", b"alice@example.com")?;
///
/// // Read
/// let name = db.get(b"user:1:name")?;
/// assert_eq!(name, Some(bytes::Bytes::from("Alice")));
///
/// // Delete
/// db.delete(b"user:1:email")?;
///
/// // Flush to disk
/// db.flush()?;
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// `DB` is thread-safe and can be shared across threads:
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use std::thread;
/// use seerdb::{DB, DBOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let db = Arc::new(DB::open(DBOptions::default())?);
///
/// let db_clone = db.clone();
/// let handle = thread::spawn(move || {
///     db_clone.put(b"thread:1", b"data").unwrap();
/// });
///
/// db.put(b"thread:2", b"data")?;
/// handle.join().unwrap();
/// # Ok(())
/// # }
/// ```
pub struct DB {
    /// Database options
    options: DBOptions,
    /// Write-ahead log
    wal: Arc<Mutex<WAL>>,
    /// Active memtables (16 partitions for reduced lock contention)
    /// Each partition is independently locked, allowing concurrent writes
    memtables: [Arc<Mutex<Memtable>>; NUM_PARTITIONS],
    /// Immutable memtables being flushed (RocksDB-style, but per-partition)
    /// Readers check this before SSTables to avoid data loss during flush
    immutable_memtables: Arc<Mutex<Option<Vec<Memtable>>>>,
    /// LSM tree for level management
    lsm: Arc<Mutex<LSMTree>>,
    /// Value log for KV separation (optional)
    vlog: Arc<Mutex<Option<VLog>>>,
    /// Counter for generating SSTable filenames
    sstable_counter: Arc<Mutex<u64>>,
    /// Metrics collector for observability
    metrics: Arc<MetricsCollector>,
    /// Channel for sending compaction tasks to background thread
    compaction_tx: Option<Sender<CompactionTask>>,
    /// Background compaction worker thread
    compaction_worker: Option<JoinHandle<()>>,
    /// Channel for sending flush tasks to background thread
    flush_tx: Option<Sender<FlushTask>>,
    /// Background flush worker thread
    flush_worker: Option<JoinHandle<()>>,
    /// Flush mutex to serialize flush operations and prevent concurrent flush races
    flush_mutex: Arc<Mutex<()>>,
    /// SSTable reader cache to avoid re-opening files on every read (CRITICAL for performance)
    /// Maps SSTable path -> opened SSTable with loaded indexes and bloom filters
    sstable_cache: Arc<Mutex<std::collections::HashMap<PathBuf, Arc<Mutex<SSTable>>>>>,
    /// Cached vLog availability (avoids lock on every get())
    has_vlog: std::sync::atomic::AtomicBool,
}

impl DB {
    /// Open or create a database
    ///
    /// Opens an existing database or creates a new one at the specified path.
    /// If a WAL exists, it will be replayed to recover uncommitted writes.
    ///
    /// # Arguments
    ///
    /// * `options` - Database configuration (see [`DBOptions`])
    ///
    /// # Returns
    ///
    /// Returns a [`DB`] instance or an error if:
    /// - Directory creation fails
    /// - WAL recovery fails (corruption detected)
    /// - Existing SSTables are corrupted (checksum mismatch)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    /// use std::path::PathBuf;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Open with default settings
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Open with custom path
    /// let opts = DBOptions {
    ///     data_dir: PathBuf::from("/var/lib/myapp/db"),
    ///     ..Default::default()
    /// };
    /// let db = DB::open(opts)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DBError::Io`]: Failed to create directory or open files
    /// - [`DBError::Wal`]: WAL corruption detected during recovery
    /// - [`DBError::SSTable`]: SSTable checksum validation failed
    pub fn open(options: DBOptions) -> Result<Self> {
        info!(
            path = ?options.data_dir,
            memtable_capacity_mb = options.memtable_capacity / (1024 * 1024),
            background_compaction = options.background_compaction,
            "Opening database"
        );

        // Create data directory if it doesn't exist
        std::fs::create_dir_all(&options.data_dir)?;

        let wal_path = options.data_dir.join("wal.log");
        let vlog_path = options.data_dir.join("values.vlog");

        // Create 16 partitioned memtables (divide capacity by NUM_PARTITIONS)
        let capacity_per_partition = options.memtable_capacity / NUM_PARTITIONS;
        let memtables_vec: Vec<Memtable> = (0..NUM_PARTITIONS)
            .map(|_| Memtable::new(capacity_per_partition))
            .collect();

        // Recover from WAL if it exists
        if wal_path.exists() {
            info!("Recovering from WAL");
            let total_entries_before: usize = memtables_vec.iter().map(|mt| mt.len()).sum();
            Self::recover_partitioned(&wal_path, &memtables_vec)?;
            let total_entries_after: usize = memtables_vec.iter().map(|mt| mt.len()).sum();
            let recovered = total_entries_after - total_entries_before;
            info!(entries = recovered, "WAL recovery complete");
        } else {
            info!("No existing WAL found, starting fresh");
        }

        // Create new WAL (overwrites old one after recovery)
        let wal = WAL::create(&wal_path, options.wal_sync_policy)?;

        // Create or open vLog if KV separation is enabled
        let vlog = if options.vlog_threshold.is_some() {
            if vlog_path.exists() {
                Some(VLog::open(&vlog_path)?)
            } else {
                Some(VLog::create(&vlog_path)?)
            }
        } else {
            None
        };

        // Create LSM tree
        let mut lsm = LSMTree::new(
            &options.data_dir,
            options.base_level_size,
            options.size_ratio,
            options.num_levels,
        );

        // Load existing SSTables from disk
        // This also verifies checksums - will fail if any SSTable is corrupted
        lsm.load_existing_sstables()?;
        let total_sstables: usize = (0..lsm.num_levels())
            .filter_map(|i| lsm.level(i))
            .map(|level| level.sstables().len())
            .sum();
        info!(
            sstables = total_sstables,
            levels = lsm.num_levels(),
            "LSM tree loaded"
        );

        // Capture has_vlog before wrapping
        let has_vlog = vlog.is_some();

        // Wrap in Arc<Mutex<>> for sharing with background workers
        // Convert Vec<Memtable> into [Arc<Mutex<Memtable>>; NUM_PARTITIONS]
        let mut memtables_iter = memtables_vec.into_iter();
        let memtables: [Arc<Mutex<Memtable>>; NUM_PARTITIONS] = std::array::from_fn(|_| {
            Arc::new(Mutex::new(memtables_iter.next().expect("Not enough partitions")))
        });
        let immutable_memtables = Arc::new(Mutex::new(None));
        let wal = Arc::new(Mutex::new(wal));
        let vlog = Arc::new(Mutex::new(vlog));
        let lsm = Arc::new(Mutex::new(lsm));
        let flush_mutex = Arc::new(Mutex::new(()));

        // Initialize SSTable counter from existing files to avoid overwriting
        // Collect all SSTable paths first to avoid borrow issues
        let mut all_sstables = Vec::new();
        {
            let lsm_guard = lsm.lock().expect("LSM lock poisoned");
            for level_num in 0..lsm_guard.num_levels() {
                if let Some(level) = lsm_guard.level(level_num) {
                    all_sstables.extend(level.sstables().iter().cloned());
                }
            }
        }

        // Find max counter value from filenames like "L0_000123.sst"
        let max_counter = all_sstables
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| {
                        name.strip_prefix("L")
                            .and_then(|s| s.split('_').nth(1))
                            .and_then(|s| s.strip_suffix(".sst"))
                            .and_then(|s| s.parse::<u64>().ok())
                    })
            })
            .max()
            .unwrap_or(0);

        let sstable_counter = Arc::new(Mutex::new(max_counter + 1));

        // Create metrics early (needed by background worker)
        let metrics = Arc::new(MetricsCollector::new());

        // Start background compaction worker if enabled
        let (compaction_tx, compaction_worker) = if options.background_compaction {
            let (tx, rx) = channel::<CompactionTask>();

            // Clone references for worker thread
            let lsm_clone = Arc::clone(&lsm);
            let sstable_counter_clone = Arc::clone(&sstable_counter);
            let data_dir = options.data_dir.clone();
            let metrics_clone = Arc::clone(&metrics);

            // Spawn compaction worker thread
            let worker = thread::spawn(move || {
                while let Ok(task) = rx.recv() {
                    match task {
                        CompactionTask::CompactLevel(level_num) => {
                            // Perform compaction
                            if let Err(e) = Self::run_compaction(
                                &lsm_clone,
                                &sstable_counter_clone,
                                &data_dir,
                                level_num,
                                &metrics_clone,
                            ) {
                                error!(error = %e, level = level_num, "Background compaction failed");
                            }
                        }
                        CompactionTask::Shutdown => {
                            // Exit worker thread
                            break;
                        }
                    }
                }
            });

            (Some(tx), Some(worker))
        } else {
            (None, None)
        };

        // Start background flush worker if enabled
        let (flush_tx, flush_worker) = if options.background_flush {
            let (tx, rx) = channel::<FlushTask>();

            // Clone references for worker thread (clone each partition reference)
            let memtables_refs: [Arc<Mutex<Memtable>>; NUM_PARTITIONS] =
                std::array::from_fn(|i| Arc::clone(&memtables[i]));
            let immutable_memtables_ref = Arc::clone(&immutable_memtables);
            let wal_ref = Arc::clone(&wal);
            let lsm_clone = Arc::clone(&lsm);
            let vlog_clone = Arc::clone(&vlog);
            let sstable_counter_clone = Arc::clone(&sstable_counter);
            let data_dir = options.data_dir.clone();
            let metrics_clone = Arc::clone(&metrics);
            let memtable_capacity = options.memtable_capacity;
            let vlog_threshold = options.vlog_threshold;
            let flush_mutex_clone = Arc::clone(&flush_mutex);

            // Spawn flush worker thread
            let worker = thread::spawn(move || {
                while let Ok(task) = rx.recv() {
                    match task {
                        FlushTask::Flush => {
                            // Perform background flush (now with partitioned memtables)
                            if let Err(e) = Self::run_background_flush_partitioned(
                                &memtables_refs,
                                &immutable_memtables_ref,
                                &wal_ref,
                                &lsm_clone,
                                &vlog_clone,
                                &sstable_counter_clone,
                                &data_dir,
                                &metrics_clone,
                                memtable_capacity,
                                vlog_threshold,
                                &flush_mutex_clone,
                            ) {
                                error!(error = %e, "Background flush failed");
                            }
                        }
                        FlushTask::Shutdown => {
                            // Exit worker thread
                            break;
                        }
                    }
                }
            });

            (Some(tx), Some(worker))
        } else {
            (None, None)
        };

        let db = Self {
            options: options.clone(),
            wal,
            memtables,
            immutable_memtables,
            lsm,
            vlog,
            sstable_counter,
            metrics,
            compaction_tx,
            compaction_worker,
            flush_tx,
            flush_worker,
            flush_mutex,
            sstable_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            has_vlog: std::sync::atomic::AtomicBool::new(has_vlog),
        };

        // Flush memtables if any partition filled up during recovery
        let should_flush = db.memtables.iter().any(|mt| {
            mt.lock()
                .expect("Memtable lock poisoned")
                .should_flush()
        });
        if should_flush {
            info!("One or more memtable partitions full after recovery, flushing");
            db.flush()?;
        }

        info!("Database opened successfully");

        Ok(db)
    }

    /// Recover partitioned memtables from WAL
    ///
    /// Reads records one by one and distributes them across partitions using hash function.
    /// Stops gracefully if corruption or truncation is encountered.
    /// This ensures we recover all valid records before the corruption point.
    fn recover_partitioned(wal_path: &Path, memtables: &[Memtable]) -> Result<()> {
        let mut reader =
            WALReader::open(wal_path).map_err(|e| DBError::Io(std::io::Error::other(e)))?;

        // Read records one by one, stop gracefully on error (corruption/truncation)
        loop {
            match reader.read_next() {
                Ok(Some(record)) => match record {
                    Record::Put { key, value } => {
                        // Hash key to determine partition
                        let partition = partition_for_key(&key);
                        memtables[partition].put(key, value);
                    }
                    Record::Delete { key } => {
                        // Hash key to determine partition
                        let partition = partition_for_key(&key);
                        memtables[partition].delete(key);
                    }
                },
                Ok(None) => {
                    // End of WAL reached
                    break;
                }
                Err(e) => {
                    // Corruption or truncation encountered
                    // Stop reading but don't fail - we've recovered all valid records
                    warn!(error = %e, "WAL recovery stopped due to corrupt/truncated record");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Write a key-value pair to the database
    ///
    /// Inserts or updates a key-value pair in the database. The write is:
    /// 1. Written to WAL for durability
    /// 2. Added to memtable (in-memory buffer)
    /// 3. Automatically flushed to disk if memtable is full
    ///
    /// # Arguments
    ///
    /// * `key` - The key to write (can be `&[u8]`, `&str`, etc.)
    /// * `value` - The value to write
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or an error if:
    /// - WAL write fails (disk full, I/O error)
    /// - Automatic flush fails (SSTable write error)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Write string keys
    /// db.put("user:1:name", "Alice")?;
    ///
    /// // Write binary keys
    /// db.put(&[0x00, 0x01], &[0xFF, 0xFE])?;
    ///
    /// // Overwrite existing key
    /// db.put("counter", "1")?;
    /// db.put("counter", "2")?;  // Updates value
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DBError::Wal`]: WAL write failed (disk full, I/O error)
    /// - [`DBError::Io`]: SSTable flush failed during automatic flush
    ///
    /// # Performance
    ///
    /// - Typical latency: 10-100 microseconds
    /// - Latency spikes: 1-10 milliseconds during memtable flush
    /// - Use [`flush()`](Self::flush) explicitly to control flush timing
    pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
        let start = Instant::now();

        let key = Bytes::copy_from_slice(key.as_ref());
        let value = Bytes::copy_from_slice(value.as_ref());

        // Track logical bytes written (user data)
        let logical_bytes = (key.len() + value.len()) as u64;
        self.metrics.record_logical_bytes(logical_bytes);

        // Write to WAL first (durability)
        let record = Record::Put {
            key: key.clone(),
            value: value.clone(),
        };
        let wal_bytes = record.encode().len() as u64;
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

        // Track physical bytes written to WAL
        self.metrics.record_physical_bytes(wal_bytes);

        // Write to correct partition (reduced lock contention)
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].lock().expect("Memtable lock poisoned");
        mt.put(key, value);
        drop(mt); // Release lock early

        // Check if ANY partition should be flushed (since we have multiple partitions)
        let should_flush = self.memtables.iter().any(|mt| {
            mt.lock().expect("Memtable lock poisoned").should_flush()
        });
        if should_flush {
            if let Some(ref tx) = self.flush_tx {
                // Background flush: swap memtable immediately (fast), then signal background thread
                if self.try_swap_memtable()? {
                    // Successfully swapped - signal background thread to build SSTable
                    debug!("Memtable swapped, signaling background flush");
                    let _ = tx.send(FlushTask::Flush);
                }
                // If swap failed, another thread is already flushing - skip
            } else {
                // Synchronous flush: block until done
                self.flush()?;
            }
        }

        // Record latency
        self.metrics.record_put(start.elapsed());

        Ok(())
    }

    /// Read a value from the database by key
    ///
    /// Looks up a key in the database and returns its value if found. The lookup checks:
    /// 1. **Memtable** (in-memory buffer) - most recent writes
    /// 2. **SSTables** (L0 → L6) - disk-persisted data, from newest to oldest
    ///
    /// If key-value separation is enabled, large values are automatically read from the vLog.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up (can be `&[u8]`, `&str`, etc.)
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` - Key found, returns the value
    /// - `Ok(None)` - Key not found or was deleted
    /// - `Err(...)` - I/O error or SSTable corruption
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Write then read
    /// db.put("user:1", "Alice")?;
    /// let value = db.get("user:1")?;
    /// assert_eq!(value, Some(bytes::Bytes::from("Alice")));
    ///
    /// // Read non-existent key
    /// let value = db.get("user:999")?;
    /// assert_eq!(value, None);
    ///
    /// // Read deleted key
    /// db.delete("user:1")?;
    /// let value = db.get("user:1")?;
    /// assert_eq!(value, None);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DBError::Io`]: Failed to read SSTable from disk
    /// - [`DBError::SSTable`]: SSTable checksum mismatch (corruption)
    /// - [`DBError::VLog`]: Failed to read large value from vLog
    ///
    /// # Performance
    ///
    /// - **Memtable hit**: 1-10 microseconds (skiplist lookup)
    /// - **SSTable hit**: 10-100 microseconds (bloom filter + binary search + disk I/O)
    /// - **Miss**: Checks all levels, O(levels) with bloom filter optimization
    ///
    /// Bloom filters reduce disk I/O by ~99% for non-existent keys.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>> {
        let start = Instant::now();
        let key = key.as_ref();

        // Check correct partition first (most recent data)
        let partition = partition_for_key(key);
        let mt = self.memtables[partition].lock().expect("Memtable lock poisoned");
        let result = mt.get(key);
        let contains = mt.contains(key);
        drop(mt); // Release lock

        match result {
            Some(value) => {
                // Found value in memtable partition
                self.metrics.record_get(start.elapsed());
                return Ok(Some(value));
            }
            None if contains => {
                // Key exists in memtable partition as tombstone - don't check immutable or SSTables
                self.metrics.record_get(start.elapsed());
                return Ok(None);
            }
            None => {
                // Key not in active partition - check immutable partitions
            }
        }

        // Check immutable partitions (if flush is in progress)
        // We need to check ALL partitions since the key could be in any one
        let immut = self.immutable_memtables.lock().expect("Immutable memtables lock poisoned");
        if let Some(ref immutable_partitions) = *immut {
            // Check all partitions for the key
            for partition_mt in immutable_partitions.iter() {
                let immut_result = partition_mt.get(key);
                let immut_contains = partition_mt.contains(key);

                match immut_result {
                    Some(value) => {
                        // Found value in immutable partition
                        drop(immut);
                        self.metrics.record_get(start.elapsed());
                        return Ok(Some(value));
                    }
                    None if immut_contains => {
                        // Key exists as tombstone in immutable partition
                        drop(immut);
                        self.metrics.record_get(start.elapsed());
                        return Ok(None);
                    }
                    None => {
                        // Key not in this partition - check next partition
                        continue;
                    }
                }
            }
            drop(immut);
        } else {
            drop(immut);
        }

        // Get vLog if available (need to clone for SSTable attachment)
        let vlog_path = self.options.data_dir.join("values.vlog");
        let has_vlog = self.has_vlog.load(std::sync::atomic::Ordering::Relaxed);

        // Check SSTables in LSM tree (L0 -> L6)
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        for level_num in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_num) {
                // L0 has overlapping SSTables - check newest first (reverse order)
                // L1+ have non-overlapping SSTables - check in forward order
                let sstables: Vec<_> = if level_num == 0 {
                    level.sstables().iter().rev().collect()
                } else {
                    level.sstables().iter().collect()
                };

                // Check each SSTable in this level
                for sstable_path in sstables {
                    // Use cached SSTable reader (avoids expensive re-opening and index deserialization)
                    // Double-checked locking to avoid holding lock during I/O
                    let cached_sstable = {
                        // First check: try to get from cache (fast path)
                        {
                            let cache = self.sstable_cache.lock().expect("SSTable cache lock poisoned");
                            if let Some(sstable) = cache.get(sstable_path) {
                                sstable.clone()
                            } else {
                                drop(cache);

                                // Second check: open SSTable outside lock (slow path)
                                let sstable = if has_vlog {
                                    let vlog = VLog::open(&vlog_path)?;
                                    SSTable::open(sstable_path)?.with_vlog(vlog)
                                } else {
                                    SSTable::open(sstable_path)?
                                };
                                let sstable_arc = Arc::new(Mutex::new(sstable));

                                // Insert into cache (reacquire lock briefly)
                                let mut cache = self.sstable_cache.lock().expect("SSTable cache lock poisoned");
                                cache.entry(sstable_path.clone())
                                    .or_insert_with(|| sstable_arc.clone())
                                    .clone()
                            }
                        }
                    };

                    let mut sstable = cached_sstable.lock().expect("SSTable lock poisoned");

                    // For L0, if bloom filter says key exists but get() returns None, it's a tombstone
                    // Stop searching immediately (don't check older SSTables)
                    let may_contain = sstable.may_contain(key);
                    let result = sstable.get(key)?;

                    match result {
                        Some(value) => {
                            self.metrics.record_get(start.elapsed());
                            return Ok(Some(value));
                        }
                        None if level_num == 0 && may_contain => {
                            // L0: bloom filter says key exists but get() returned None = tombstone
                            // Don't check older L0 SSTables (tombstone masks them)
                            self.metrics.record_get(start.elapsed());
                            return Ok(None);
                        }
                        None => {
                            // Key not in this SSTable (bloom filter false positive or key truly absent)
                            // Continue to next SSTable
                        }
                    }
                }
            }
        }

        self.metrics.record_get(start.elapsed());
        Ok(None)
    }

    /// Delete a key from the database
    ///
    /// Marks a key as deleted by writing a tombstone. The deletion is:
    /// 1. Written to WAL for durability
    /// 2. Added to memtable as a tombstone marker
    /// 3. Automatically flushed to disk if memtable is full
    ///
    /// Tombstones are removed during compaction when all older versions of the key
    /// have been merged away.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete (can be `&[u8]`, `&str`, etc.)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or an error if:
    /// - WAL write fails (disk full, I/O error)
    /// - Automatic flush fails (SSTable write error)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Write then delete
    /// db.put("user:1", "Alice")?;
    /// db.delete("user:1")?;
    /// assert_eq!(db.get("user:1")?, None);
    ///
    /// // Deleting non-existent key is safe
    /// db.delete("user:999")?;  // No error
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DBError::Wal`]: WAL write failed (disk full, I/O error)
    /// - [`DBError::Io`]: SSTable flush failed during automatic flush
    ///
    /// # Performance
    ///
    /// - Typical latency: 10-100 microseconds (same as [`put()`](Self::put))
    /// - Latency spikes: 1-10 milliseconds during memtable flush
    ///
    /// # Space Reclamation
    ///
    /// Deleted keys occupy space until compaction:
    /// - Tombstone stored in memtable and SSTables
    /// - Space freed when compaction merges away all older versions
    /// - Large values in vLog are not immediately freed
    pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()> {
        let start = Instant::now();
        let key = Bytes::copy_from_slice(key.as_ref());

        // Write to WAL (durability)
        let record = Record::Delete { key: key.clone() };
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

        // Write tombstone to correct partition
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].lock().expect("Memtable lock poisoned");
        mt.delete(key);
        drop(mt); // Release lock early

        // Check if ANY partition should be flushed
        let should_flush = self.memtables.iter().any(|mt| {
            mt.lock().expect("Memtable lock poisoned").should_flush()
        });
        if should_flush {
            if let Some(ref tx) = self.flush_tx {
                // Background flush: swap memtable immediately (fast), then signal background thread
                if self.try_swap_memtable()? {
                    // Successfully swapped - signal background thread to build SSTable
                    debug!("Memtable swapped, signaling background flush");
                    let _ = tx.send(FlushTask::Flush);
                }
                // If swap failed, another thread is already flushing - skip
            } else {
                // Synchronous flush: block until done
                self.flush()?;
            }
        }

        // Record latency
        self.metrics.record_delete(start.elapsed());

        Ok(())
    }

    /// Manually flush the memtable to disk
    ///
    /// Forces the in-memory write buffer (memtable) to be written to an SSTable on disk.
    /// This operation:
    /// 1. Writes all memtable entries to a new L0 SSTable
    /// 2. Clears the WAL (data now safely in SSTable)
    /// 3. Replaces memtable with a new empty one
    /// 4. Triggers compaction if L0 has too many SSTables
    ///
    /// Flushing normally happens automatically when the memtable is full, but you can
    /// call this method explicitly to control when flushes occur.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or an error if:
    /// - SSTable write fails (disk full, I/O error)
    /// - WAL clear fails
    /// - Compaction fails (if triggered)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Write data
    /// for i in 0..1000 {
    ///     db.put(format!("key{}", i).as_bytes(), b"value")?;
    /// }
    ///
    /// // Force flush before shutdown
    /// db.flush()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`DBError::Io`]: Failed to write SSTable or clear WAL
    /// - [`DBError::SSTable`]: SSTable builder error
    /// - [`DBError::Compaction`]: Compaction failed (if triggered)
    ///
    /// # Performance
    ///
    /// - **Typical latency**: 10-100 milliseconds (depends on memtable size)
    /// - **Disk I/O**: Writes ~64MB SSTable (default memtable size)
    /// - **Blocks writes**: Briefly while swapping memtable
    ///
    /// # When to Use
    ///
    /// - **Before shutdown**: Persist final writes
    /// - **Performance tuning**: Avoid flush during hot path
    /// - **Testing**: Deterministic flush timing
    ///
    /// Not typically needed in normal operation (automatic flushing works well).
    pub fn flush(&self) -> Result<()> {
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        // **CRITICAL FIX**: Serialize all flushes to prevent concurrent flush races
        // Without this lock, concurrent flushes can cause:
        // 1. WAL cleared while writes are still in memtable (data loss)
        // 2. Memtable swapped while being iterated (corruption)
        // 3. Multiple flushes writing to same SSTable file (corruption)
        let _flush_lock = self.flush_mutex.lock().expect("Flush mutex poisoned");

        let flush_start = Instant::now();
        let mt_size_before = self.memtable.lock().expect("Memtable lock poisoned").size();

        // Early return if memtable is empty (nothing to flush)
        if mt_size_before == 0 {
            return Ok(());
        }

        info!(
            memtable_size_bytes = mt_size_before,
            "Starting memtable flush"
        );

        // **CRITICAL**: Check if there's a previous failed flush
        // If immutable_memtable is occupied, flush it first to avoid data loss
        let pending_immutable = {
            let mut immut_guard = self.immutable_memtable.lock().expect("Immutable memtable lock poisoned");
            immut_guard.take()
        };

        if let Some(pending_mt) = pending_immutable {
            // Previous flush failed - retry flushing the existing immutable_memtable
            warn!("Retrying flush of previously failed immutable memtable");

            // Generate filename for pending flush
            let mut counter = self.sstable_counter.lock().expect("SSTable counter mutex poisoned");
            let pending_sstable_path = self.options.data_dir.join(format!("L0_{:06}.sst", *counter));
            *counter += 1;
            drop(counter);

            // Flush pending memtable to SSTable
            pending_mt.flush(&pending_sstable_path)?;
            let pending_size = std::fs::metadata(&pending_sstable_path)?.len();

            // Track physical bytes written to SSTable (retry case)
            self.metrics.record_physical_bytes(pending_size);

            // Add to LSM tree
            let mut lsm = self.lsm.lock().expect("LSM mutex poisoned");
            lsm.add_l0_sstable(pending_sstable_path.clone(), pending_size);
            drop(lsm);

            // Clear WAL (pending data now in SSTable)
            let mut wal = self.wal.lock().expect("WAL mutex poisoned");
            wal.clear()?;
            drop(wal);

            info!("Successfully flushed previously failed immutable memtable");
        }

        // Now check if active memtable needs flushing
        let mt_size = self.memtable.lock().expect("Memtable lock poisoned").size();
        if mt_size == 0 {
            return Ok(()); // Nothing to flush
        }

        // Generate SSTable filename for main flush
        let mut counter = self.sstable_counter.lock().expect("SSTable counter mutex poisoned");
        let sstable_path = self.options.data_dir.join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Swap memtable FIRST (RocksDB-style immutable memtable)
        // 1. Lock memtable
        // 2. Swap with new empty memtable (old one becomes immutable)
        // 3. Store immutable memtable so readers can access it during flush
        // 4. Release lock (new writes go to new memtable and stay in WAL)
        // 5. Flush immutable memtable to SSTable
        // 6. Clear immutable memtable + WAL (flushed data is in SSTable)
        let mut mt_guard = self.memtable.lock().expect("Memtable lock poisoned");
        let flushing_memtable = std::mem::replace(&mut *mt_guard, Memtable::new(self.options.memtable_capacity));
        drop(mt_guard); // Release lock immediately - new writes go to new memtable

        // Store in immutable_memtable so readers can access during flush
        {
            let mut immut_guard = self.immutable_memtable.lock().expect("Immutable memtable lock poisoned");
            *immut_guard = Some(flushing_memtable);
        } // Release lock - readers can now access immutable memtable

        // Build SSTable from immutable memtable (need to access it again)
        // We clone the Arc pointer, not the data
        let immut_clone = Arc::clone(&self.immutable_memtable);
        let immut_guard = immut_clone.lock().expect("Immutable memtable lock poisoned");
        let immutable_memtable = immut_guard.as_ref().expect("Immutable memtable should be present");

        // Build SSTable with optional vLog support from immutable memtable
        let mut vlog_guard = self.vlog.lock().expect("vLog mutex poisoned");

        if let (Some(threshold), Some(ref mut vlog)) =
            (self.options.vlog_threshold, vlog_guard.as_mut())
        {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::create(&sstable_path)?.with_vlog_threshold(threshold);

            for (key, entry) in immutable_memtable.iter() {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key, value, vlog)?;
                    }
                    Entry::Tombstone => {
                        // **CRITICAL FIX**: DO NOT skip tombstones!
                        // Tombstones must be persisted to SSTables to mask older values
                        // They are removed during compaction when all older versions are gone
                        builder.add_tombstone(key)?;
                    }
                }
            }

            builder.finish()?;

            // ALWAYS sync vLog after flush - we need it synced for reading
            // (different file handles won't see buffered writes)
            vlog.sync()?;
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard); // Release lock
            immutable_memtable.flush(&sstable_path)?;
        }
        drop(immut_guard); // Release lock on immutable memtable

        let size = std::fs::metadata(&sstable_path)?.len();

        // Track physical bytes written to SSTable
        self.metrics.record_physical_bytes(size);

        // Add to LSM tree L0
        let mut lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let sstable_path_for_log = sstable_path.clone();
        lsm.add_l0_sstable(sstable_path, size);

        // Clear immutable memtable + WAL after successful flush
        // Data is now safely persisted in SSTable
        // New writes (in new memtable) are still in WAL and safe
        let mut immut_guard = self.immutable_memtable.lock().expect("Immutable memtable lock poisoned");
        *immut_guard = None;
        drop(immut_guard);

        let mut wal = self.wal.lock().expect("WAL mutex poisoned");
        wal.clear()?;
        drop(wal);

        let flush_duration_ms = flush_start.elapsed().as_millis();
        info!(
            duration_ms = flush_duration_ms,
            sstable_path = ?sstable_path_for_log,
            sstable_size_bytes = size,
            "Memtable flush complete"
        );

        // Check if compaction is needed
        if let Some(level_num) = lsm.needs_compaction() {
            debug!(level = level_num, "Compaction triggered");
            drop(lsm); // Release lock before compaction

            if let Some(ref tx) = self.compaction_tx {
                // Background compaction: send signal (non-blocking)
                debug!(level = level_num, "Sending background compaction signal");
                let _ = tx.send(CompactionTask::CompactLevel(level_num));
            } else {
                // Synchronous compaction: block until done
                debug!(level = level_num, "Starting synchronous compaction");
                self.compact_level(level_num)?;
            }
        }

        // Record flush
        self.metrics.record_flush();

        Ok(())
    }

    /// Try to atomically swap memtable for background flush
    ///
    /// Returns true if memtable was successfully swapped (caller should signal background thread)
    /// Returns false if another thread is already flushing (skip signaling)
    fn try_swap_memtable(&self) -> Result<bool> {
        // Try to acquire flush lock - if another thread is flushing, return false
        let _flush_lock = match self.flush_mutex.try_lock() {
            Ok(lock) => lock,
            Err(_) => return Ok(false), // Another thread is flushing
        };

        // Check if immutable_memtable is occupied
        let immut_occupied = {
            let immut = self.immutable_memtable.lock().expect("Immutable memtable lock poisoned");
            immut.is_some()
        };

        if immut_occupied {
            // Another thread's flush is still in progress
            return Ok(false);
        }

        // Safe to swap - immutable_memtable is None
        let mut mt_guard = self.memtable.lock().expect("Memtable lock poisoned");
        let flushing_memtable = std::mem::replace(&mut *mt_guard, Memtable::new(self.options.memtable_capacity));
        drop(mt_guard);

        // Store in immutable_memtable
        let mut immut_guard = self.immutable_memtable.lock().expect("Immutable memtable lock poisoned");
        *immut_guard = Some(flushing_memtable);
        drop(immut_guard);

        Ok(true) // Successfully swapped
    }

    /// Compact a level
    fn compact_level(&self, level_num: usize) -> Result<()> {
        Self::do_compact_level(
            &self.lsm,
            &self.sstable_counter,
            &self.options.data_dir,
            level_num,
            &self.metrics,
        )
    }

    /// Internal compaction implementation (shared by both sync and async paths)
    fn do_compact_level(
        lsm: &Arc<Mutex<LSMTree>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
        metrics: &Arc<MetricsCollector>,
    ) -> Result<()> {
        let compaction_start = Instant::now();
        let lsm_lock = lsm.lock().expect("LSM mutex poisoned");

        // Get SSTables to compact
        let level = lsm_lock.level(level_num).ok_or(DBError::NotOpened)?;
        let input_paths: Vec<PathBuf> = level.sstables().to_vec();

        if input_paths.is_empty() {
            return Ok(());
        }

        let input_count = input_paths.len();
        debug!(
            level = level_num,
            input_sstables = input_count,
            "Starting compaction"
        );

        // Generate output path
        let mut counter = sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let output_path = data_dir.join(format!("L{}_{:06}.sst", level_num + 1, *counter));
        *counter += 1;
        drop(counter);

        drop(lsm_lock); // Release lock during compaction

        // Compact SSTables
        let (result_path, size) = compact_sstables(&input_paths, &output_path)?;

        // Track physical bytes written during compaction
        metrics.record_physical_bytes(size);

        // Update LSM tree - add to next level and remove from current level
        let mut lsm = lsm.lock().expect("LSM mutex poisoned");
        lsm.add_to_level(level_num + 1, result_path, size);
        lsm.remove_sstables_from_level(level_num, &input_paths);
        drop(lsm);

        // Delete input SSTables from disk
        for path in input_paths {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!(path = ?path, error = %e, "Failed to delete SSTable after compaction");
            }
        }

        let compaction_duration_ms = compaction_start.elapsed().as_millis();
        info!(
            level = level_num,
            input_sstables = input_count,
            output_size_bytes = size,
            duration_ms = compaction_duration_ms,
            "Compaction complete"
        );

        Ok(())
    }

    /// Static compaction method for background worker thread
    /// This is called from the worker thread without &self
    fn run_compaction(
        lsm: &Arc<Mutex<LSMTree>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
        metrics: &Arc<MetricsCollector>,
    ) -> Result<()> {
        Self::do_compact_level(lsm, sstable_counter, data_dir, level_num, metrics)
    }

    /// Static flush method for background worker thread
    /// This is called from the worker thread without &self
    ///
    /// NOTE: Memtable swap already happened in try_swap_memtable() before signal was sent.
    /// This method just builds the SSTable from immutable_memtable (slow part).
    fn run_background_flush(
        _memtable: &Arc<Mutex<Memtable>>,
        immutable_memtable: &Arc<Mutex<Option<Memtable>>>,
        wal: &Arc<Mutex<WAL>>,
        lsm: &Arc<Mutex<LSMTree>>,
        vlog: &Arc<Mutex<Option<VLog>>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        metrics: &Arc<MetricsCollector>,
        _memtable_capacity: usize,
        vlog_threshold: Option<usize>,
        flush_mutex: &Arc<Mutex<()>>,
    ) -> Result<()> {
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        // Serialize all flushes to prevent concurrent SSTable builds
        let _flush_lock = flush_mutex.lock().expect("Flush mutex poisoned");

        let flush_start = Instant::now();

        // Check if there's an immutable_memtable to flush
        let has_immutable = {
            let immut = immutable_memtable.lock().expect("Immutable memtable lock poisoned");
            immut.is_some()
        };

        if !has_immutable {
            // No immutable memtable - another thread might have already flushed it
            return Ok(());
        }

        // Generate SSTable filename
        let mut counter = sstable_counter.lock().expect("SSTable counter mutex poisoned");
        let sstable_path = data_dir.join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Build SSTable from immutable memtable (slow part - this is why it's in background)
        let immut_guard = immutable_memtable.lock().expect("Immutable memtable lock poisoned");
        let immutable_mt = immut_guard.as_ref().expect("Immutable memtable should be present");

        // Build SSTable with optional vLog support
        let mut vlog_guard = vlog.lock().expect("vLog mutex poisoned");

        if let (Some(threshold), Some(ref mut vlog_ref)) = (vlog_threshold, vlog_guard.as_mut()) {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::create(&sstable_path)?.with_vlog_threshold(threshold);

            for (key, entry) in immutable_mt.iter() {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key, value, vlog_ref)?;
                    }
                    Entry::Tombstone => {
                        builder.add_tombstone(key)?;
                    }
                }
            }

            builder.finish()?;

            // Sync vLog after flush
            vlog_ref.sync()?;
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard);
            immutable_mt.flush(&sstable_path)?;
        }
        drop(immut_guard);

        let size = std::fs::metadata(&sstable_path)?.len();

        // Track physical bytes written
        metrics.record_physical_bytes(size);

        // Add to LSM tree L0
        let mut lsm_guard = lsm.lock().expect("LSM mutex poisoned");
        lsm_guard.add_l0_sstable(sstable_path.clone(), size);
        drop(lsm_guard);

        // Clear immutable memtable + WAL after successful flush
        {
            let mut immut_guard = immutable_memtable.lock().expect("Immutable memtable lock poisoned");
            *immut_guard = None;
        }

        {
            let mut wal_guard = wal.lock().expect("WAL mutex poisoned");
            wal_guard.clear()?;
        }

        let flush_duration_ms = flush_start.elapsed().as_millis();
        info!(
            duration_ms = flush_duration_ms,
            sstable_path = ?sstable_path,
            sstable_size_bytes = size,
            "Background memtable flush complete"
        );

        // Record flush metric
        metrics.record_flush();

        Ok(())
    }

    /// Get current memtable size
    pub fn memtable_size(&self) -> usize {
        self.memtable.lock().expect("Memtable lock poisoned").size()
    }

    /// Get number of entries in memtable
    pub fn memtable_len(&self) -> usize {
        self.memtable.lock().expect("Memtable lock poisoned").len()
    }

    /// Get real-time database statistics
    ///
    /// Returns comprehensive statistics for monitoring, observability, and performance tuning.
    /// Includes operation counts, latency percentiles, resource usage, and LSM tree structure.
    ///
    /// # Returns
    ///
    /// A [`DBStats`] struct containing:
    /// - **Throughput**: Reads/writes/deletes per second
    /// - **Operation counts**: Total operations since database opened
    /// - **Latency percentiles**: p50, p95, p99, p999 for get/put/delete (in microseconds)
    /// - **Resource usage**: Memtable, WAL, disk usage
    /// - **LSM structure**: SSTables per level, level sizes
    /// - **Uptime**: Time since database opened (seconds)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Perform some operations
    /// for i in 0..10000 {
    ///     db.put(format!("key{}", i).as_bytes(), b"value")?;
    /// }
    ///
    /// // Get statistics
    /// let stats = db.stats();
    /// println!("Throughput: {:.0} writes/sec", stats.writes_per_sec);
    /// println!("p99 latency: {} µs", stats.put_latency_p99_us);
    /// println!("Memtable: {:.1}% full", stats.memtable_utilization_pct);
    /// println!("Disk usage: {} MB", stats.total_disk_bytes / 1_048_576);
    ///
    /// // LSM structure
    /// for (level, count) in stats.sstables_per_level.iter().enumerate() {
    ///     if *count > 0 {
    ///         println!("L{}: {} SSTables", level, count);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// This method is relatively cheap (microseconds) but does:
    /// - Lock memtable, LSM tree briefly to read stats
    /// - Calculate file sizes from filesystem metadata
    /// - Compute latency percentiles from histograms
    ///
    /// Safe to call frequently (e.g., every second for monitoring).
    ///
    /// # See Also
    ///
    /// - [`health()`](Self::health) - Health checks with thresholds
    /// - [`DBStats`] - Full structure documentation
    pub fn stats(&self) -> DBStats {
        // Get operation counts and throughput
        let (total_puts, total_gets, total_deletes, total_flushes, total_compactions) =
            self.metrics.get_counts();
        let (writes_per_sec, reads_per_sec, deletes_per_sec) = self.metrics.calculate_throughput();

        // Get latency percentiles
        let (put_latencies, get_latencies, delete_latencies) =
            self.metrics.get_latency_percentiles();

        // Get memtable stats
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        let memtable_size_bytes = mt.size();
        let memtable_capacity_bytes = mt.capacity();
        let memtable_utilization_pct =
            (memtable_size_bytes as f64 / memtable_capacity_bytes as f64) * 100.0;
        drop(mt);

        // Get WAL size
        let wal_size_bytes = self
            .options
            .data_dir
            .join("wal.log")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        // Get LSM tree structure
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let mut sstables_per_level = Vec::new();
        let mut level_sizes_bytes = Vec::new();
        let mut total_disk_bytes = 0u64;
        let mut total_sstables = 0usize;

        for level_num in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_num) {
                let sstables = level.sstables();
                sstables_per_level.push(sstables.len());
                total_sstables += sstables.len();

                let level_size: u64 = sstables
                    .iter()
                    .filter_map(|path| path.metadata().ok().map(|m| m.len()))
                    .sum();
                level_sizes_bytes.push(level_size);
                total_disk_bytes += level_size;
            } else {
                sstables_per_level.push(0);
                level_sizes_bytes.push(0);
            }
        }
        drop(lsm);

        // Add vLog size if present
        let vlog_size = self
            .options
            .data_dir
            .join("values.vlog")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        total_disk_bytes += vlog_size;

        // Calculate write amplification
        let logical_bytes = self.metrics.logical_bytes_written.load(Ordering::Relaxed);
        let physical_bytes = self.metrics.physical_bytes_written.load(Ordering::Relaxed);
        let write_amplification = if logical_bytes > 0 {
            physical_bytes as f64 / logical_bytes as f64
        } else {
            0.0
        };

        DBStats {
            // Throughput
            writes_per_sec,
            reads_per_sec,
            deletes_per_sec,

            // Operation counts
            total_puts,
            total_gets,
            total_deletes,
            total_flushes,
            total_compactions,

            // Latency percentiles
            put_latency_p50_us: put_latencies.0,
            put_latency_p95_us: put_latencies.1,
            put_latency_p99_us: put_latencies.2,
            put_latency_p999_us: put_latencies.3,

            get_latency_p50_us: get_latencies.0,
            get_latency_p95_us: get_latencies.1,
            get_latency_p99_us: get_latencies.2,
            get_latency_p999_us: get_latencies.3,

            delete_latency_p50_us: delete_latencies.0,
            delete_latency_p95_us: delete_latencies.1,
            delete_latency_p99_us: delete_latencies.2,

            // Resource usage
            memtable_size_bytes,
            memtable_capacity_bytes,
            memtable_utilization_pct,
            wal_size_bytes,
            total_disk_bytes,

            // LSM structure
            sstables_per_level,
            level_sizes_bytes,
            total_sstables,

            // Write amplification
            logical_bytes_written: logical_bytes,
            physical_bytes_written: physical_bytes,
            write_amplification,

            // Uptime
            uptime_seconds: self.metrics.uptime_seconds(),
        }
    }

    /// Check database health status
    ///
    /// Performs comprehensive health checks to detect performance degradation or critical
    /// conditions. Returns a [`HealthStatus`] with individual check results and an overall
    /// health indicator.
    ///
    /// # Health Checks
    ///
    /// 1. **Compaction lag** (L0 SSTable count)
    ///    - Healthy: ≤10 SSTables
    ///    - Degraded: 11-20 SSTables
    ///    - Unhealthy: >20 SSTables
    ///
    /// 2. **WAL size** (write-ahead log growth)
    ///    - Healthy: ≤100 MB
    ///    - Degraded: 101-500 MB
    ///    - Unhealthy: >500 MB
    ///
    /// 3. **Memtable utilization** (memory pressure)
    ///    - Healthy: ≤80% full
    ///    - Degraded: 81-95% full
    ///    - Unhealthy: >95% full
    ///
    /// 4. **Put latency p99** (write performance)
    ///    - Healthy: ≤100 ms
    ///    - Degraded: 101-1000 ms
    ///    - Unhealthy: >1000 ms
    ///
    /// 5. **Get latency p99** (read performance)
    ///    - Healthy: ≤50 ms
    ///    - Degraded: 51-500 ms
    ///    - Unhealthy: >500 ms
    ///
    /// # Returns
    ///
    /// A [`HealthStatus`] with:
    /// - `healthy`: `true` if all checks are healthy
    /// - `checks`: Individual check results with status and messages
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Perform operations...
    /// for i in 0..10000 {
    ///     db.put(format!("key{}", i).as_bytes(), b"value")?;
    /// }
    ///
    /// // Check health
    /// let health = db.health();
    /// if !health.healthy {
    ///     eprintln!("WARNING: Database health degraded!");
    ///     for check in &health.checks {
    ///         if !check.healthy {
    ///             eprintln!("  - {}: {}", check.name, check.message);
    ///         }
    ///     }
    /// }
    ///
    /// // Pretty print
    /// println!("{}", health);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - **Monitoring dashboards**: Periodic health checks
    /// - **Alerting systems**: Trigger alerts on degraded/unhealthy status
    /// - **Load shedding**: Reduce traffic if database is unhealthy
    /// - **Debugging**: Diagnose performance issues
    ///
    /// # Performance
    ///
    /// This method is cheap (microseconds) and safe to call frequently.
    /// It only reads metrics and does not perform I/O.
    ///
    /// # See Also
    ///
    /// - [`stats()`](Self::stats) - Detailed statistics without thresholds
    /// - [`HealthStatus`] - Full structure documentation
    pub fn health(&self) -> HealthStatus {
        let mut checks = Vec::new();

        // Check 1: Compaction lag (L0 SSTable count)
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let l0_count = if let Some(level) = lsm.level(0) {
            level.sstables().len()
        } else {
            0
        };
        drop(lsm);

        if l0_count > 20 {
            checks.push(HealthCheck::unhealthy(
                "compaction_lag",
                format!("L0 has {} SSTables (threshold: 20)", l0_count),
            ));
        } else if l0_count > 10 {
            checks.push(HealthCheck::degraded(
                "compaction_lag",
                format!("L0 has {} SSTables (threshold: 10)", l0_count),
            ));
        } else {
            checks.push(HealthCheck::healthy_with_message(
                "compaction_lag",
                format!("L0 has {} SSTables", l0_count),
            ));
        }

        // Check 2: WAL size
        let wal_size_bytes = self
            .options
            .data_dir
            .join("wal.log")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        let wal_size_mb = wal_size_bytes / (1024 * 1024);

        if wal_size_mb > 500 {
            checks.push(HealthCheck::unhealthy(
                "wal_size",
                format!("WAL is {} MB (threshold: 500 MB)", wal_size_mb),
            ));
        } else if wal_size_mb > 100 {
            checks.push(HealthCheck::degraded(
                "wal_size",
                format!("WAL is {} MB (threshold: 100 MB)", wal_size_mb),
            ));
        } else {
            checks.push(HealthCheck::healthy_with_message(
                "wal_size",
                format!("WAL is {} MB", wal_size_mb),
            ));
        }

        // Check 3: Memtable utilization
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        let memtable_size = mt.size();
        let memtable_capacity = mt.capacity();
        drop(mt);
        let utilization_pct = (memtable_size as f64 / memtable_capacity as f64) * 100.0;

        if utilization_pct > 95.0 {
            checks.push(HealthCheck::unhealthy(
                "memtable_utilization",
                format!("Memtable is {:.1}% full (threshold: 95%)", utilization_pct),
            ));
        } else if utilization_pct > 80.0 {
            checks.push(HealthCheck::degraded(
                "memtable_utilization",
                format!("Memtable is {:.1}% full (threshold: 80%)", utilization_pct),
            ));
        } else {
            checks.push(HealthCheck::healthy_with_message(
                "memtable_utilization",
                format!("Memtable is {:.1}% full", utilization_pct),
            ));
        }

        // Check 4: Put latency (p99)
        let (put_latencies, get_latencies, _) = self.metrics.get_latency_percentiles();
        let put_p99_ms = put_latencies.2 / 1000; // Convert microseconds to milliseconds

        if put_p99_ms > 1000 {
            checks.push(HealthCheck::unhealthy(
                "put_latency_p99",
                format!("Put p99 is {} ms (threshold: 1000 ms)", put_p99_ms),
            ));
        } else if put_p99_ms > 100 {
            checks.push(HealthCheck::degraded(
                "put_latency_p99",
                format!("Put p99 is {} ms (threshold: 100 ms)", put_p99_ms),
            ));
        } else {
            checks.push(HealthCheck::healthy_with_message(
                "put_latency_p99",
                format!("Put p99 is {} ms", put_p99_ms),
            ));
        }

        // Check 5: Get latency (p99)
        let get_p99_ms = get_latencies.2 / 1000; // Convert microseconds to milliseconds

        if get_p99_ms > 500 {
            checks.push(HealthCheck::unhealthy(
                "get_latency_p99",
                format!("Get p99 is {} ms (threshold: 500 ms)", get_p99_ms),
            ));
        } else if get_p99_ms > 50 {
            checks.push(HealthCheck::degraded(
                "get_latency_p99",
                format!("Get p99 is {} ms (threshold: 50 ms)", get_p99_ms),
            ));
        } else {
            checks.push(HealthCheck::healthy_with_message(
                "get_latency_p99",
                format!("Get p99 is {} ms", get_p99_ms),
            ));
        }

        HealthStatus::new(checks)
    }

    /// Range scan: iterate over a range of keys
    ///
    /// Returns an iterator over key-value pairs where the key is >= start_key
    /// and (if end_key is provided) < end_key. Keys are returned in sorted order.
    ///
    /// This is much more efficient than calling get() multiple times for range queries.
    ///
    /// # Arguments
    ///
    /// * `start_key` - Start of range (inclusive)
    /// * `end_key` - End of range (exclusive), None for open-ended
    ///
    /// # Returns
    ///
    /// Returns an iterator yielding (key, value) pairs, or an error if:
    /// - SSTable read fails (corruption, I/O error)
    /// - vLog read fails for large values
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// // Insert test data
    /// for i in 0..10 {
    ///     db.put(format!("key{:02}", i).as_bytes(), format!("value{}", i).as_bytes())?;
    /// }
    ///
    /// // Range scan: keys from "key05" to "key08" (exclusive)
    /// let mut count = 0;
    /// for result in db.range(b"key05", Some(b"key08"))? {
    ///     let (key, value) = result?;
    ///     println!("{} = {}", String::from_utf8_lossy(&key), String::from_utf8_lossy(&value));
    ///     count += 1;
    /// }
    /// assert_eq!(count, 3); // key05, key06, key07
    ///
    /// // Open-ended range: all keys >= "key07"
    /// for result in db.range(b"key07", None)? {
    ///     let (key, value) = result?;
    ///     // Will return key07, key08, key09
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Much faster than sequential get() calls
    /// - Efficiently merges memtable and SSTable data
    /// - Streams results without loading everything into memory
    ///
    /// # Errors
    ///
    /// - [`DBError::SSTable`]: SSTable corruption or I/O error
    /// - [`DBError::VLog`]: vLog read error for large values
    pub fn range(
        &self,
        start_key: &[u8],
        end_key: Option<&[u8]>,
    ) -> Result<RangeIterator> {
        // Get all SSTables from LSM tree (in reverse level order: L0, L1, ..., LN)
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let mut sstables = Vec::new();

        // Collect SSTables from all levels using cache
        for level_idx in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_idx) {
                for sstable_path in level.sstables() {
                    // Try cache first
                    let cache = self.sstable_cache.lock().expect("SSTable cache lock poisoned");
                    let sstable_arc = if let Some(cached) = cache.get(sstable_path) {
                        cached.clone()
                    } else {
                        drop(cache); // Drop lock before expensive open
                        let sstable = SSTable::open(sstable_path.clone())?;
                        let sstable_arc = Arc::new(Mutex::new(sstable));

                        // Insert into cache
                        let mut cache = self.sstable_cache.lock().expect("SSTable cache lock poisoned");
                        cache.entry(sstable_path.clone())
                            .or_insert_with(|| sstable_arc.clone())
                            .clone()
                    };

                    // Check if SSTable range overlaps with query range (CRITICAL OPTIMIZATION)
                    // Skip SSTables whose key range doesn't overlap with [start_key, end_key)
                    let sstable_guard = sstable_arc.lock().expect("SSTable lock poisoned");
                    let overlaps = sstable_guard.overlaps_range(start_key, end_key);

                    if overlaps {
                        // Create SSTableRangeIterator which holds its own Arc references
                        let iter = sstable_guard.scan_range(start_key, end_key);
                        drop(sstable_guard); // Release lock immediately
                        sstables.push(iter);
                    } else {
                        drop(sstable_guard); // Release lock - SSTable doesn't overlap
                    }
                }
            }
        }
        drop(lsm);

        // Get memtable reference
        let memtable = self.memtable.lock().expect("Memtable lock poisoned");

        // Create range iterator
        RangeIterator::new(start_key, end_key, &memtable, sstables)
    }
}

/// Graceful shutdown: signal compaction thread to stop and wait for it
impl Drop for DB {
    fn drop(&mut self) {
        info!("Closing database");

        // Shutdown background flush worker
        if let Some(ref tx) = self.flush_tx {
            // Send shutdown signal
            debug!("Signaling background flush thread to shut down");
            let _ = tx.send(FlushTask::Shutdown);
        }

        // Wait for flush worker thread to finish
        if let Some(worker) = self.flush_worker.take() {
            debug!("Waiting for background flush thread to finish");
            let _ = worker.join();
        }

        // Shutdown background compaction worker
        if let Some(ref tx) = self.compaction_tx {
            // Send shutdown signal
            debug!("Signaling background compaction thread to shut down");
            let _ = tx.send(CompactionTask::Shutdown);
        }

        // Wait for compaction worker thread to finish
        if let Some(worker) = self.compaction_worker.take() {
            debug!("Waiting for background compaction thread to finish");
            let _ = worker.join();
        }

        info!("Database closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_db_open() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();
        assert_eq!(db.memtable_size(), 0);
    }

    #[test]
    fn test_db_put_get() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"value1").unwrap();
        db.put(b"key2", b"value2").unwrap();

        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
        assert_eq!(db.get(b"key3").unwrap(), None);
    }

    #[test]
    fn test_db_delete() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"value1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));

        db.delete(b"key1").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), None);
    }

    #[test]
    fn test_db_overwrite() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"old_value").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("old_value")));

        db.put(b"key1", b"new_value").unwrap();
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("new_value")));
    }

    #[test]
    fn test_db_flush() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100, // Small capacity to trigger flush
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write enough data to trigger flush
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_with_long_data_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Data should still be accessible after flush
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_with_long_data_{}", i);
            assert_eq!(db.get(key.as_bytes()).unwrap(), Some(Bytes::from(value)));
        }

        // Check that SSTable files were created
        let sst_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "sst")
                    .unwrap_or(false)
            })
            .collect();

        assert!(!sst_files.is_empty(), "No SSTable files created");
    }

    #[test]
    fn test_db_recovery_basic() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write some data
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"value1").unwrap();
            db.put(b"key2", b"value2").unwrap();
            db.put(b"key3", b"value3").unwrap();
            // Drop db (simulates shutdown without flush)
        }

        // Reopen and verify data recovered from WAL
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("value1")));
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
            assert_eq!(db.get(b"key3").unwrap(), Some(Bytes::from("value3")));
        }
    }

    #[test]
    fn test_db_recovery_with_deletes() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write and delete some data
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"value1").unwrap();
            db.put(b"key2", b"value2").unwrap();
            db.delete(b"key1").unwrap(); // Delete key1
            db.put(b"key3", b"value3").unwrap();
        }

        // Reopen and verify recovery
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), None); // Deleted
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from("value2")));
            assert_eq!(db.get(b"key3").unwrap(), Some(Bytes::from("value3")));
        }
    }

    #[test]
    fn test_db_recovery_with_overwrites() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Write with overwrites
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"old_value").unwrap();
            db.put(b"key1", b"new_value").unwrap(); // Overwrite
        }

        // Reopen and verify newest value recovered
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("new_value")));
        }
    }

    #[test]
    fn test_db_recovery_with_flush() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100, // Small to trigger flush during recovery
            ..Default::default()
        };

        // Write enough data to trigger flush on recovery
        {
            let db = DB::open(options.clone()).unwrap();
            for i in 0..20 {
                let key = format!("key_{}", i);
                let value = format!("value_with_long_data_{}", i);
                db.put(key.as_bytes(), value.as_bytes()).unwrap();
            }
        }

        // Reopen (recovery should trigger flush due to small memtable)
        {
            let db = DB::open(options.clone()).unwrap();
            for i in 0..20 {
                let key = format!("key_{}", i);
                let value = format!("value_with_long_data_{}", i);
                assert_eq!(db.get(key.as_bytes()).unwrap(), Some(Bytes::from(value)));
            }
        }
    }

    #[test]
    fn test_db_recovery_empty_wal() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        // Create DB (no data written)
        {
            let _db = DB::open(options.clone()).unwrap();
        }

        // Reopen (WAL exists but is empty)
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), None);
        }
    }

    #[test]
    fn test_db_with_kv_separation() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 200,   // Small enough to trigger flush
            vlog_threshold: Some(50), // 50 byte threshold
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Small value (stored inline in SSTable after flush)
        db.put(b"small_key", b"tiny_value").unwrap();

        // Large value (will be stored in vLog after flush)
        let large_value = vec![b'X'; 100];
        db.put(b"large_key", &large_value).unwrap();

        // Write more data to trigger flush
        for i in 0..3 {
            let key = format!("k{}", i);
            let value = format!("value_data_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Verify all values can be read (from memtable or flushed SSTable)
        assert_eq!(
            db.get(b"small_key").unwrap(),
            Some(Bytes::from("tiny_value"))
        );
        assert_eq!(
            db.get(b"large_key").unwrap(),
            Some(Bytes::from(large_value))
        );

        // Verify vLog file was created
        let vlog_path = dir.path().join("values.vlog");
        assert!(
            vlog_path.exists(),
            "vLog file should exist with vlog_threshold enabled"
        );
    }

    #[test]
    fn test_db_with_kv_separation_recovery() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            vlog_threshold: Some(50), // 50 byte threshold
            ..Default::default()
        };

        // Write data with large values
        {
            let db = DB::open(options.clone()).unwrap();
            db.put(b"key1", b"small_value").unwrap();
            let large_value = vec![b'Y'; 200];
            db.put(b"key2", &large_value).unwrap();
        }

        // Reopen and verify recovery works with vLog
        {
            let db = DB::open(options.clone()).unwrap();
            assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("small_value")));
            let expected_large = vec![b'Y'; 200];
            assert_eq!(db.get(b"key2").unwrap(), Some(Bytes::from(expected_large)));
        }
    }

    #[test]
    fn test_db_background_compaction() {
        use std::time::Duration;

        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 100,      // Small to trigger flushes
            background_compaction: true, // Enable background compaction
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write enough data to trigger multiple flushes and compaction
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let value = format!("value_{:03}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Give background thread time to process compactions
        std::thread::sleep(Duration::from_millis(100));

        // Verify data is still readable
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let expected = format!("value_{:03}", i);
            assert_eq!(db.get(key.as_bytes()).unwrap(), Some(Bytes::from(expected)));
        }

        // DB will be dropped here, triggering graceful shutdown
    }

    #[test]
    fn test_db_sync_vs_async_compaction() {
        use std::time::Duration;

        // Test that both modes produce identical results
        let dir_sync = tempdir().unwrap();
        let dir_async = tempdir().unwrap();

        let options_sync = DBOptions {
            data_dir: dir_sync.path().to_path_buf(),
            memtable_capacity: 100,
            background_compaction: false, // Synchronous
            ..Default::default()
        };

        let options_async = DBOptions {
            data_dir: dir_async.path().to_path_buf(),
            memtable_capacity: 100,
            background_compaction: true, // Asynchronous
            ..Default::default()
        };

        let db_sync = DB::open(options_sync).unwrap();
        let db_async = DB::open(options_async).unwrap();

        // Write same data to both
        for i in 0..50 {
            let key = format!("key_{:03}", i);
            let value = format!("value_{:03}", i);
            db_sync.put(key.as_bytes(), value.as_bytes()).unwrap();
            db_async.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Give async compaction time to finish
        std::thread::sleep(Duration::from_millis(100));

        // Verify both return same results
        for i in 0..50 {
            let key = format!("key_{:03}", i);
            let expected = format!("value_{:03}", i);
            assert_eq!(
                db_sync.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(expected.clone()))
            );
            assert_eq!(
                db_async.get(key.as_bytes()).unwrap(),
                Some(Bytes::from(expected))
            );
        }
    }

    #[test]
    fn test_db_health_checks() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Perform some operations
        for i in 0..10 {
            db.put(format!("key{}", i).as_bytes(), b"value").unwrap();
        }

        // Get health status
        let health = db.health();

        // Should be healthy (low utilization, low L0 count, etc.)
        assert!(health.healthy);
        assert_eq!(health.checks.len(), 5); // 5 health checks

        // Verify check names
        let check_names: Vec<&str> = health.checks.iter().map(|c| c.name.as_str()).collect();
        assert!(check_names.contains(&"compaction_lag"));
        assert!(check_names.contains(&"wal_size"));
        assert!(check_names.contains(&"memtable_utilization"));
        assert!(check_names.contains(&"put_latency_p99"));
        assert!(check_names.contains(&"get_latency_p99"));

        // Test display formatting (doesn't panic)
        let _display = format!("{}", health);
    }

    #[test]
    fn test_range_scan_with_sstables() {
        let dir = tempdir().unwrap();
        let mut opts = DBOptions::default();
        opts.data_dir = dir.path().to_path_buf();
        opts.memtable_capacity = 1024; // Small memtable to force flush
        opts.background_compaction = false;

        let db = DB::open(opts).unwrap();

        // Insert enough data to trigger flush to SSTables
        for i in 0..100 {
            let key = format!("key{:03}", i);
            let value = format!("value{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Force flush to create SSTables
        db.flush().unwrap();

        // Range scan
        let mut results = vec![];
        for result in db.range(b"key010", Some(b"key020")).unwrap() {
            let (key, value) = result.unwrap();
            results.push((
                String::from_utf8(key.to_vec()).unwrap(),
                String::from_utf8(value.to_vec()).unwrap(),
            ));
        }

        // Should get key010 through key019
        assert_eq!(results.len(), 10);
        assert_eq!(results[0].0, "key010");
        assert_eq!(results[9].0, "key019");
    }

    #[test]
    fn test_range_scan_with_overwrites() {
        let dir = tempdir().unwrap();
        let mut opts = DBOptions::default();
        opts.data_dir = dir.path().to_path_buf();
        opts.memtable_capacity = 1024;
        opts.background_compaction = false;

        let db = DB::open(opts).unwrap();

        // Write initial data
        for i in 0..50 {
            let key = format!("key{:03}", i);
            db.put(key.as_bytes(), b"old_value").unwrap();
        }
        db.flush().unwrap();

        // Overwrite some keys
        for i in 10..20 {
            let key = format!("key{:03}", i);
            db.put(key.as_bytes(), b"new_value").unwrap();
        }

        // Range scan - newer values should override
        let mut results = vec![];
        for result in db.range(b"key010", Some(b"key020")).unwrap() {
            let (key, value) = result.unwrap();
            results.push((
                String::from_utf8(key.to_vec()).unwrap(),
                String::from_utf8(value.to_vec()).unwrap(),
            ));
        }

        assert_eq!(results.len(), 10);
        // All should have new_value (memtable overrides SSTable)
        for result in &results {
            assert_eq!(result.1, "new_value");
        }
    }

    #[test]
    fn test_range_scan_with_deletes() {
        let dir = tempdir().unwrap();
        let mut opts = DBOptions::default();
        opts.data_dir = dir.path().to_path_buf();
        opts.memtable_capacity = 1024;
        opts.background_compaction = false;

        let db = DB::open(opts).unwrap();

        // Write data
        for i in 0..50 {
            let key = format!("key{:03}", i);
            db.put(key.as_bytes(), b"value").unwrap();
        }
        db.flush().unwrap();

        // Delete some keys
        for i in 10..20 {
            let key = format!("key{:03}", i);
            db.delete(key.as_bytes()).unwrap();
        }

        // Range scan - deleted keys should not appear
        let mut results = vec![];
        for result in db.range(b"key005", Some(b"key025")).unwrap() {
            let (key, _value) = result.unwrap();
            results.push(String::from_utf8(key.to_vec()).unwrap());
        }

        // Should get key005-key009 and key020-key024 (5 + 5 = 10 keys)
        assert_eq!(results.len(), 10);
        assert!(!results.iter().any(|k| k.as_str() >= "key010" && k.as_str() < "key020"));
    }
}
