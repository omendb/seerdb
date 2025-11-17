// Background worker threads for async operations
//
// This module contains the background worker implementation for:
// - WAL writer: Batches writes to the write-ahead log
// - Flush worker: Converts memtables to SSTables
// - Compaction worker: Merges SSTables to reduce read amplification

use crate::compaction::LSMTree;
use crate::memtable::{Entry, Memtable};
use crate::metrics::MetricsCollector;
use crate::sstable::SSTableBuilder;
use crate::vlog::VLog;
use crate::wal::{Record, WAL};
use arc_swap::ArcSwap;
use bytes::Bytes;
use crossbeam_channel::{unbounded, Sender as CrossbeamSender};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tracing::{debug, error, info};

use crate::db::Result;

/// Number of memtable partitions (must match db.rs)
const NUM_PARTITIONS: usize = 16;

/// Messages sent to the background compaction worker thread
#[derive(Debug)]
pub(crate) enum CompactionTask {
    /// Compact a specific level
    CompactLevel(usize),
    /// Shutdown signal
    Shutdown,
}

/// Messages sent to the background flush worker thread
#[derive(Debug)]
pub(crate) enum FlushTask {
    /// Flush the memtable to SSTable
    Flush,
    /// Shutdown signal
    Shutdown,
}

/// Messages sent to the background WAL writer thread
#[derive(Debug)]
pub(crate) enum WALMessage {
    /// Write a record to the WAL
    Record(Record),
    /// Barrier: flush all pending records and send acknowledgement
    /// Used by flush() to ensure WAL is fully written before clearing
    Barrier(CrossbeamSender<()>),
}

/// Static compaction method for background worker thread
/// This is called from the worker thread without &self
///
/// NOTE: Background compaction does NOT upload to cloud storage yet.
/// This keeps the background worker simple and avoids passing storage_backend through threads.
/// Synchronous compaction and flush operations DO support cloud storage.
pub(crate) fn run_compaction(
    lsm: &Arc<ArcSwap<LSMTree>>,
    lsm_mutex: &Arc<Mutex<()>>,
    sstable_counter: &Arc<Mutex<u64>>,
    data_dir: &Path,
    level_num: usize,
    metrics: &Arc<MetricsCollector>,
    max_flushed_seq: &Arc<AtomicU64>,
    pending_deletions: &Arc<Mutex<Vec<(PathBuf, std::time::Instant)>>>,
) -> Result<()> {
    use crate::db::DB;

    // Background compaction uses local file-based approach (no cloud upload)
    // This simplifies the implementation - cloud upload is supported in:
    // - Synchronous compaction (compact_level)
    // - Flush operations (build_sstable_from_entries)
    #[cfg(feature = "object-store")]
    {
        DB::do_compact_level(
            lsm,
            lsm_mutex,
            sstable_counter,
            data_dir,
            level_num,
            metrics,
            max_flushed_seq,
            pending_deletions,
            &None, // No cloud storage for background worker
        )
    }

    #[cfg(not(feature = "object-store"))]
    {
        DB::do_compact_level(
            lsm,
            lsm_mutex,
            sstable_counter,
            data_dir,
            level_num,
            metrics,
            max_flushed_seq,
            pending_deletions,
        )
    }
}

/// Static flush method for background worker thread
/// This is called from the worker thread without &self
///
/// NOTE: Memtable swap already happened in try_swap_memtable() before signal was sent.
/// This method just builds the SSTable from immutable_memtable (slow part).
pub(crate) fn run_background_flush_partitioned(
    _memtables: &Arc<[ArcSwap<Memtable>; NUM_PARTITIONS]>,
    immutable_memtables: &Arc<ArcSwap<Option<Arc<Vec<Arc<Memtable>>>>>>,
    wal: &Arc<Mutex<WAL>>,
    lsm: &Arc<ArcSwap<LSMTree>>,
    lsm_mutex: &Arc<Mutex<()>>,
    vlog: &Arc<Mutex<Option<VLog>>>,
    sstable_counter: &Arc<Mutex<u64>>,
    data_dir: &Path,
    metrics: &Arc<MetricsCollector>,
    _memtable_capacity: usize,
    vlog_threshold: Option<usize>,
    flush_mutex: &Arc<Mutex<()>>,
    max_flushed_seq: &Arc<AtomicU64>,
) -> Result<()> {
    // Serialize all flushes to prevent concurrent SSTable builds
    let _flush_lock = flush_mutex.lock().expect("Flush mutex poisoned");

    let flush_start = Instant::now();

    // Check if there are immutable_memtables to flush (LOCK-FREE!)
    let immut_arc = immutable_memtables.load();
    let has_immutable = immut_arc.is_some();

    if !has_immutable {
        // No immutable memtables - another thread might have already flushed them
        return Ok(());
    }

    // Generate SSTable filename
    let mut counter = sstable_counter
        .lock()
        .expect("SSTable counter mutex poisoned");
    let flush_sequence = *counter; // Capture sequence for this background flush
    let sstable_path = data_dir.join(format!("L0_{:06}.sst", *counter));
    *counter += 1;
    drop(counter);

    // Build SSTable from immutable memtable partitions (slow part - this is why it's in background) (LOCK-FREE!)
    // Keep Arc alive and get reference to the Vec
    let immutable_partitions_arc = immut_arc
        .as_ref()
        .as_ref()
        .expect("Immutable partitions should be present");

    // Collect entries from ALL partitions and sort
    let mut all_entries: Vec<(Bytes, Entry)> = Vec::new();
    for partition_mt in immutable_partitions_arc.iter() {
        for (key, entry) in partition_mt.iter() {
            all_entries.push((key, entry));
        }
    }
    all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

    // Build SSTable with optional vLog support
    let mut vlog_guard = vlog.lock().expect("vLog mutex poisoned");

    if let (Some(threshold), Some(ref mut vlog_ref)) = (vlog_threshold, vlog_guard.as_mut()) {
        // KV separation enabled - use vLog for large values
        let mut builder = SSTableBuilder::create(&sstable_path)?
            .with_vlog_threshold(threshold)
            .with_max_sequence(flush_sequence);

        for (key, entry) in &all_entries {
            match entry {
                Entry::Value(value) => {
                    builder.add_with_vlog(key.clone(), value.clone(), vlog_ref)?;
                }
                Entry::Tombstone => {
                    builder.add_tombstone(key.clone())?;
                }
            }
        }

        builder.finish()?;

        // Sync vLog after flush
        vlog_ref.sync()?;
    } else {
        // No KV separation - traditional flush
        drop(vlog_guard);

        let mut builder = SSTableBuilder::create(&sstable_path)?.with_max_sequence(flush_sequence);
        for (key, entry) in &all_entries {
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
    // Arc automatically dropped (lock-free, no explicit drop needed!)

    let size = std::fs::metadata(&sstable_path)?.len();

    // Track physical bytes written
    metrics.record_physical_bytes(size);

    // CRITICAL FIX (Bug #7c): Serialize LSM tree updates to prevent ABA race
    // Hold mutex during read-modify-write to ensure atomicity
    {
        let _lsm_lock = lsm_mutex.lock().expect("LSM mutex poisoned");

        // Add to LSM tree L0 (serialized)
        let mut lsm_clone = (**lsm.load()).clone();
        lsm_clone.add_l0_sstable(sstable_path.clone(), size);
        lsm.store(Arc::new(lsm_clone));

        // Lock released here (automatic drop)
    }

    // Clear immutable memtables + WAL after successful flush (LOCK-FREE!)
    immutable_memtables.store(Arc::new(None));

    {
        let mut wal_guard = wal.lock().expect("WAL mutex poisoned");
        wal_guard.clear()?;
    }

    // CRITICAL FIX (Bug #7d): Update max_flushed_seq to allow compaction of this SSTable
    // This MUST happen after immutable_memtables is cleared to prevent data loss
    // Without this, compaction will skip all background-flushed SSTables forever!
    // Use fetch_max to handle out-of-order flush completions (only update if new value is greater)
    max_flushed_seq.fetch_max(flush_sequence, Ordering::SeqCst);

    let flush_duration_ms = flush_start.elapsed().as_millis();
    info!(
        duration_ms = flush_duration_ms,
        sstable_path = ?sstable_path,
        sstable_size_bytes = size,
        partitions_merged = NUM_PARTITIONS,
        "Background partitioned memtable flush complete"
    );

    // Record flush metric
    metrics.record_flush();

    Ok(())
}

/// Spawn background compaction worker thread if enabled
///
/// Returns (Option<Sender>, Option<JoinHandle>) for sending tasks and joining the thread
pub(crate) fn spawn_compaction_worker(
    enabled: bool,
    lsm: Arc<ArcSwap<LSMTree>>,
    lsm_mutex: Arc<Mutex<()>>,
    sstable_counter: Arc<Mutex<u64>>,
    data_dir: PathBuf,
    metrics: Arc<MetricsCollector>,
    max_flushed_seq: Arc<AtomicU64>,
    compaction_healthy: Arc<AtomicBool>,
    pending_deletions: Arc<Mutex<Vec<(PathBuf, Instant)>>>,
) -> (Option<Sender<CompactionTask>>, Option<JoinHandle<()>>) {
    if !enabled {
        return (None, None);
    }

    let (tx, rx) = channel::<CompactionTask>();

    // Spawn compaction worker thread with panic detection
    let worker = thread::Builder::new()
        .name("compaction-worker".to_string())
        .spawn(move || {
            // Wrap in catch_unwind to detect panics and mark health status
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while let Ok(task) = rx.recv() {
                    match task {
                        CompactionTask::CompactLevel(level_num) => {
                            // Perform compaction
                            if let Err(e) = run_compaction(
                                &lsm,
                                &lsm_mutex,
                                &sstable_counter,
                                &data_dir,
                                level_num,
                                &metrics,
                                &max_flushed_seq,
                                &pending_deletions,
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
            }));

            // If panicked, mark as unhealthy
            if result.is_err() {
                error!("Compaction worker thread panicked");
                compaction_healthy.store(false, Ordering::SeqCst);
            }
        })
        .expect("Failed to spawn compaction worker thread");

    (Some(tx), Some(worker))
}

/// Spawn background flush worker thread if enabled
///
/// Returns (Option<Sender>, Option<JoinHandle>) for sending tasks and joining the thread
pub(crate) fn spawn_flush_worker(
    enabled: bool,
    memtables: Arc<[ArcSwap<Memtable>; NUM_PARTITIONS]>,
    immutable_memtables: Arc<ArcSwap<Option<Arc<Vec<Arc<Memtable>>>>>>,
    wal: Arc<Mutex<WAL>>,
    lsm: Arc<ArcSwap<LSMTree>>,
    lsm_mutex: Arc<Mutex<()>>,
    vlog: Arc<Mutex<Option<VLog>>>,
    sstable_counter: Arc<Mutex<u64>>,
    data_dir: PathBuf,
    metrics: Arc<MetricsCollector>,
    memtable_capacity: usize,
    vlog_threshold: Option<usize>,
    flush_mutex: Arc<Mutex<()>>,
    max_flushed_seq: Arc<AtomicU64>,
    flush_healthy: Arc<AtomicBool>,
) -> (Option<Sender<FlushTask>>, Option<JoinHandle<()>>) {
    if !enabled {
        return (None, None);
    }

    let (tx, rx) = channel::<FlushTask>();

    // Spawn flush worker thread with panic detection
    let worker = thread::Builder::new()
        .name("flush-worker".to_string())
        .spawn(move || {
            // Wrap in catch_unwind to detect panics and mark health status
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while let Ok(task) = rx.recv() {
                    match task {
                        FlushTask::Flush => {
                            // Perform background flush (now with partitioned memtables)
                            if let Err(e) = run_background_flush_partitioned(
                                &memtables,
                                &immutable_memtables,
                                &wal,
                                &lsm,
                                &lsm_mutex,
                                &vlog,
                                &sstable_counter,
                                &data_dir,
                                &metrics,
                                memtable_capacity,
                                vlog_threshold,
                                &flush_mutex,
                                &max_flushed_seq,
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
            }));

            // If panicked, mark as unhealthy
            if result.is_err() {
                error!("Flush worker thread panicked");
                flush_healthy.store(false, Ordering::SeqCst);
            }
        })
        .expect("Failed to spawn flush worker thread");

    (Some(tx), Some(worker))
}

/// Spawn background WAL writer thread (always enabled for lock-free writes)
///
/// Returns (Sender, JoinHandle) for sending messages and joining the thread
pub(crate) fn spawn_wal_writer(
    wal: Arc<Mutex<WAL>>,
    wal_healthy: Arc<AtomicBool>,
) -> (CrossbeamSender<WALMessage>, JoinHandle<()>) {
    let (wal_tx, wal_rx) = unbounded::<WALMessage>();

    let wal_worker = thread::Builder::new()
        .name("wal-writer".to_string())
        .spawn(move || {
            // Wrap in catch_unwind to detect panics and mark health status
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut batch = Vec::with_capacity(1000);

                loop {
                    // Block on first message
                    let channel_closed = match wal_rx.recv() {
                        Ok(WALMessage::Record(record)) => {
                            batch.push(record);
                            false
                        }
                        Ok(WALMessage::Barrier(ack_tx)) => {
                            // Flush all pending records before acknowledging
                            if !batch.is_empty() {
                                if let Err(e) =
                                    wal.lock().expect("WAL mutex poisoned").write_batch(&batch)
                                {
                                    error!(error = %e, "WAL batch write failed during barrier");
                                }
                                batch.clear();
                            }
                            // Send acknowledgement (flush() is waiting for this)
                            let _ = ack_tx.send(());
                            false // Continue processing
                        }
                        Err(_) => {
                            // Channel closed - need to drain remaining messages
                            true
                        }
                    };

                    // Drain channel (collect all pending messages)
                    // If channel closed, drain until truly empty
                    // If channel open, drain up to batch limit
                    loop {
                        match wal_rx.try_recv() {
                            Ok(WALMessage::Record(record)) => {
                                batch.push(record);
                                // Keep draining if channel closed, otherwise respect batch limit
                                if !channel_closed && batch.len() >= 1000 {
                                    break;
                                }
                            }
                            Ok(WALMessage::Barrier(ack_tx)) => {
                                // Flush current batch before acknowledging
                                if !batch.is_empty() {
                                    if let Err(e) =
                                        wal.lock().expect("WAL mutex poisoned").write_batch(&batch)
                                    {
                                        error!(error = %e, "WAL batch write failed during barrier");
                                    }
                                    batch.clear();
                                }
                                // Send acknowledgement
                                let _ = ack_tx.send(());
                                // Continue draining if more messages
                            }
                            Err(_) => break, // Channel empty
                        }
                    }

                    // Write batch if not empty
                    if !batch.is_empty() {
                        if let Err(e) = wal.lock().expect("WAL mutex poisoned").write_batch(&batch)
                        {
                            error!(error = %e, "WAL batch write failed");
                        }
                        batch.clear();
                    }

                    // Exit after writing final batch
                    if channel_closed {
                        // Final fsync to ensure all data is on disk before thread exits
                        if let Err(e) = wal.lock().expect("WAL mutex poisoned").sync() {
                            error!(error = %e, "Final WAL sync failed - DATA MAY BE LOST");
                        }
                        debug!("WAL writer thread: channel closed, all records flushed and synced");
                        break;
                    }
                }

                info!("WAL writer thread shutting down");
            }));

            // If panicked, mark as unhealthy - CRITICAL for data safety
            if result.is_err() {
                error!("WAL writer thread panicked - DATA LOSS MAY OCCUR");
                wal_healthy.store(false, Ordering::SeqCst);
            }
        })
        .expect("Failed to spawn WAL writer thread");

    (wal_tx, wal_worker)
}
