// Database helper functions
//
// Utility functions used by the main DB implementation that are relatively
// self-contained and don't require the full DB struct.

use crate::db::{DBError, DBOptions, Result, partition_for_key};
use crate::memtable::Memtable;
use crate::wal::{Record, WALReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Recover partitioned memtables from WAL
///
/// Reads records one by one and distributes them across partitions using hash function.
/// Stops gracefully if corruption or truncation is encountered.
/// This ensures we recover all valid records before the corruption point.
pub(crate) fn recover_partitioned(wal_path: &Path, memtables: &[Memtable]) -> Result<()> {
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
                Record::Batch { operations } => {
                    // Apply all operations in the batch atomically
                    // All operations were written as a single WAL record, so they're atomic
                    for op in operations {
                        match op {
                            crate::wal::BatchOp::Put { key, value } => {
                                let partition = partition_for_key(&key);
                                memtables[partition].put(key, value);
                            }
                            crate::wal::BatchOp::Delete { key } => {
                                let partition = partition_for_key(&key);
                                memtables[partition].delete(key);
                            }
                        }
                    }
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

/// Clean up old SSTable deletion queue
///
/// Files are queued for deletion after compaction but kept for 5 seconds
/// to allow concurrent readers to finish. This function periodically removes
/// files that are old enough.
///
/// This is called after each compaction to safely delete old SSTable files
/// without interfering with concurrent readers.
pub(crate) fn cleanup_old_deletions(
    pending_deletions: &Arc<Mutex<Vec<(PathBuf, std::time::Instant)>>>,
) {
    const DELETION_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

    let mut pending = pending_deletions.lock().unwrap();
    let now = std::time::Instant::now();

    // Separate files into (ready_to_delete, still_pending)
    let mut still_pending = Vec::new();
    let mut ready_to_delete = Vec::new();

    for (path, queued_at) in pending.drain(..) {
        if now.duration_since(queued_at) >= DELETION_DELAY {
            ready_to_delete.push(path);
        } else {
            still_pending.push((path, queued_at));
        }
    }

    // Keep files that aren't old enough yet
    *pending = still_pending;
    drop(pending); // Release lock before doing I/O

    // Delete old files (outside the lock to avoid blocking)
    for path in ready_to_delete {
        if let Err(e) = std::fs::remove_file(&path) {
            // Log but don't fail - file might already be deleted
            warn!(path = ?path, error = %e, "Failed to delete old SSTable");
        } else {
            debug!(path = ?path, "Deleted old SSTable after safe delay");
        }
    }
}

/// Check if there is sufficient disk space
///
/// If min_disk_space_bytes is configured in options, verifies that the disk
/// containing the data directory has at least that much space available.
///
/// Returns an error if disk space is insufficient.
pub(crate) fn check_disk_space(options: &DBOptions) -> Result<()> {
    if let Some(min_space) = options.min_disk_space_bytes {
        use sysinfo::{DiskExt, System, SystemExt};

        // Get disk information (only refresh disks, not all system info)
        let mut sys = System::new();
        sys.refresh_disks_list();

        // Find the disk containing our data directory
        let data_dir = &options.data_dir;
        if let Some(disk) = sys
            .disks()
            .iter()
            .find(|d| data_dir.starts_with(d.mount_point()))
        {
            let available = disk.available_space();
            if available < min_space {
                return Err(DBError::DiskSpaceFull {
                    available,
                    required: min_space,
                });
            }
        }
    }
    Ok(())
}
