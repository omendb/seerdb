// Main database interface
// Integrates WAL, Memtable, SSTable, Compaction, and VLog

use crate::compaction::{compact_sstables, LSMTree};
use crate::health::{HealthCheck, HealthStatus};
use crate::memtable::Memtable;
use crate::metrics::{DBStats, MetricsCollector};
use crate::sstable::SSTable;
use crate::vlog::VLog;
use crate::wal::{Record, SyncPolicy, WALReader, WAL};
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error, info, warn};

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

/// Main database configuration
#[derive(Debug, Clone)]
pub struct DBOptions {
    /// Directory for database files
    pub data_dir: PathBuf,
    /// Memtable capacity in bytes (default: 64MB)
    pub memtable_capacity: usize,
    /// WAL sync policy (default: SyncData)
    pub wal_sync_policy: SyncPolicy,
    /// LSM base level size (default: 10MB)
    pub base_level_size: u64,
    /// LSM size ratio between levels (default: 10)
    pub size_ratio: u64,
    /// Number of LSM levels (default: 7)
    pub num_levels: usize,
    /// VLog threshold: values larger than this go to vLog (default: None = disabled)
    /// Set to Some(4096) for 4KB threshold (good for embeddings)
    pub vlog_threshold: Option<usize>,
    /// Enable background compaction (default: false for compatibility)
    /// When true, compaction runs in background thread (non-blocking writes)
    pub background_compaction: bool,
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
            vlog_threshold: None,         // Disabled by default
            background_compaction: false, // Disabled by default for compatibility
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

/// Main database structure
pub struct DB {
    /// Database options
    options: DBOptions,
    /// Write-ahead log
    wal: Arc<Mutex<WAL>>,
    /// Active memtable (Mutex allows swapping after flush)
    memtable: Arc<Mutex<Memtable>>,
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
}

impl DB {
    /// Open or create a database
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

        // Create memtable
        let memtable = Memtable::new(options.memtable_capacity);

        // Recover from WAL if it exists
        if wal_path.exists() {
            info!("Recovering from WAL");
            let entries = memtable.len();
            Self::recover(&wal_path, &memtable)?;
            let recovered = memtable.len() - entries;
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

        let lsm = Arc::new(Mutex::new(lsm));
        let sstable_counter = Arc::new(Mutex::new(0));

        // Start background compaction worker if enabled
        let (compaction_tx, compaction_worker) = if options.background_compaction {
            let (tx, rx) = channel::<CompactionTask>();

            // Clone references for worker thread
            let lsm_clone = Arc::clone(&lsm);
            let sstable_counter_clone = Arc::clone(&sstable_counter);
            let data_dir = options.data_dir.clone();

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

        let db = Self {
            options: options.clone(),
            wal: Arc::new(Mutex::new(wal)),
            memtable: Arc::new(Mutex::new(memtable)),
            lsm,
            vlog: Arc::new(Mutex::new(vlog)),
            sstable_counter,
            metrics: Arc::new(MetricsCollector::new()),
            compaction_tx,
            compaction_worker,
        };

        // Flush memtable if it filled up during recovery
        let should_flush = db
            .memtable
            .lock()
            .expect("Memtable lock poisoned")
            .should_flush();
        if should_flush {
            info!("Memtable full after recovery, flushing");
            db.flush()?;
        }

        info!("Database opened successfully");

        Ok(db)
    }

    /// Recover memtable from WAL
    ///
    /// Reads records one by one and stops gracefully if corruption or truncation is encountered.
    /// This ensures we recover all valid records before the corruption point.
    fn recover(wal_path: &Path, memtable: &Memtable) -> Result<()> {
        let mut reader =
            WALReader::open(wal_path).map_err(|e| DBError::Io(std::io::Error::other(e)))?;

        // Read records one by one, stop gracefully on error (corruption/truncation)
        loop {
            match reader.read_next() {
                Ok(Some(record)) => match record {
                    Record::Put { key, value } => {
                        memtable.put(key, value);
                    }
                    Record::Delete { key } => {
                        memtable.delete(key);
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

    /// Put a key-value pair
    pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
        let start = Instant::now();

        let key = Bytes::copy_from_slice(key.as_ref());
        let value = Bytes::copy_from_slice(value.as_ref());

        // Write to WAL first (durability)
        let record = Record::Put {
            key: key.clone(),
            value: value.clone(),
        };
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

        // Write to memtable
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        mt.put(key, value);
        let should_flush = mt.should_flush();
        drop(mt); // Release lock

        // Check if memtable should be flushed
        if should_flush {
            self.flush()?;
        }

        // Record latency
        self.metrics.record_put(start.elapsed());

        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Bytes>> {
        let start = Instant::now();
        let key = key.as_ref();

        // Check memtable first (most recent data)
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        let result = mt.get(key);
        drop(mt); // Release lock
        if let Some(value) = result {
            self.metrics.record_get(start.elapsed());
            return Ok(Some(value));
        }

        // Get vLog if available (need to clone for SSTable attachment)
        let vlog_path = self.options.data_dir.join("values.vlog");
        let has_vlog = self.vlog.lock().expect("vLog mutex poisoned").is_some();

        // Check SSTables in LSM tree (L0 -> L6)
        let lsm = self.lsm.lock().expect("LSM mutex poisoned");
        for level_num in 0..lsm.num_levels() {
            if let Some(level) = lsm.level(level_num) {
                // Check each SSTable in this level
                for sstable_path in level.sstables() {
                    let mut sstable = if has_vlog {
                        // Attach vLog for reading value pointers
                        let vlog = VLog::open(&vlog_path)?;
                        SSTable::open(sstable_path)?.with_vlog(vlog)
                    } else {
                        SSTable::open(sstable_path)?
                    };

                    if let Some(value) = sstable.get(key)? {
                        self.metrics.record_get(start.elapsed());
                        return Ok(Some(value));
                    }
                }
            }
        }

        self.metrics.record_get(start.elapsed());
        Ok(None)
    }

    /// Delete a key
    pub fn delete(&self, key: impl AsRef<[u8]>) -> Result<()> {
        let start = Instant::now();
        let key = Bytes::copy_from_slice(key.as_ref());

        // Write to WAL (durability)
        let record = Record::Delete { key: key.clone() };
        self.wal
            .lock()
            .expect("WAL mutex poisoned")
            .write(&record)?;

        // Write tombstone to memtable
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        mt.delete(key);
        let should_flush = mt.should_flush();
        drop(mt); // Release lock

        // Check if memtable should be flushed
        if should_flush {
            self.flush()?;
        }

        // Record latency
        self.metrics.record_delete(start.elapsed());

        Ok(())
    }

    /// Flush memtable to L0 SSTable
    pub fn flush(&self) -> Result<()> {
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        let flush_start = Instant::now();
        let mt_size_before = self.memtable.lock().expect("Memtable lock poisoned").size();

        info!(
            memtable_size_bytes = mt_size_before,
            "Starting memtable flush"
        );

        // Generate SSTable filename
        let mut counter = self
            .sstable_counter
            .lock()
            .expect("SSTable counter mutex poisoned");
        let sstable_path = self
            .options
            .data_dir
            .join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Build SSTable with optional vLog support
        let mt = self.memtable.lock().expect("Memtable lock poisoned");
        let mut vlog_guard = self.vlog.lock().expect("vLog mutex poisoned");

        let _sstable = if let (Some(threshold), Some(ref mut vlog)) =
            (self.options.vlog_threshold, vlog_guard.as_mut())
        {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::new().with_vlog_threshold(threshold);

            for (key, entry) in mt.iter() {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key, value, vlog)?;
                    }
                    Entry::Tombstone => {
                        // Skip tombstones during flush
                    }
                }
            }

            builder.build(&sstable_path)?
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard); // Release lock
            mt.flush(&sstable_path)?
        };
        drop(mt); // Release memtable lock

        let size = std::fs::metadata(&sstable_path)?.len();

        // Add to LSM tree L0
        let mut lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let sstable_path_for_log = sstable_path.clone();
        lsm.add_l0_sstable(sstable_path, size);

        // Clear WAL after successful flush
        // Data is now safely persisted in SSTable
        let mut wal = self.wal.lock().expect("WAL mutex poisoned");
        wal.clear()?;
        drop(wal);

        // **CRITICAL FIX**: Replace memtable with a new empty one to free memory
        let mut mt_guard = self.memtable.lock().expect("Memtable lock poisoned");
        *mt_guard = Memtable::new(self.options.memtable_capacity);
        drop(mt_guard);

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

    /// Compact a level
    fn compact_level(&self, level_num: usize) -> Result<()> {
        Self::do_compact_level(
            &self.lsm,
            &self.sstable_counter,
            &self.options.data_dir,
            level_num,
        )
    }

    /// Internal compaction implementation (shared by both sync and async paths)
    fn do_compact_level(
        lsm: &Arc<Mutex<LSMTree>>,
        sstable_counter: &Arc<Mutex<u64>>,
        data_dir: &Path,
        level_num: usize,
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
    ) -> Result<()> {
        Self::do_compact_level(lsm, sstable_counter, data_dir, level_num)
    }

    /// Get current memtable size
    pub fn memtable_size(&self) -> usize {
        self.memtable.lock().expect("Memtable lock poisoned").size()
    }

    /// Get number of entries in memtable
    pub fn memtable_len(&self) -> usize {
        self.memtable.lock().expect("Memtable lock poisoned").len()
    }

    /// Get database statistics for monitoring and observability
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

            // Uptime
            uptime_seconds: self.metrics.uptime_seconds(),
        }
    }

    /// Get database health status for monitoring
    ///
    /// Performs various health checks to detect degraded performance or critical conditions.
    /// Returns a HealthStatus with individual check results.
    ///
    /// Health check thresholds:
    /// - Compaction lag: L0 >10 SSTables = degraded, >20 = unhealthy
    /// - WAL size: >100MB = degraded, >500MB = unhealthy
    /// - Memtable: >80% full = degraded, >95% = unhealthy
    /// - Put latency p99: >100ms = degraded, >1s = unhealthy
    /// - Get latency p99: >50ms = degraded, >500ms = unhealthy
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
}

/// Graceful shutdown: signal compaction thread to stop and wait for it
impl Drop for DB {
    fn drop(&mut self) {
        info!("Closing database");

        if let Some(ref tx) = self.compaction_tx {
            // Send shutdown signal
            debug!("Signaling background compaction thread to shut down");
            let _ = tx.send(CompactionTask::Shutdown);
        }

        // Wait for worker thread to finish
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
}
