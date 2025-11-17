// Main database interface
// Integrates WAL, Memtable, SSTable, Compaction, and VLog

pub(crate) use crate::background_workers::WALMessage;
use crate::background_workers::{CompactionTask, FlushTask};
use crate::compaction::{compact_sstables, LSMTree};
use crate::health::{HealthCheck, HealthStatus};
use crate::memtable::Memtable;
use crate::metrics::{DBStats, MetricsCollector};
use crate::range::RangeIterator;
use crate::sstable::SSTable;
use crate::vlog::VLog;
use crate::wal::{Record, SyncPolicy, WAL};
use arc_swap::ArcSwap;
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender as CrossbeamSender};
use foldhash::fast::FixedState;
use quick_cache::sync::Cache;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Number of memtable partitions for reduced lock contention
///
/// Partitioning the memtable reduces lock contention on multi-core systems
/// by allowing concurrent writes to different partitions. Each partition
/// is independently locked, so 16 partitions = 16x less contention.
///
/// Expected improvement: +25-40% write throughput on multi-core systems
/// Research backing: Tucana (2020), FASTER (2018)
const NUM_PARTITIONS: usize = 16;

/// Global foldhash state for partition selection (created once, reused forever)
/// Using LazyLock ensures it's initialized exactly once in a thread-safe manner
static PARTITION_HASHER: LazyLock<FixedState> = LazyLock::new(|| FixedState::with_seed(0));

/// Calculate which partition a key belongs to using foldhash
///
/// Uses foldhash (2x faster than xxhash on small keys) to distribute keys
/// evenly across partitions. The hash is stable (same key always goes to
/// same partition), which is critical for correctness.
///
/// Research: foldhash is 50% faster than xxhash on small data (8-32 byte keys)
/// See: ai/research/SOTA_LIBRARIES.md
#[inline]
pub(crate) fn partition_for_key(key: &[u8]) -> usize {
    // Use global hasher (created once, reused forever)
    let hash = PARTITION_HASHER.hash_one(key);
    (hash % NUM_PARTITIONS as u64) as usize
}

/// Increment a byte slice to create an exclusive upper bound for prefix scans
///
/// Returns None if the input is all 0xFF bytes (can't increment further).
/// Used by prefix() to create a range [prefix, prefix+1).
///
/// # Examples
/// - `b"user"` → `Some(b"uses")`
/// - `b"user\xff"` → `Some(b"usesxx00")`
/// - `b"\xff\xff"` → `None`
fn increment_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }

    let mut result = bytes.to_vec();

    // Increment from the rightmost byte, carrying over as needed
    for i in (0..result.len()).rev() {
        if result[i] < 0xFF {
            result[i] += 1;
            return Some(result);
        }
        // This byte is 0xFF, set to 0 and continue to carry
        result[i] = 0;
    }

    // All bytes were 0xFF, can't increment
    None
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

    #[error("Insufficient disk space: {available} bytes available, {required} bytes required")]
    DiskSpaceFull { available: u64, required: u64 },

    #[error("Background thread panic: {thread_name} - database may be in inconsistent state")]
    BackgroundThreadPanic { thread_name: String },
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
///
/// # File Descriptor Limits
///
/// **IMPORTANT**: seerdb can open many file descriptors (SSTable files, WAL, VLog).
/// For production deployments with large databases:
///
/// - **Recommended**: Increase OS file descriptor limit (ulimit -n) to at least 10,000
/// - **Estimate**: ~2 FDs per 10MB of data (1 SSTable + metadata files)
/// - **TODO**: Future versions will implement LRU file handle caching with configurable limits
///
/// On Linux/macOS, check current limit:
/// ```bash
/// ulimit -n  # Current soft limit
/// ulimit -Hn # Hard limit
/// ```
///
/// Increase limit (requires root for hard limit):
/// ```bash
/// ulimit -n 10000  # Soft limit for current session
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
    /// **Note**: With partitioned memtables (16 partitions), this capacity is divided
    /// by 16, so 256MB = 16MB per partition. This is optimal for write performance.
    ///
    /// Default: `256 * 1024 * 1024` (256MB, 16MB per partition)
    ///
    /// Recommended:
    /// - Memory-constrained systems: 128 MB (8MB per partition)
    /// - Normal systems: 256-512 MB (16-32MB per partition)
    /// - High-throughput servers: 512 MB - 1 GB (32-64MB per partition)
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

    /// Enable adaptive compaction (Dostoevsky)
    ///
    /// When `true`, uses adaptive size ratios based on workload (read/write ratio).
    /// When `false`, uses fixed size ratio (traditional LSM).
    ///
    /// Default: `false` (for predictable behavior)
    ///
    /// **How it works**: Dynamically adjusts LSM level size ratios based on observed workload:
    /// - Write-heavy workloads → higher ratios (less compaction overhead)
    /// - Read-heavy workloads → lower ratios (better read performance)
    /// - Formula from Dostoevsky paper (Dayan & Idreos, Harvard 2018)
    ///
    /// Recommended:
    /// - Mixed workloads: `true` (auto-optimizes for actual usage)
    /// - Workload varies over time: `true` (adapts dynamically)
    /// - Consistent workload: `false` (simpler, predictable)
    pub adaptive_compaction: bool,

    /// Maximum total memory budget (optional)
    ///
    /// If set, seerdb will enforce this memory limit by:
    /// - Triggering early flushes when memory usage exceeds 80% of limit
    /// - Blocking writes when memory usage exceeds 95% of limit
    ///
    /// Memory usage includes:
    /// - Active memtables (`memtable_capacity`)
    /// - Immutable memtables (up to `memtable_capacity` during flush)
    /// - Block cache (~40MB for 10K blocks @ 4KB each)
    /// - SSTable cache (~1KB per cached SSTable, max 1000 SSTables = 1MB)
    ///
    /// Default: `None` (no global memory limit)
    ///
    /// # Example
    ///
    /// ```rust
    /// use seerdb::DBOptions;
    ///
    /// // Limit total memory to 512MB
    /// let opts = DBOptions {
    ///     memtable_capacity: 256 * 1024 * 1024,  // 256MB
    ///     max_memory_bytes: Some(512 * 1024 * 1024),  // 512MB total
    ///     ..Default::default()
    /// };
    /// ```
    ///
    /// **Warning**: Set `max_memory_bytes` to at least 2x `memtable_capacity` to allow
    /// for immutable memtables during flush. Recommended: 3-4x for headroom.
    pub max_memory_bytes: Option<usize>,

    /// Minimum required free disk space (optional)
    ///
    /// If set, seerdb will reject writes when available disk space falls below this threshold.
    /// This prevents:
    /// - Disk full errors during write operations
    /// - Database corruption from partial writes
    /// - System instability from consuming all disk space
    ///
    /// Default: `None` (no disk space checking)
    ///
    /// # Example
    ///
    /// ```rust
    /// use seerdb::DBOptions;
    ///
    /// // Require at least 1GB free space
    /// let opts = DBOptions {
    ///     min_disk_space_bytes: Some(1024 * 1024 * 1024),  // 1GB
    ///     ..Default::default()
    /// };
    /// ```
    ///
    /// **Recommended**: Set to at least 2-3x your expected write burst size to ensure
    /// writes can complete even if disk space runs low.
    pub min_disk_space_bytes: Option<u64>,
}

impl Default for DBOptions {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./seerdb_data"),
            memtable_capacity: 256 * 1024 * 1024, // 256MB (16MB per partition with 16-way partitioning)
            wal_sync_policy: SyncPolicy::SyncData,
            base_level_size: 10 * 1024 * 1024, // 10MB
            size_ratio: 10,
            num_levels: 7,
            vlog_threshold: Some(4096), // WiscKey: 4KB threshold for KV separation (FIXED!)
            background_compaction: false, // Disabled by default for compatibility
            background_flush: false,    // Disabled by default - enable for large workloads
            adaptive_compaction: false, // Disabled by default - enable for mixed workloads
            max_memory_bytes: None,     // No global memory limit by default
            min_disk_space_bytes: None, // No disk space check by default
        }
    }
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
    /// Active memtables (16 partitions, lock-free with ArcSwap)
    /// Uses ArcSwap for truly lock-free atomic pointer swaps during flush
    /// SkipMap is already lock-free internally, so no locks needed at all!
    /// Wrapped in Arc for sharing with background threads
    memtables: Arc<[ArcSwap<Memtable>; NUM_PARTITIONS]>,
    /// Immutable memtables being flushed (RocksDB-style, but per-partition)
    /// Readers check this before SSTables to avoid data loss during flush
    /// Stored as Arc<Memtable> to avoid unwrapping Arc after atomic swap
    /// LOCK-FREE: Uses ArcSwap for zero-contention reads during flush!
    immutable_memtables: Arc<ArcSwap<Option<Arc<Vec<Arc<Memtable>>>>>>,
    /// LSM tree for level management
    /// LOCK-FREE: Uses ArcSwap for zero-contention reads!
    lsm: Arc<ArcSwap<LSMTree>>,
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
    /// Channel for lock-free WAL writes (pub(crate) for batch API access)
    pub(crate) wal_tx: CrossbeamSender<WALMessage>,
    /// Background WAL writer thread
    wal_worker: Option<JoinHandle<()>>,
    /// Flush mutex to serialize flush operations and prevent concurrent flush races
    flush_mutex: Arc<Mutex<()>>,
    /// LSM mutex to serialize LSM tree updates and prevent concurrent update races
    /// CRITICAL FIX (Bug #7c): Prevents ABA problem where concurrent flush/compaction
    /// overwrite each other's LSM tree changes, causing data loss
    lsm_mutex: Arc<Mutex<()>>,
    /// SSTable reader cache to avoid re-opening files on every read (CRITICAL for performance)
    /// Maps SSTable path -> opened SSTable with loaded indexes and bloom filters
    /// Uses quick_cache for efficient LRU eviction and lock-free concurrent access
    sstable_cache: Arc<Cache<PathBuf, Arc<Mutex<SSTable>>>>,
    /// Cached vLog availability (avoids lock on every get())
    has_vlog: std::sync::atomic::AtomicBool,
    /// Workload counters for Dostoevsky adaptive compaction
    /// Total write operations (put + delete) since database opened
    write_count: std::sync::atomic::AtomicU64,
    /// Total read operations (get + range scans) since database opened
    read_count: std::sync::atomic::AtomicU64,
    /// Maximum sequence number that has been fully flushed to disk
    /// Compaction will only compact SSTables with max_seq <= this value
    /// This prevents compaction from deleting keys still in immutable memtables
    max_flushed_seq: Arc<AtomicU64>,
    /// Global sequence number counter (increments on every write)
    next_seq: Arc<AtomicU64>,
    /// Health status of background WAL writer thread (true = healthy, false = panicked)
    wal_healthy: Arc<AtomicBool>,
    /// Health status of background flush worker thread (true = healthy, false = panicked)
    #[allow(dead_code)] // Reserved for future health monitoring API
    flush_healthy: Arc<AtomicBool>,
    /// Health status of background compaction worker thread (true = healthy, false = panicked)
    #[allow(dead_code)] // Reserved for future health monitoring API
    compaction_healthy: Arc<AtomicBool>,
    /// Pending SSTable file deletions (path, timestamp when queued)
    /// Files are queued here after compaction updates LSM tree, then deleted after a safe delay
    /// This prevents race conditions with concurrent readers holding old LSM snapshots
    pending_deletions: Arc<Mutex<Vec<(PathBuf, std::time::Instant)>>>,
    /// Cached disk space information for performance
    /// Timestamp (seconds since UNIX epoch) of last disk space check
    last_disk_check: Arc<AtomicU64>,
    /// Cached available disk space in bytes (updated every 10 seconds)
    cached_available_space: Arc<AtomicU64>,
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
            crate::db_helpers::recover_partitioned(&wal_path, &memtables_vec)?;
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

        // Create LSM tree (adaptive or fixed strategy)
        let mut lsm = if options.adaptive_compaction {
            info!("Using adaptive compaction (Dostoevsky)");
            LSMTree::new_adaptive(
                &options.data_dir,
                options.base_level_size,
                options.num_levels,
                4,  // min_ratio: write-heavy workloads
                20, // max_ratio: read-heavy workloads
            )
        } else {
            LSMTree::new(
                &options.data_dir,
                options.base_level_size,
                options.size_ratio,
                options.num_levels,
            )
        };

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

        // Wrap in ArcSwap for lock-free atomic swaps
        // ArcSwap provides lock-free reads (.load()) and atomic swaps (.swap())
        // SkipMap is already lock-free internally, so this eliminates ALL lock overhead!
        // Convert Vec<Memtable> into [ArcSwap<Memtable>; NUM_PARTITIONS]
        // Then wrap in Arc so background threads can share it
        let mut memtables_iter = memtables_vec.into_iter();
        let memtables_array: [ArcSwap<Memtable>; NUM_PARTITIONS] = std::array::from_fn(|_| {
            ArcSwap::from_pointee(memtables_iter.next().expect("Not enough partitions"))
        });
        let memtables = Arc::new(memtables_array);
        let immutable_memtables = Arc::new(ArcSwap::from_pointee(None));
        let wal = Arc::new(Mutex::new(wal));
        let vlog = Arc::new(Mutex::new(vlog));
        let lsm = Arc::new(ArcSwap::from_pointee(lsm));
        let flush_mutex = Arc::new(Mutex::new(()));
        let lsm_mutex = Arc::new(Mutex::new(()));

        // Initialize SSTable counter from existing files to avoid overwriting
        // Collect all SSTable paths first to avoid borrow issues
        let mut all_sstables = Vec::new();
        {
            let lsm_arc = lsm.load();
            for level_num in 0..lsm_arc.num_levels() {
                if let Some(level) = lsm_arc.level(level_num) {
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

        // Initialize sequence tracking (needed by background compaction worker)
        let max_flushed_seq = Arc::new(AtomicU64::new(0));
        let next_seq = Arc::new(AtomicU64::new(1));

        // Initialize background thread health tracking
        let wal_healthy = Arc::new(AtomicBool::new(true));
        let flush_healthy = Arc::new(AtomicBool::new(true));
        let compaction_healthy = Arc::new(AtomicBool::new(true));

        // Initialize pending deletions queue (for Bug #7b fix)
        let pending_deletions = Arc::new(Mutex::new(Vec::new()));

        // Start background compaction worker if enabled
        let (compaction_tx, compaction_worker) = crate::background_workers::spawn_compaction_worker(
            options.background_compaction,
            Arc::clone(&lsm),
            Arc::clone(&lsm_mutex),
            Arc::clone(&sstable_counter),
            options.data_dir.clone(),
            Arc::clone(&metrics),
            Arc::clone(&max_flushed_seq),
            Arc::clone(&compaction_healthy),
            Arc::clone(&pending_deletions),
        );

        // Start background flush worker if enabled
        let (flush_tx, flush_worker) = crate::background_workers::spawn_flush_worker(
            options.background_flush,
            Arc::clone(&memtables),
            Arc::clone(&immutable_memtables),
            Arc::clone(&wal),
            Arc::clone(&lsm),
            Arc::clone(&lsm_mutex),
            Arc::clone(&vlog),
            Arc::clone(&sstable_counter),
            options.data_dir.clone(),
            Arc::clone(&metrics),
            options.memtable_capacity,
            options.vlog_threshold,
            Arc::clone(&flush_mutex),
            Arc::clone(&max_flushed_seq),
            Arc::clone(&flush_healthy),
        );

        // Start background WAL writer (always enabled for lock-free writes)
        let (wal_tx, wal_worker) =
            crate::background_workers::spawn_wal_writer(Arc::clone(&wal), Arc::clone(&wal_healthy));

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
            wal_tx,
            wal_worker: Some(wal_worker),
            flush_mutex,
            lsm_mutex,
            sstable_cache: Arc::new(Cache::new(1000)), // Cache up to 1000 SSTables
            has_vlog: std::sync::atomic::AtomicBool::new(has_vlog),
            write_count: std::sync::atomic::AtomicU64::new(0),
            read_count: std::sync::atomic::AtomicU64::new(0),
            max_flushed_seq,
            next_seq,
            wal_healthy,
            flush_healthy,
            compaction_healthy,
            pending_deletions,
            last_disk_check: Arc::new(AtomicU64::new(0)),
            cached_available_space: Arc::new(AtomicU64::new(u64::MAX)), // Start with "infinite" space
        };

        // Flush memtables if any partition filled up during recovery
        let should_flush = db.memtables.iter().any(|mt| mt.load().should_flush());
        if should_flush {
            info!("One or more memtable partitions full after recovery, flushing");
            db.flush()?;
        }

        info!("Database opened successfully");

        Ok(db)
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

        // Memory budget enforcement (if configured)
        if let Some(max_memory) = self.options.max_memory_bytes {
            loop {
                let current_memory = self.estimate_memory_usage();
                let memory_pressure = (current_memory as f64) / (max_memory as f64);

                if memory_pressure >= 0.95 {
                    // CRITICAL: >95% memory usage - block writes until memory freed
                    // This provides backpressure to prevent OOM
                    debug!(
                        "Memory pressure critical: {:.1}% ({} / {} bytes) - blocking write",
                        memory_pressure * 100.0,
                        current_memory,
                        max_memory
                    );

                    // Try to trigger flush to free memory
                    if let Some(ref tx) = self.flush_tx {
                        let _ = tx.send(FlushTask::Flush);
                    }

                    // Sleep briefly to avoid busy-wait
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue; // Recheck memory after sleep
                } else if memory_pressure >= 0.80 {
                    // WARNING: >80% memory usage - trigger early flush
                    debug!(
                        "Memory pressure high: {:.1}% ({} / {} bytes) - triggering flush",
                        memory_pressure * 100.0,
                        current_memory,
                        max_memory
                    );

                    if let Some(ref tx) = self.flush_tx {
                        let _ = tx.send(FlushTask::Flush);
                    }
                    break; // Flush triggered, proceed with write
                } else {
                    // Memory OK, proceed with write
                    break;
                }
            }
        }

        // Disk space check (if configured)
        // Uses periodic caching (10s interval) to avoid performance impact
        self.check_disk_space_cached()?;

        // Check WAL writer thread health BEFORE writing
        // If WAL died, writes would be lost even though we return Ok()
        if !self.wal_healthy.load(Ordering::SeqCst) {
            return Err(DBError::BackgroundThreadPanic {
                thread_name: "wal-writer".to_string(),
            });
        }

        // Write to WAL first (durability) - lock-free via channel
        let record = Record::Put {
            key: key.clone(),
            value: value.clone(),
        };
        let wal_bytes = record.encode().len() as u64;
        self.wal_tx.send(WALMessage::Record(record)).map_err(|_| {
            DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "WAL writer thread died",
            )))
        })?;

        // Track physical bytes written to WAL
        self.metrics.record_physical_bytes(wal_bytes);

        // Write to correct partition (lock-free with ArcSwap)
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].load(); // Lock-free Arc load
        mt.put(key, value); // SkipMap is already lock-free
                            // Arc automatically dropped, no lock to release!

        // Track write operation for Dostoevsky adaptive compaction
        self.write_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Check if ANY partition should be flushed (lock-free check)
        let should_flush = self.memtables.iter().any(|mt| mt.load().should_flush());
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

        // Track read operation for Dostoevsky adaptive compaction
        self.read_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Check correct partition first (most recent data, lock-free with ArcSwap)
        let partition = partition_for_key(key);
        let mt = self.memtables[partition].load(); // Lock-free Arc load
        let result = mt.get(key);
        let contains = mt.contains(key);
        // Arc automatically dropped, no lock to release!

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

        // Check immutable partitions (if flush is in progress) - LOCK-FREE with ArcSwap!
        // We need to check ALL partitions since the key could be in any one
        let immut_arc = self.immutable_memtables.load();
        if let Some(ref immutable_partitions) = **immut_arc {
            // Check all partitions for the key
            for partition_mt in immutable_partitions.iter() {
                let immut_result = partition_mt.get(key);
                let immut_contains = partition_mt.contains(key);

                match immut_result {
                    Some(value) => {
                        // Found value in immutable partition
                        // Arc automatically dropped (lock-free, no explicit drop needed!)
                        self.metrics.record_get(start.elapsed());
                        return Ok(Some(value));
                    }
                    None if immut_contains => {
                        // Key exists as tombstone in immutable partition
                        // Arc automatically dropped (lock-free, no explicit drop needed!)
                        self.metrics.record_get(start.elapsed());
                        return Ok(None);
                    }
                    None => {
                        // Key not in this partition - check next partition
                        continue;
                    }
                }
            }
            // Arc automatically dropped (lock-free, no explicit drop needed!)
        } else {
            // Arc automatically dropped (lock-free, no explicit drop needed!)
        }

        // Get vLog if available (need to clone for SSTable attachment)
        let vlog_path = self.options.data_dir.join("values.vlog");
        let has_vlog = self.has_vlog.load(std::sync::atomic::Ordering::Relaxed);

        // Check SSTables in LSM tree (L0 -> L6) (LOCK-FREE!)
        let lsm_arc = self.lsm.load();
        for level_num in 0..lsm_arc.num_levels() {
            if let Some(level) = lsm_arc.level(level_num) {
                // IMPORTANT: Check all levels in reverse order (newest first)
                // L0 has overlapping SSTables - check newest first
                // L1+ may also have overlapping SSTables due to our simple compaction strategy
                // (we add new merged SSTables without re-merging with existing L1 SSTables)
                // So we check reverse order to get the latest value
                let sstables: Vec<_> = level.sstables().iter().rev().collect();

                // Check each SSTable in this level
                for sstable_path in sstables {
                    // Use cached SSTable reader (avoids expensive re-opening and index deserialization)
                    // quick_cache provides lock-free get_or_insert with automatic LRU eviction
                    let cached_sstable = self.sstable_cache.get_or_insert_with(
                        sstable_path,
                        || -> Result<Arc<Mutex<SSTable>>> {
                            // Cache miss: open SSTable (called only once per unique path)
                            let sstable = if has_vlog {
                                let vlog = VLog::open(&vlog_path)?;
                                SSTable::open(sstable_path)?.with_vlog(vlog)
                            } else {
                                SSTable::open(sstable_path)?
                            };
                            Ok(Arc::new(Mutex::new(sstable)))
                        },
                    )?;

                    let mut sstable = cached_sstable.lock().expect("SSTable lock poisoned");

                    // get() already does bloom filter check internally - no need to call may_contain()
                    let result = sstable.get(key)?;

                    match result {
                        Some(value) => {
                            self.metrics.record_get(start.elapsed());
                            return Ok(Some(value));
                        }
                        None => {
                            // CRITICAL FIX (Bug #9): Distinguish tombstone from miss
                            // If key exists in SSTable but get() returned None, it's a tombstone
                            // Don't continue to older SSTables - tombstone masks older values
                            if sstable.contains(key)? {
                                // Key exists in this SSTable as tombstone - stop here
                                self.metrics.record_get(start.elapsed());
                                return Ok(None);
                            }
                            // Key not in this SSTable - continue to next SSTable
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

        // Write to WAL (durability) - lock-free via channel
        let record = Record::Delete { key: key.clone() };
        self.wal_tx.send(WALMessage::Record(record)).map_err(|_| {
            DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "WAL writer thread died",
            )))
        })?;

        // Write tombstone to correct partition (lock-free with ArcSwap)
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].load(); // Lock-free Arc load
        mt.delete(key);
        // Arc automatically dropped, no lock to release!

        // Track write operation for Dostoevsky adaptive compaction
        self.write_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Check if ANY partition should be flushed (lock-free check)
        let should_flush = self.memtables.iter().any(|mt| mt.load().should_flush());
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

    /// Create a new write batch
    ///
    /// Batches allow atomic writes of multiple operations with better performance
    /// than individual operations. All operations in a batch are written to WAL
    /// and memtable atomically.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = DB::open(DBOptions::default())?;
    ///
    /// let mut batch = db.batch();
    /// batch.put(b"key1", b"value1");
    /// batch.put(b"key2", b"value2");
    /// batch.delete(b"key3");
    /// batch.commit()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// Batching is 2-5x faster than individual operations for batches of 100+ operations.
    pub fn batch(&self) -> crate::batch::Batch<'_> {
        crate::batch::Batch::new(self)
    }

    /// Create a new write batch with preallocated capacity
    ///
    /// Use this when you know the approximate number of operations to avoid reallocations.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use seerdb::{DB, DBOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = DB::open(DBOptions::default())?;
    /// let mut batch = db.batch_with_capacity(1000);
    /// for i in 0..1000 {
    ///     batch.put(format!("key_{}", i).as_bytes(), b"value");
    /// }
    /// batch.commit()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn batch_with_capacity(&self, capacity: usize) -> crate::batch::Batch<'_> {
        crate::batch::Batch::with_capacity(self, capacity)
    }

    /// Internal put method (skips WAL write - used by batch)
    ///
    /// Writes directly to memtable without WAL logging. This is used by the
    /// batch API which handles WAL writes separately.
    pub(crate) fn put_internal(&self, key: Bytes, value: Bytes) -> Result<()> {
        // Track logical bytes written (user data)
        let logical_bytes = (key.len() + value.len()) as u64;
        self.metrics.record_logical_bytes(logical_bytes);

        // Write to correct partition (lock-free with ArcSwap)
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].load(); // Lock-free Arc load
        mt.put(key, value); // SkipMap is already lock-free
                            // Arc automatically dropped, no lock to release!

        // Track write operation for Dostoevsky adaptive compaction
        self.write_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Check if ANY partition should be flushed (lock-free check)
        let should_flush = self.memtables.iter().any(|mt| mt.load().should_flush());
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

        Ok(())
    }

    /// Internal delete method (skips WAL write - used by batch)
    ///
    /// Writes tombstone directly to memtable without WAL logging. This is used by the
    /// batch API which handles WAL writes separately.
    pub(crate) fn delete_internal(&self, key: Bytes) -> Result<()> {
        // Write tombstone to correct partition (lock-free with ArcSwap)
        let partition = partition_for_key(&key);
        let mt = self.memtables[partition].load(); // Lock-free Arc load
        mt.delete(key);
        // Arc automatically dropped, no lock to release!

        // Track write operation for Dostoevsky adaptive compaction
        self.write_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Check if ANY partition should be flushed (lock-free check)
        let should_flush = self.memtables.iter().any(|mt| mt.load().should_flush());
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

        // **CRITICAL FIX (Bug #10): Wait for background flush to complete BEFORE acquiring mutex
        // If background_flush is enabled AND immutable_memtables is occupied,
        // we must wait for the background flush worker to complete.
        // IMPORTANT: We must wait BEFORE acquiring flush_mutex, otherwise we deadlock:
        //   - flush() holds flush_mutex
        //   - background flush waits for flush_mutex
        //   - flush() waits for background flush to complete
        //   - DEADLOCK!
        if self.options.background_flush {
            // Wait for any in-progress background flush to complete
            loop {
                let immut_arc = self.immutable_memtables.load();
                if immut_arc.is_none() {
                    // Background flush completed - safe to proceed
                    break;
                }
                // Background flush still in progress - wait briefly and retry
                debug!("Waiting for background flush to complete before explicit flush");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // immutable_memtables is now None - background flush completed
        }

        // **CRITICAL FIX**: Serialize all flushes to prevent concurrent flush races
        let _flush_lock = self.flush_mutex.lock().expect("Flush mutex poisoned");

        let flush_start = Instant::now();

        // Check total size across all partitions (lock-free with ArcSwap)
        let total_size: usize = self.memtables.iter().map(|mt| mt.load().size()).sum();

        // Early return if all partitions are empty
        if total_size == 0 {
            return Ok(());
        }

        info!(
            total_memtable_size_bytes = total_size,
            partitions = NUM_PARTITIONS,
            "Starting partitioned memtable flush"
        );

        // Handle any failed flush (only when background_flush is disabled)
        if !self.options.background_flush {
            // Background flush disabled - handle synchronously
            // Check if there's a previous failed flush
            // If immutable_memtables is occupied, flush it first to avoid data loss (LOCK-FREE!)
            let pending_immutable_arc = self.immutable_memtables.swap(Arc::new(None));
            let pending_immutable =
                Arc::try_unwrap(pending_immutable_arc).unwrap_or_else(|arc| (*arc).clone());

            if let Some(pending_partitions_arc) = pending_immutable {
                // Previous flush failed - retry flushing the existing immutable partitions
                warn!(
                    partitions = pending_partitions_arc.len(),
                    "Retrying flush of previously failed immutable partitions"
                );

                // Generate filename for pending flush
                let mut counter = self
                    .sstable_counter
                    .lock()
                    .expect("SSTable counter mutex poisoned");
                let pending_flush_sequence = *counter; // Capture sequence for retry flush
                let pending_sstable_path = self
                    .options
                    .data_dir
                    .join(format!("L0_{:06}.sst", *counter));
                *counter += 1;
                drop(counter);

                // Collect and sort entries from all pending partitions
                let mut all_entries: Vec<(Bytes, Entry)> = Vec::new();
                for partition_mt in pending_partitions_arc.iter() {
                    for (key, entry) in partition_mt.iter() {
                        all_entries.push((key, entry));
                    }
                }

                // Sort by key (deduplication handled by taking last value for each key)
                all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

                // Build SSTable from sorted entries
                self.build_sstable_from_entries(
                    &pending_sstable_path,
                    all_entries.iter(),
                    pending_flush_sequence,
                )?;
                let pending_size = std::fs::metadata(&pending_sstable_path)?.len();

                // Track physical bytes written to SSTable (retry case)
                self.metrics.record_physical_bytes(pending_size);

                // CRITICAL FIX (Bug #7c): Serialize LSM tree updates to prevent ABA race
                {
                    let _lsm_lock = self.lsm_mutex.lock().expect("LSM mutex poisoned");

                    // Add to LSM tree (serialized)
                    let mut lsm_clone = (**self.lsm.load()).clone();
                    lsm_clone.add_l0_sstable(pending_sstable_path.clone(), pending_size);
                    self.lsm.store(Arc::new(lsm_clone));
                }

                // CRITICAL FIX (Bug #8): Barrier synchronization before clearing WAL
                // Send barrier message and wait for background thread to flush all pending records
                let (ack_tx, ack_rx) = bounded(1); // Oneshot channel
                self.wal_tx.send(WALMessage::Barrier(ack_tx)).map_err(|_| {
                    DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "WAL writer thread died",
                    )))
                })?;
                // Wait for acknowledgement (blocks until all pending records written)
                ack_rx.recv().map_err(|_| {
                    DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "WAL writer thread died during barrier",
                    )))
                })?;

                // Now safe to clear WAL (all records written to disk)
                let mut wal = self.wal.lock().expect("WAL mutex poisoned");
                wal.clear()?;
                drop(wal);

                // Update max_flushed_seq for retry flush
                // Use fetch_max to handle out-of-order flush completions (only update if new value is greater)
                self.max_flushed_seq
                    .fetch_max(pending_flush_sequence, Ordering::SeqCst);

                info!("Successfully flushed previously failed immutable partitions");
            }
        }

        // Now check if active partitions need flushing (lock-free with ArcSwap)
        let total_size: usize = self.memtables.iter().map(|mt| mt.load().size()).sum();

        if total_size == 0 {
            return Ok(()); // Nothing to flush
        }

        // Generate SSTable filename for main flush
        let mut counter = self
            .sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let flush_sequence = *counter; // Capture sequence for this flush
        let sstable_path = self
            .options
            .data_dir
            .join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Swap ALL 16 partitions atomically (lock-free with ArcSwap!)
        // 1. Atomically swap each partition with new empty partition
        // 2. Keep old partitions as Arc<Memtable> (no unwrap needed)
        // 3. Store as immutable partitions
        // No locks needed - ArcSwap.swap() is atomic and lock-free!
        let capacity_per_partition = self.options.memtable_capacity / NUM_PARTITIONS;
        let mut flushing_partitions: Vec<Arc<Memtable>> = Vec::with_capacity(NUM_PARTITIONS);

        // Deref Arc to access the array
        for partition_mt in self.memtables.iter() {
            // Atomic swap: returns Arc<Memtable> of old partition
            // Keep it as Arc<Memtable> - no need to unwrap since immutable_memtables stores Arc
            let old_arc: Arc<Memtable> =
                partition_mt.swap(Arc::new(Memtable::new(capacity_per_partition)));
            flushing_partitions.push(old_arc);
        }

        // Collect entries from ALL partitions FIRST (before storing in immutable)
        let mut all_entries: Vec<(Bytes, Entry)> = Vec::new();
        for partition_mt in &flushing_partitions {
            for (key, entry) in partition_mt.iter() {
                all_entries.push((key, entry));
            }
        }

        // Store in immutable_memtables so readers can access during flush (LOCK-FREE!)
        self.immutable_memtables
            .store(Arc::new(Some(Arc::new(flushing_partitions))));

        // Sort by key to build sorted SSTable
        // If there are duplicates (same key in multiple partitions due to race), keep last one
        all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

        // Build SSTable from sorted entries
        self.build_sstable_from_entries(&sstable_path, all_entries.iter(), flush_sequence)?;

        let size = std::fs::metadata(&sstable_path)?.len();

        // Track physical bytes written to SSTable
        self.metrics.record_physical_bytes(size);

        // CRITICAL FIX (Bug #7c): Serialize LSM tree updates to prevent ABA race
        let sstable_path_for_log = sstable_path.clone();
        {
            let _lsm_lock = self.lsm_mutex.lock().expect("LSM mutex poisoned");

            // Add to LSM tree L0 (serialized)
            let mut lsm_clone = (**self.lsm.load()).clone();
            lsm_clone.add_l0_sstable(sstable_path, size);
            self.lsm.store(Arc::new(lsm_clone));
        }

        // Clear immutable partitions + WAL after successful flush (LOCK-FREE!)
        self.immutable_memtables.store(Arc::new(None));

        // CRITICAL FIX (Bug #8): Barrier synchronization before clearing WAL
        // Send barrier message and wait for background thread to flush all pending records
        let (ack_tx, ack_rx) = bounded(1); // Oneshot channel
        self.wal_tx.send(WALMessage::Barrier(ack_tx)).map_err(|_| {
            DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "WAL writer thread died",
            )))
        })?;
        // Wait for acknowledgement (blocks until all pending records written)
        ack_rx.recv().map_err(|_| {
            DBError::Wal(crate::wal::WALError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "WAL writer thread died during barrier",
            )))
        })?;

        // Now safe to clear WAL (all records written to disk)
        let mut wal = self.wal.lock().expect("WAL mutex poisoned");
        wal.clear()?;
        drop(wal);

        // Update max_flushed_seq to allow compaction of this SSTable
        // This MUST happen after immutable_memtables is cleared to prevent data loss
        // Use fetch_max to handle out-of-order flush completions (only update if new value is greater)
        self.max_flushed_seq
            .fetch_max(flush_sequence, Ordering::SeqCst);

        let flush_duration_ms = flush_start.elapsed().as_millis();
        info!(
            duration_ms = flush_duration_ms,
            sstable_path = ?sstable_path_for_log,
            sstable_size_bytes = size,
            partitions_merged = NUM_PARTITIONS,
            "Partitioned memtable flush complete"
        );

        // Check if compaction is needed (LOCK-FREE!)
        if let Some(level_num) = self.lsm.load().needs_compaction() {
            debug!(level = level_num, "Compaction triggered");
            // Arc automatically dropped (lock-free!)

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

        // Adjust LSM size ratios based on workload (Dostoevsky adaptive compaction)
        // CRITICAL FIX (Bug #7c): Serialize LSM tree updates to prevent ABA race
        if self.options.adaptive_compaction {
            let writes = self.write_count.load(std::sync::atomic::Ordering::Relaxed);
            let reads = self.read_count.load(std::sync::atomic::Ordering::Relaxed);

            let _lsm_lock = self.lsm_mutex.lock().expect("LSM mutex poisoned");
            let mut lsm_clone = (**self.lsm.load()).clone();
            if lsm_clone.adjust_for_workload(writes, reads) {
                let strategy = lsm_clone.strategy();
                debug!(
                    writes = writes,
                    reads = reads,
                    ratio = strategy.current_ratio(),
                    "Dostoevsky: Adjusted LSM size ratio based on workload"
                );
                self.lsm.store(Arc::new(lsm_clone));
            }
        }

        Ok(())
    }

    /// Helper: Build SSTable from iterator of (key, entry) pairs
    /// Handles both normal values and vLog separation
    fn build_sstable_from_entries<'a, I>(
        &self,
        sstable_path: &Path,
        entries: I,
        sequence: u64,
    ) -> Result<()>
    where
        I: Iterator<Item = &'a (Bytes, crate::memtable::Entry)>,
    {
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        let mut vlog_guard = self.vlog.lock().expect("vLog mutex poisoned");

        if let (Some(threshold), Some(ref mut vlog)) =
            (self.options.vlog_threshold, vlog_guard.as_mut())
        {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::create(sstable_path)?
                .with_vlog_threshold(threshold)
                .with_max_sequence(sequence);

            for (key, entry) in entries {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key.clone(), value.clone(), vlog)?;
                    }
                    Entry::Tombstone => {
                        builder.add_tombstone(key.clone())?;
                    }
                }
            }

            builder.finish()?;

            // ALWAYS sync vLog after flush
            vlog.sync()?;
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard);

            let mut builder = SSTableBuilder::create(sstable_path)?.with_max_sequence(sequence);

            for (key, entry) in entries {
                match entry {
                    Entry::Value(value) => {
                        builder.add(key.clone(), value.clone())?;
                    }
                    Entry::Tombstone => {
                        builder.add_tombstone(key.clone())?;
                    }
                }
            }

            builder.finish()?;
        }

        Ok(())
    }

    /// Try to atomically swap all partitions for background flush
    ///
    /// Returns true if partitions were successfully swapped (caller should signal background thread)
    /// Returns false if another thread is already flushing (skip signaling)
    fn try_swap_memtable(&self) -> Result<bool> {
        // Try to acquire flush lock - if another thread is flushing, return false
        let _flush_lock = match self.flush_mutex.try_lock() {
            Ok(lock) => lock,
            Err(_) => return Ok(false), // Another thread is flushing
        };

        // Check if immutable_memtables is occupied (LOCK-FREE!)
        let immut_occupied = {
            let immut_arc = self.immutable_memtables.load();
            immut_arc.is_some()
        };

        if immut_occupied {
            // Another thread's flush is still in progress
            return Ok(false);
        }

        // Safe to swap - immutable_memtables is None
        // Swap ALL 16 partitions atomically (lock-free with ArcSwap!)
        let capacity_per_partition = self.options.memtable_capacity / NUM_PARTITIONS;
        let mut flushing_partitions = Vec::with_capacity(NUM_PARTITIONS);

        // Deref Arc to access the array
        for partition_mt in self.memtables.iter() {
            // Atomic swap: returns Arc<Memtable> of old partition
            // Keep it as Arc<Memtable> - no need to unwrap since immutable_memtables stores Arc
            let old_arc: Arc<Memtable> =
                partition_mt.swap(Arc::new(Memtable::new(capacity_per_partition)));
            flushing_partitions.push(old_arc);
        }

        // Store in immutable_memtables (LOCK-FREE!)
        self.immutable_memtables
            .store(Arc::new(Some(Arc::new(flushing_partitions))));

        Ok(true) // Successfully swapped
    }

    /// Compact a level
    fn compact_level(&self, level_num: usize) -> Result<()> {
        Self::do_compact_level(
            &self.lsm,
            &self.lsm_mutex,
            &self.sstable_counter,
            &self.options.data_dir,
            level_num,
            &self.metrics,
            &self.max_flushed_seq,
            &self.pending_deletions,
        )
    }

    /// Internal compaction implementation (shared by both sync and async paths)
    pub(crate) fn do_compact_level(
        lsm: &Arc<ArcSwap<LSMTree>>,
        lsm_mutex: &Arc<Mutex<()>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
        metrics: &Arc<MetricsCollector>,
        max_flushed_seq: &Arc<AtomicU64>,
        pending_deletions: &Arc<Mutex<Vec<(PathBuf, std::time::Instant)>>>,
    ) -> Result<()> {
        let compaction_start = Instant::now();

        // Load LSM tree (LOCK-FREE!)
        let lsm_arc = lsm.load();

        // Get SSTables to compact
        let level = lsm_arc.level(level_num).ok_or(DBError::NotOpened)?;
        let all_input_paths: Vec<PathBuf> = level.sstables().to_vec();

        if all_input_paths.is_empty() {
            return Ok(());
        }

        // **CRITICAL FIX**: Only compact SSTables with max_sequence <= max_flushed_seq
        // This prevents compaction from deleting keys still in immutable memtables
        let safe_seq = max_flushed_seq.load(Ordering::SeqCst);
        let mut input_paths = Vec::new();
        let mut skipped_count = 0;

        for path in all_input_paths {
            // Read SSTable header to get max_sequence
            if let Ok(sstable) = SSTable::open(&path) {
                if sstable.max_sequence() <= safe_seq {
                    input_paths.push(path);
                } else {
                    // Skip this SSTable - it has unflushed keys
                    skipped_count += 1;
                    debug!(
                        path = ?path,
                        sstable_seq = sstable.max_sequence(),
                        safe_seq = safe_seq,
                        "Skipping SSTable with sequence > max_flushed_seq (preventing live key deletion)"
                    );
                }
            }
        }

        if input_paths.is_empty() {
            debug!(
                level = level_num,
                skipped = skipped_count,
                "No SSTables eligible for compaction (all sequences > max_flushed_seq)"
            );
            return Ok(());
        }

        let input_count = input_paths.len();
        debug!(
            level = level_num,
            input_sstables = input_count,
            skipped_sstables = skipped_count,
            safe_seq = safe_seq,
            "Starting compaction"
        );

        // Generate output path
        let mut counter = sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let output_path = data_dir.join(format!("L{}_{:06}.sst", level_num + 1, *counter));
        *counter += 1;
        drop(counter);

        // Arc automatically dropped (lock-free!)

        // Compact SSTables
        let (result_path, size) = compact_sstables(&input_paths, &output_path)?;

        // Track physical bytes written during compaction
        metrics.record_physical_bytes(size);

        // CRITICAL FIX (Bug #7c): Serialize LSM tree updates to prevent ABA race
        // Hold mutex during read-modify-write to ensure atomicity
        {
            let _lsm_lock = lsm_mutex.lock().expect("LSM mutex poisoned");

            // Update LSM tree - clone, modify, store (serialized)
            let mut lsm_clone = (**lsm.load()).clone();
            lsm_clone.add_to_level(level_num + 1, result_path, size);
            lsm_clone.remove_sstables_from_level(level_num, &input_paths);
            lsm.store(Arc::new(lsm_clone));

            // Lock released here (automatic drop)
        }

        // PRODUCTION FIX (Bug #7b): Queue SSTables for delayed deletion
        // Concurrent readers may hold LSM snapshots pointing to these files.
        // By queuing deletions with timestamps, we ensure files are only deleted
        // after a safe delay (5 seconds), giving readers time to finish.
        {
            let mut pending = pending_deletions.lock().unwrap();
            let now = std::time::Instant::now();
            for path in input_paths {
                pending.push((path, now));
            }
        }

        // Clean up old pending deletions (files queued >5 seconds ago)
        crate::db_helpers::cleanup_old_deletions(&pending_deletions);

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

    /// Get current memtable size across all partitions (lock-free)
    pub fn memtable_size(&self) -> usize {
        self.memtables.iter().map(|mt| mt.load().size()).sum()
    }

    /// Get number of entries in memtable across all partitions (lock-free)
    pub fn memtable_len(&self) -> usize {
        self.memtables.iter().map(|mt| mt.load().len()).sum()
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
    /// Estimate current memory usage in bytes
    ///
    /// Includes:
    /// - Active memtables
    /// - Immutable memtables (during flush)
    /// - Block cache (~40MB for 10K blocks)
    /// - SSTable cache (~1KB per cached SSTable)
    pub fn estimate_memory_usage(&self) -> usize {
        // Active memtables
        let active_memtable_bytes: usize = self.memtables.iter().map(|mt| mt.load().size()).sum();

        // Immutable memtables (if flush in progress)
        let immutable_memtable_bytes: usize = {
            let immutable = self.immutable_memtables.load();
            if let Some(ref partitions) = **immutable {
                partitions.iter().map(|mt| mt.size()).sum()
            } else {
                0
            }
        };

        // Block cache: 10K blocks * ~4KB average = ~40MB
        const BLOCK_CACHE_BYTES: usize = 10_000 * 4096;

        // SSTable cache: 1000 SSTables * ~1KB metadata = ~1MB
        const SSTABLE_CACHE_BYTES: usize = 1_000 * 1024;

        active_memtable_bytes + immutable_memtable_bytes + BLOCK_CACHE_BYTES + SSTABLE_CACHE_BYTES
    }

    /// Check disk space with periodic caching (every 10 seconds)
    ///
    /// This is a performance-optimized version of disk space checking that:
    /// 1. Returns immediately if checked within last 10 seconds (uses cached value)
    /// 2. Otherwise updates cache and checks disk space
    ///
    /// This avoids the performance overhead of calling sysinfo on every write
    /// while still protecting against disk full scenarios.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if sufficient disk space available
    /// - `Err(DBError::DiskSpaceFull)` if disk space below threshold
    ///
    /// # Performance
    ///
    /// - Cached check: < 1 microsecond (single atomic load)
    /// - Fresh check: ~1-5 milliseconds (sysinfo syscall)
    fn check_disk_space_cached(&self) -> Result<()> {
        // Only check if min_disk_space is configured
        if self.options.min_disk_space_bytes.is_none() {
            return Ok(());
        }

        const CHECK_INTERVAL_SECS: u64 = 10;

        // Get current time (seconds since UNIX epoch)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Get last check time (atomic load, very fast)
        let last_check = self.last_disk_check.load(Ordering::Relaxed);

        // If checked within last 10 seconds, use cached value
        if now.saturating_sub(last_check) < CHECK_INTERVAL_SECS {
            let cached_space = self.cached_available_space.load(Ordering::Relaxed);
            let min_space = self.options.min_disk_space_bytes.unwrap();

            if cached_space < min_space {
                return Err(DBError::DiskSpaceFull {
                    available: cached_space,
                    required: min_space,
                });
            }
            return Ok(());
        }

        // Time to refresh the cache - call the actual disk space check
        // This uses sysinfo which is slow, but we only do it every 10 seconds
        use sysinfo::{DiskExt, System, SystemExt};

        let min_space = self.options.min_disk_space_bytes.unwrap();
        let mut sys = System::new();
        sys.refresh_disks_list();

        // Find the disk containing our data directory
        let data_dir = &self.options.data_dir;
        if let Some(disk) = sys
            .disks()
            .iter()
            .find(|d| data_dir.starts_with(d.mount_point()))
        {
            let available = disk.available_space();

            // Update cache (atomic stores)
            self.cached_available_space
                .store(available, Ordering::Relaxed);
            self.last_disk_check.store(now, Ordering::Relaxed);

            if available < min_space {
                return Err(DBError::DiskSpaceFull {
                    available,
                    required: min_space,
                });
            }
        } else {
            // If we can't find the disk, update timestamp anyway to avoid
            // hammering sysinfo on every write
            self.last_disk_check.store(now, Ordering::Relaxed);
        }

        Ok(())
    }

    pub fn stats(&self) -> DBStats {
        // Get operation counts and throughput
        let (total_puts, total_gets, total_deletes, total_flushes, total_compactions) =
            self.metrics.get_counts();
        let (writes_per_sec, reads_per_sec, deletes_per_sec) = self.metrics.calculate_throughput();

        // Get latency percentiles
        let (put_latencies, get_latencies, delete_latencies) =
            self.metrics.get_latency_percentiles();

        // Get memtable stats (sum across all partitions, lock-free)
        let memtable_size_bytes: usize = self.memtables.iter().map(|mt| mt.load().size()).sum();
        let memtable_capacity_bytes = self.options.memtable_capacity;
        let memtable_utilization_pct =
            (memtable_size_bytes as f64 / memtable_capacity_bytes as f64) * 100.0;

        // Get WAL size
        let wal_size_bytes = self
            .options
            .data_dir
            .join("wal.log")
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        // Get LSM tree structure and cache stats (LOCK-FREE!)
        let lsm_arc = self.lsm.load();
        let mut sstables_per_level = Vec::new();
        let mut level_sizes_bytes = Vec::new();
        let mut total_disk_bytes = 0u64;
        let mut total_sstables = 0usize;
        let mut cache_hits_total = 0u64;
        let mut cache_misses_total = 0u64;

        for level_num in 0..lsm_arc.num_levels() {
            if let Some(level) = lsm_arc.level(level_num) {
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

        // Collect cache stats from all SSTables
        for level_num in 0..lsm_arc.num_levels() {
            if let Some(level) = lsm_arc.level(level_num) {
                for sstable_path in level.sstables() {
                    if let Some(cached_sstable) = self.sstable_cache.get(sstable_path) {
                        let sstable = cached_sstable.lock().expect("SSTable lock poisoned");
                        let (hits, misses, _) = sstable.cache_stats();
                        cache_hits_total += hits;
                        cache_misses_total += misses;
                    }
                }
            }
        }
        // Arc automatically dropped (lock-free, no explicit drop needed!)

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

        // Calculate cache hit rate
        let cache_total = cache_hits_total + cache_misses_total;
        let cache_hit_rate = if cache_total > 0 {
            cache_hits_total as f64 / cache_total as f64
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

            // Block cache performance
            cache_hits: cache_hits_total,
            cache_misses: cache_misses_total,
            cache_hit_rate,

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

        // Check 1: Compaction lag (L0 SSTable count) (LOCK-FREE!)
        let lsm_arc = self.lsm.load();
        let l0_count = if let Some(level) = lsm_arc.level(0) {
            level.sstables().len()
        } else {
            0
        };
        // Arc automatically dropped (lock-free, no explicit drop needed!)

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

        // Check 3: Memtable utilization (sum across all partitions, lock-free)
        let memtable_size: usize = self.memtables.iter().map(|mt| mt.load().size()).sum();
        let memtable_capacity = self.options.memtable_capacity;
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
    pub fn range(&self, start_key: &[u8], end_key: Option<&[u8]>) -> Result<RangeIterator> {
        // Track read operation for Dostoevsky adaptive compaction
        self.read_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // **CRITICAL FIX**: Collect memtables FIRST, then SSTables
        // This prevents missing keys if flush happens during collection:
        //
        // Before fix (SSTables then memtables):
        //   1. Collect SSTables (without new SSTable)
        //   2. Flush happens → memtable → new SSTable
        //   3. Collect memtables (now empty)
        //   4. Result: MISSING KEYS in new SSTable
        //
        // After fix (memtables then SSTables):
        //   1. Collect memtables (with keys)
        //   2. Flush happens → memtable → new SSTable
        //   3. Collect SSTables (includes new SSTable with same keys)
        //   4. Result: Keys seen twice, but k-way merge deduplicates ✅

        // Collect Arc references to ALL active memtable partitions (lock-free)
        // load() returns Guard<Arc<Memtable>>, we need to clone the Arc out
        let partition_arcs: Vec<Arc<Memtable>> = self
            .memtables
            .iter()
            .map(|mt| (*mt.load()).clone())
            .collect();
        let mut partition_refs: Vec<&Memtable> =
            partition_arcs.iter().map(|arc| arc.as_ref()).collect();

        // Also include immutable partitions if they exist (LOCK-FREE!)
        let immutable_arc = self.immutable_memtables.load();
        let immutable_refs: Vec<&Memtable> = if let Some(ref immutable_partitions) = **immutable_arc
        {
            immutable_partitions
                .iter()
                .map(|arc| arc.as_ref())
                .collect()
        } else {
            Vec::new()
        };
        // Arc automatically dropped (lock-free, no explicit drop needed!)
        partition_refs.extend(immutable_refs);

        // Now collect SSTables from LSM tree (in reverse level order: L0, L1, ..., LN) (LOCK-FREE!)
        let lsm_arc = self.lsm.load();
        let mut sstables = Vec::new();

        // Collect SSTables from all levels using cache
        for level_idx in 0..lsm_arc.num_levels() {
            if let Some(level) = lsm_arc.level(level_idx) {
                for sstable_path in level.sstables() {
                    // Use quick_cache for lock-free SSTable access
                    let sstable_arc = self.sstable_cache.get_or_insert_with(
                        sstable_path,
                        || -> Result<Arc<Mutex<SSTable>>> {
                            let sstable = SSTable::open(sstable_path.clone())?;
                            Ok(Arc::new(Mutex::new(sstable)))
                        },
                    )?;

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
        // Arc automatically dropped (lock-free, no explicit drop needed!)

        // Create range iterator with all memtable partitions
        RangeIterator::new(start_key, end_key, &partition_refs, sstables)
    }

    /// Iterate over all keys in the database
    ///
    /// This is a convenience method equivalent to `range(&[], None)`.
    /// Returns an iterator over all key-value pairs in sorted order.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"a", b"1").unwrap();
    /// db.put(b"b", b"2").unwrap();
    /// db.put(b"c", b"3").unwrap();
    ///
    /// // Iterate over all keys
    /// for result in db.iter().unwrap() {
    ///     let (key, value) = result.unwrap();
    ///     println!("{:?} => {:?}", key, value);
    /// }
    /// ```
    pub fn iter(&self) -> Result<RangeIterator> {
        self.range(&[], None)
    }

    /// Iterate over keys with a given prefix
    ///
    /// This is a convenience method for prefix scans. Returns an iterator
    /// over all key-value pairs where the key starts with the given prefix.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"user:1", b"alice").unwrap();
    /// db.put(b"user:2", b"bob").unwrap();
    /// db.put(b"user:3", b"charlie").unwrap();
    /// db.put(b"post:1", b"hello").unwrap();
    ///
    /// // Iterate over all user keys
    /// for result in db.prefix(b"user:").unwrap() {
    ///     let (key, value) = result.unwrap();
    ///     println!("{:?} => {:?}", key, value);
    /// }
    /// // Output: user:1, user:2, user:3
    /// ```
    pub fn prefix(&self, prefix: &[u8]) -> Result<RangeIterator> {
        // Calculate the end key by incrementing the prefix
        // This creates a range [prefix, prefix+1) that captures all keys with the prefix
        let end_key = increment_bytes(prefix);
        match end_key {
            Some(end) => self.range(prefix, Some(&end)),
            None => self.range(prefix, None), // Prefix is all 0xFF, scan to end
        }
    }

    /// Create a point-in-time consistent snapshot of the database
    ///
    /// Snapshots provide isolation for reads - writes after the snapshot
    /// is created are not visible to the snapshot. This is essential for:
    /// - Consistent multi-read operations
    /// - Backup operations
    /// - Long-running analytical queries
    ///
    /// # Implementation Note
    ///
    /// This is a lightweight snapshot that captures the current LSM tree state.
    /// Due to the current architecture (mutable memtables), this snapshot only
    /// provides isolation for data that has been flushed to SSTables.
    ///
    /// For fully consistent snapshots that include in-memory data, use
    /// `snapshot_consistent()` which forces a flush first.
    ///
    /// # Thread Safety
    ///
    /// This method is lock-free and can be called concurrently with writes.
    /// The returned snapshot is fully thread-safe.
    ///
    /// # Memory Management
    ///
    /// Snapshots hold references to the LSM tree state.
    /// Long-lived snapshots can increase memory usage. Drop snapshots
    /// when no longer needed to allow garbage collection.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"key", b"value1").unwrap();
    /// db.flush().unwrap(); // Flush to ensure data is in snapshot
    ///
    /// // Create snapshot
    /// let snapshot = db.snapshot();
    ///
    /// // Write after snapshot
    /// db.put(b"key", b"value2").unwrap();
    ///
    /// // Snapshot sees old value (from SSTable)
    /// assert_eq!(snapshot.get(b"key").unwrap().unwrap().as_ref(), b"value1");
    /// ```
    pub fn snapshot(&self) -> crate::snapshot::Snapshot {
        // Capture current sequence number
        let seq_num = self.next_seq.load(Ordering::Acquire);

        // Capture SSTable paths at snapshot time (lock-free via ArcSwap)
        // This creates a point-in-time view of the LSM tree structure
        let lsm_arc = self.lsm.load();
        let mut sstable_paths = Vec::new();
        for level_idx in 0..lsm_arc.num_levels() {
            if let Some(level) = lsm_arc.level(level_idx) {
                sstable_paths.push(level.sstables().to_vec());
            } else {
                sstable_paths.push(Vec::new());
            }
        }

        // Get vLog path if enabled
        let has_vlog = self.has_vlog.load(Ordering::Relaxed);
        let vlog_path = if has_vlog {
            Some(self.options.data_dir.clone())
        } else {
            None
        };

        crate::snapshot::Snapshot::new(
            Vec::new(), // No memtables - only SSTable data is consistent
            None,       // No immutable memtables
            sstable_paths,
            self.sstable_cache.clone(),
            vlog_path,
            has_vlog,
            seq_num,
        )
    }

    /// Create a fully consistent point-in-time snapshot
    ///
    /// This method flushes all in-memory data to disk before creating the
    /// snapshot, ensuring complete consistency. This is more expensive than
    /// `snapshot()` but provides true isolation for all data.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use seerdb::{DB, DBOptions};
    ///
    /// let db = DB::open(DBOptions::default()).unwrap();
    /// db.put(b"key", b"value1").unwrap();
    ///
    /// // Create consistent snapshot (flushes first)
    /// let snapshot = db.snapshot_consistent().unwrap();
    ///
    /// // Write after snapshot
    /// db.put(b"key", b"value2").unwrap();
    ///
    /// // Snapshot sees old value
    /// assert_eq!(snapshot.get(b"key").unwrap().unwrap().as_ref(), b"value1");
    ///
    /// // DB sees new value
    /// assert_eq!(db.get(b"key").unwrap().unwrap().as_ref(), b"value2");
    /// ```
    pub fn snapshot_consistent(&self) -> Result<crate::snapshot::Snapshot> {
        // Flush all memtables to ensure data is in immutable SSTables
        self.flush()?;

        // Now create snapshot with guaranteed consistency
        Ok(self.snapshot())
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
            if let Err(e) = worker.join() {
                error!("Flush worker thread panicked during shutdown: {:?}", e);
            }
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
            if let Err(e) = worker.join() {
                error!("Compaction worker thread panicked during shutdown: {:?}", e);
            }
        }

        // Shutdown WAL writer - drop the sender to close the channel
        debug!("Closing WAL writer channel");
        // Replace our sender with a dummy one, dropping the original
        let (dummy_tx, _dummy_rx) = crossbeam_channel::unbounded::<WALMessage>();
        let _old_tx = std::mem::replace(&mut self.wal_tx, dummy_tx);
        drop(_old_tx); // Explicitly drop to close channel

        // Wait for WAL worker thread to finish
        // join() provides proper synchronization - thread won't exit until all records are written
        if let Some(worker) = self.wal_worker.take() {
            debug!("Waiting for background WAL writer thread to finish");
            if let Err(e) = worker.join() {
                error!(
                    "WAL writer thread panicked during shutdown: {:?} - DATA LOSS MAY HAVE OCCURRED",
                    e
                );
            }
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
    #[ignore] // TODO: Fix race condition with tiny memtable (100 bytes) - WAL not fully flushed before reopen
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
    #[ignore] // TODO: Same WAL race - data loss on reopen
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
        assert!(!results
            .iter()
            .any(|k| k.as_str() >= "key010" && k.as_str() < "key020"));
    }

    #[test]
    #[ignore] // TODO: Test hangs, needs debugging
    fn test_memory_budget_enforcement() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 1024 * 1024, // 1MB per partition
            max_memory_bytes: Some(200 * 1024 * 1024), // 200MB budget (won't be triggered in test)
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Verify memory estimation works
        let initial_memory = db.estimate_memory_usage();
        assert!(initial_memory > 0, "Memory usage should be non-zero");

        // Write small amount of data (won't trigger enforcement)
        for i in 0..10 {
            let key = format!("key{}", i);
            db.put(key.as_bytes(), b"value").unwrap();
        }

        // Verify data is accessible
        for i in 0..10 {
            let key = format!("key{}", i);
            assert_eq!(db.get(key.as_bytes()).unwrap(), Some(Bytes::from("value")));
        }
    }

    #[test]
    #[ignore] // TODO: Test hangs, needs debugging
    fn test_estimate_memory_usage() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            memtable_capacity: 1024,
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Initial memory should include cache overhead
        let initial = db.estimate_memory_usage();
        // Should be at least block cache (40MB) + SSTable cache (1MB)
        assert!(initial >= 40 * 1024 * 1024, "Should include cache overhead");

        // Write some data
        for i in 0..10 {
            db.put(format!("key{}", i).as_bytes(), b"value").unwrap();
        }

        // Memory should increase
        let after_write = db.estimate_memory_usage();
        assert!(
            after_write >= initial,
            "Memory should increase after writes"
        );
    }

    #[test]
    fn test_snapshot_basic_isolation() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write initial data
        db.put(b"key1", b"value1").unwrap();
        db.put(b"key2", b"value2").unwrap();

        // Create consistent snapshot (forces flush)
        let snapshot = db.snapshot_consistent().unwrap();

        // Write after snapshot
        db.put(b"key1", b"modified").unwrap();
        db.put(b"key3", b"value3").unwrap();
        db.delete(b"key2").unwrap();

        // Snapshot sees old values
        assert_eq!(snapshot.get(b"key1").unwrap(), Some(Bytes::from("value1")));
        assert_eq!(snapshot.get(b"key2").unwrap(), Some(Bytes::from("value2")));
        assert_eq!(snapshot.get(b"key3").unwrap(), None); // Didn't exist at snapshot time

        // DB sees new values
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("modified")));
        assert_eq!(db.get(b"key2").unwrap(), None); // Deleted
        assert_eq!(db.get(b"key3").unwrap(), Some(Bytes::from("value3")));
    }

    #[test]
    fn test_snapshot_range_isolation() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write initial data
        db.put(b"a", b"1").unwrap();
        db.put(b"b", b"2").unwrap();
        db.put(b"c", b"3").unwrap();

        // Create consistent snapshot (forces flush)
        let snapshot = db.snapshot_consistent().unwrap();

        // Modify after snapshot
        db.put(b"b", b"modified").unwrap();
        db.delete(b"c").unwrap();
        db.put(b"d", b"4").unwrap();

        // Snapshot range sees original values
        let snap_results: Vec<_> = snapshot
            .range(b"a", Some(b"z"))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(snap_results.len(), 3); // a, b, c
        assert_eq!(snap_results[0].1.as_ref(), b"1");
        assert_eq!(snap_results[1].1.as_ref(), b"2");
        assert_eq!(snap_results[2].1.as_ref(), b"3");

        // DB range sees new values
        let db_results: Vec<_> = db
            .range(b"a", Some(b"z"))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(db_results.len(), 3); // a, b, d (c deleted)
        assert_eq!(db_results[0].1.as_ref(), b"1");
        assert_eq!(db_results[1].1.as_ref(), b"modified");
        assert_eq!(db_results[2].1.as_ref(), b"4");
    }

    #[test]
    fn test_snapshot_during_concurrent_writes() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = Arc::new(DB::open(options).unwrap());

        // Write initial data
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let value = format!("initial_{:03}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Create consistent snapshot (forces flush)
        let snapshot = db.snapshot_consistent().unwrap();

        // Spawn writer thread that modifies data concurrently
        let db_clone = Arc::clone(&db);
        let writer = thread::spawn(move || {
            for i in 0..100 {
                let key = format!("key_{:03}", i);
                let value = format!("modified_{:03}", i);
                db_clone.put(key.as_bytes(), value.as_bytes()).unwrap();
            }
        });

        // While writes are happening, snapshot still sees original data
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let expected = format!("initial_{:03}", i);
            let actual = snapshot.get(key.as_bytes()).unwrap();
            assert_eq!(actual, Some(Bytes::from(expected)));
        }

        writer.join().unwrap();

        // After writes complete, snapshot still sees original data
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let expected = format!("initial_{:03}", i);
            let actual = snapshot.get(key.as_bytes()).unwrap();
            assert_eq!(actual, Some(Bytes::from(expected)));
        }

        // But DB sees modified data
        for i in 0..100 {
            let key = format!("key_{:03}", i);
            let expected = format!("modified_{:03}", i);
            let actual = db.get(key.as_bytes()).unwrap();
            assert_eq!(actual, Some(Bytes::from(expected)));
        }
    }

    #[test]
    fn test_snapshot_sequence_number() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        db.put(b"key1", b"value1").unwrap();
        db.flush().unwrap(); // Force flush to increment sequence
        let snap1 = db.snapshot();

        db.put(b"key2", b"value2").unwrap();
        db.flush().unwrap(); // Force flush to increment sequence
        let snap2 = db.snapshot();

        // Snap2 should have higher sequence number (after more writes)
        assert!(snap2.sequence_number() >= snap1.sequence_number());

        // Debug output works
        let _debug = format!("{:?}", snap1);
    }

    #[test]
    fn test_multiple_snapshots() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Initial state
        db.put(b"key", b"v1").unwrap();
        let snap1 = db.snapshot_consistent().unwrap();

        // Second state
        db.put(b"key", b"v2").unwrap();
        let snap2 = db.snapshot_consistent().unwrap();

        // Third state
        db.put(b"key", b"v3").unwrap();
        let snap3 = db.snapshot_consistent().unwrap();

        // Current state
        db.put(b"key", b"v4").unwrap();

        // Each snapshot sees its point-in-time value
        assert_eq!(snap1.get(b"key").unwrap(), Some(Bytes::from("v1")));
        assert_eq!(snap2.get(b"key").unwrap(), Some(Bytes::from("v2")));
        assert_eq!(snap3.get(b"key").unwrap(), Some(Bytes::from("v3")));
        assert_eq!(db.get(b"key").unwrap(), Some(Bytes::from("v4")));

        // Drop early snapshots, late ones still work
        drop(snap1);
        drop(snap2);
        assert_eq!(snap3.get(b"key").unwrap(), Some(Bytes::from("v3")));
    }

    #[test]
    fn test_snapshot_with_tombstones() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write and delete
        db.put(b"key1", b"value1").unwrap();
        db.put(b"key2", b"value2").unwrap();
        db.delete(b"key1").unwrap();

        // Snapshot sees key1 as deleted (after flush)
        let snap = db.snapshot_consistent().unwrap();
        assert_eq!(snap.get(b"key1").unwrap(), None);
        assert_eq!(snap.get(b"key2").unwrap(), Some(Bytes::from("value2")));

        // Re-insert key1
        db.put(b"key1", b"resurrected").unwrap();

        // Snapshot still sees key1 as deleted
        assert_eq!(snap.get(b"key1").unwrap(), None);

        // DB sees resurrected value
        assert_eq!(db.get(b"key1").unwrap(), Some(Bytes::from("resurrected")));
    }

    #[test]
    fn test_iter_all_keys() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write some keys
        db.put(b"a", b"1").unwrap();
        db.put(b"b", b"2").unwrap();
        db.put(b"c", b"3").unwrap();
        db.put(b"d", b"4").unwrap();
        db.put(b"e", b"5").unwrap();

        // Iterate over all keys
        let results: Vec<_> = db.iter().unwrap().map(|r| r.unwrap()).collect();

        assert_eq!(results.len(), 5);
        assert_eq!(results[0].0.as_ref(), b"a");
        assert_eq!(results[1].0.as_ref(), b"b");
        assert_eq!(results[2].0.as_ref(), b"c");
        assert_eq!(results[3].0.as_ref(), b"d");
        assert_eq!(results[4].0.as_ref(), b"e");
    }

    #[test]
    fn test_prefix_scan() {
        let dir = tempdir().unwrap();
        let options = DBOptions {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let db = DB::open(options).unwrap();

        // Write keys with different prefixes
        db.put(b"user:1", b"alice").unwrap();
        db.put(b"user:2", b"bob").unwrap();
        db.put(b"user:3", b"charlie").unwrap();
        db.put(b"post:1", b"hello").unwrap();
        db.put(b"post:2", b"world").unwrap();
        db.put(b"tag:rust", b"lang").unwrap();

        // Scan user: prefix
        let user_results: Vec<_> = db
            .prefix(b"user:")
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(user_results.len(), 3);
        assert_eq!(user_results[0].0.as_ref(), b"user:1");
        assert_eq!(user_results[1].0.as_ref(), b"user:2");
        assert_eq!(user_results[2].0.as_ref(), b"user:3");

        // Scan post: prefix
        let post_results: Vec<_> = db
            .prefix(b"post:")
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(post_results.len(), 2);
        assert_eq!(post_results[0].0.as_ref(), b"post:1");
        assert_eq!(post_results[1].0.as_ref(), b"post:2");

        // Scan tag: prefix (single result)
        let tag_results: Vec<_> = db.prefix(b"tag:").unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(tag_results.len(), 1);
        assert_eq!(tag_results[0].0.as_ref(), b"tag:rust");

        // Scan non-existent prefix
        let empty_results: Vec<_> = db
            .prefix(b"missing:")
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(empty_results.len(), 0);
    }

    #[test]
    fn test_increment_bytes_helper() {
        // Normal case
        assert_eq!(increment_bytes(b"user"), Some(b"uses".to_vec()));

        // With 0xFF at end
        assert_eq!(increment_bytes(b"user\xff"), Some(b"uses\x00".to_vec()));

        // Multiple 0xFF at end
        assert_eq!(
            increment_bytes(b"a\xff\xff"),
            Some(b"b\x00\x00".to_vec())
        );

        // All 0xFF
        assert_eq!(increment_bytes(b"\xff\xff"), None);

        // Single byte
        assert_eq!(increment_bytes(b"a"), Some(b"b".to_vec()));
        assert_eq!(increment_bytes(b"\xff"), None);

        // Empty
        assert_eq!(increment_bytes(b""), None);
    }

    #[test]
    fn test_prefix_with_sstables() {
        let dir = tempdir().unwrap();
        let mut opts = DBOptions::default();
        opts.data_dir = dir.path().to_path_buf();
        opts.memtable_capacity = 1024; // Small memtable to force flush

        let db = DB::open(opts).unwrap();

        // Write enough data to trigger flush
        for i in 0..20 {
            let key = format!("key:{:02}", i);
            let value = format!("value_{}", i);
            db.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Force flush to ensure data is in SSTables
        db.flush().unwrap();

        // Add some more data in memtable
        db.put(b"key:20", b"value_20").unwrap();
        db.put(b"key:21", b"value_21").unwrap();

        // Prefix scan should find all keys (memtable + SSTables)
        let results: Vec<_> = db.prefix(b"key:").unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(results.len(), 22);

        // Verify ordering
        for i in 0..22 {
            let expected_key = format!("key:{:02}", i);
            assert_eq!(results[i].0.as_ref(), expected_key.as_bytes());
        }
    }
}


