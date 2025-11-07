    pub fn flush(&self) -> Result<()> {
        use crate::memtable::Entry;
        use crate::sstable::SSTableBuilder;

        // **CRITICAL FIX**: Serialize all flushes to prevent concurrent flush races
        let _flush_lock = self.flush_mutex.lock().expect("Flush mutex poisoned");

        let flush_start = Instant::now();

        // Check total size across all partitions
        let total_size: usize = self.memtables.iter()
            .map(|mt| mt.lock().expect("Memtable lock poisoned").size())
            .sum();

        // Early return if all partitions are empty
        if total_size == 0 {
            return Ok(());
        }

        info!(
            total_memtable_size_bytes = total_size,
            partitions = NUM_PARTITIONS,
            "Starting partitioned memtable flush"
        );

        // **CRITICAL**: Check if there's a previous failed flush
        // If immutable_memtables is occupied, flush it first to avoid data loss
        let pending_immutable = {
            let mut immut_guard = self.immutable_memtables.lock().expect("Immutable memtables lock poisoned");
            immut_guard.take()
        };

        if let Some(pending_partitions) = pending_immutable {
            // Previous flush failed - retry flushing the existing immutable partitions
            warn!(partitions = pending_partitions.len(), "Retrying flush of previously failed immutable partitions");

            // Generate filename for pending flush
            let mut counter = self.sstable_counter.lock().expect("SSTable counter mutex poisoned");
            let pending_sstable_path = self.options.data_dir.join(format!("L0_{:06}.sst", *counter));
            *counter += 1;
            drop(counter);

            // Collect and sort entries from all pending partitions
            let mut all_entries = Vec::new();
            for partition_mt in &pending_partitions {
                for (key, entry) in partition_mt.iter() {
                    all_entries.push((key, entry));
                }
            }

            // Sort by key (deduplication handled by taking last value for each key)
            all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

            // Build SSTable from sorted entries
            self.build_sstable_from_entries(&pending_sstable_path, all_entries.iter())?;
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

            info!("Successfully flushed previously failed immutable partitions");
        }

        // Now check if active partitions need flushing
        let total_size: usize = self.memtables.iter()
            .map(|mt| mt.lock().expect("Memtable lock poisoned").size())
            .sum();

        if total_size == 0 {
            return Ok(()); // Nothing to flush
        }

        // Generate SSTable filename for main flush
        let mut counter = self.sstable_counter.lock().expect("SSTable counter mutex poisoned");
        let sstable_path = self.options.data_dir.join(format!("L0_{:06}.sst", *counter));
        *counter += 1;
        drop(counter);

        // Swap ALL 16 partitions atomically (RocksDB-style immutable memtables)
        // 1. Lock each partition
        // 2. Swap with new empty partition
        // 3. Collect old partitions
        // 4. Store as immutable partitions
        // 5. Release locks
        let capacity_per_partition = self.options.memtable_capacity / NUM_PARTITIONS;
        let mut flushing_partitions = Vec::with_capacity(NUM_PARTITIONS);

        for partition_mt in &self.memtables {
            let mut mt_guard = partition_mt.lock().expect("Memtable lock poisoned");
            let old_partition = std::mem::replace(&mut *mt_guard, Memtable::new(capacity_per_partition));
            flushing_partitions.push(old_partition);
            drop(mt_guard); // Release lock immediately
        }

        // Store in immutable_memtables so readers can access during flush
        {
            let mut immut_guard = self.immutable_memtables.lock().expect("Immutable memtables lock poisoned");
            *immut_guard = Some(flushing_partitions.clone());
        }

        // Collect entries from ALL partitions
        let mut all_entries: Vec<(Bytes, Entry)> = Vec::new();
        for partition_mt in &flushing_partitions {
            for (key, entry) in partition_mt.iter() {
                all_entries.push((key, entry));
            }
        }

        // Sort by key to build sorted SSTable
        // If there are duplicates (same key in multiple partitions due to race), keep last one
        all_entries.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));

        // Build SSTable from sorted entries
        self.build_sstable_from_entries(&sstable_path, all_entries.iter())?;

        let size = std::fs::metadata(&sstable_path)?.len();

        // Track physical bytes written to SSTable
        self.metrics.record_physical_bytes(size);

        // Add to LSM tree L0
        let mut lsm = self.lsm.lock().expect("LSM mutex poisoned");
        let sstable_path_for_log = sstable_path.clone();
        lsm.add_l0_sstable(sstable_path, size);

        // Clear immutable partitions + WAL after successful flush
        let mut immut_guard = self.immutable_memtables.lock().expect("Immutable memtables lock poisoned");
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
            partitions_merged = NUM_PARTITIONS,
            "Partitioned memtable flush complete"
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

    /// Helper: Build SSTable from iterator of (key, entry) pairs
    /// Handles both normal values and vLog separation
    fn build_sstable_from_entries<'a, I>(&self, sstable_path: &Path, entries: I) -> Result<()>
    where
        I: Iterator<Item = &'a (Bytes, Entry)>,
    {
        use crate::sstable::SSTableBuilder;

        let mut vlog_guard = self.vlog.lock().expect("vLog mutex poisoned");

        if let (Some(threshold), Some(ref mut vlog)) = (self.options.vlog_threshold, vlog_guard.as_mut()) {
            // KV separation enabled - use vLog for large values
            let mut builder = SSTableBuilder::create(sstable_path)?.with_vlog_threshold(threshold);

            for (key, entry) in entries {
                match entry {
                    Entry::Value(value) => {
                        builder.add_with_vlog(key, value, vlog)?;
                    }
                    Entry::Tombstone => {
                        builder.add_tombstone(key)?;
                    }
                }
            }

            builder.finish()?;

            // ALWAYS sync vLog after flush
            vlog.sync()?;
        } else {
            // No KV separation - traditional flush
            drop(vlog_guard);

            let mut builder = SSTableBuilder::create(sstable_path)?;

            for (key, entry) in entries {
                match entry {
                    Entry::Value(value) => {
                        builder.add(key, value)?;
                    }
                    Entry::Tombstone => {
                        builder.add_tombstone(key)?;
                    }
                }
            }

            builder.finish()?;
        }

        Ok(())
    }
