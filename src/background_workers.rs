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
use crossbeam_channel::Sender as CrossbeamSender;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;

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

        let mut builder =
            SSTableBuilder::create(&sstable_path)?.with_max_sequence(flush_sequence);
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
