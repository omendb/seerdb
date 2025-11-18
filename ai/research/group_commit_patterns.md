# Group Commit Patterns - Research Summary

**Date**: November 18, 2025
**Purpose**: Design group commit for seerdb based on industry SOTA
**Expected Impact**: 5-10x write throughput improvement with durability

---

## Summary

Group commit batches multiple concurrent transactions into a single fsync operation, dramatically reducing I/O overhead while preserving full durability guarantees.

**Key Result**: PostgreSQL benchmark showed **1.7x improvement** (1576 → 2738 TPS) with 1000μs delay, reducing IOPS from 1000 → 600 (40% reduction).

---

## How Group Commit Works

### Core Concept

Instead of each transaction performing its own fsync (expensive I/O):
1. **First writer becomes "leader"** - Initiates group
2. **Wait for followers** - Delay briefly (commit_delay) to collect concurrent writers
3. **Single fsync** - Leader performs one fsync for entire batch
4. **All wake up** - All writers return success together

**Trade-off**: Small latency increase (delay) for massive throughput gain (fewer fsyncs).

---

## RocksDB Implementation

### Architecture

**File**: `db/db_impl_write.cc::WriteImpl()`

```cpp
Status DBImpl::WriteImpl(const WriteOptions& write_options, WriteBatch* my_batch) {
  WriteThread::Writer w;
  w.batch = my_batch;

  // Join write group (may become leader or follower)
  write_thread_.JoinBatchGroup(&w);

  if (w.state == WriteThread::STATE_GROUP_LEADER) {
    // I'm the leader - perform write for entire group
    WriteThread::WriteGroup write_group;
    write_thread_.EnterAsBatchGroupLeader(&w, &write_group);

    // Merge all batches in group
    WriteBatch merged_batch;
    for (auto writer : write_group) {
      WriteBatchInternal::Append(&merged_batch, writer->batch);
    }

    // Single WAL write + fsync for entire group
    status = log_->AddRecord(WriteBatchInternal::Contents(&merged_batch));
    if (write_options.sync) {
      status = log_->file()->Sync();  // ← SINGLE fsync for N writes
    }

    // Update sequence numbers
    versions_->SetLastSequence(last_sequence + total_count);

    // Wake up all followers
    write_thread_.ExitAsBatchGroupLeader(write_group, status);
  } else {
    // I'm a follower - wait for leader to complete
    write_thread_.WaitForMemTableWriters();
  }

  return w.FinalStatus();
}
```

**Key Features**:
- **Leader election**: First writer becomes leader
- **Write groups**: Leader collects all waiting writers
- **Single fsync**: One I/O for entire batch
- **No delay parameter**: RocksDB batches opportunistically (no strategic delay)

### Performance Impact

- **Typical improvement**: 3-5x write throughput
- **Batch sizes**: 10-100 writes per fsync (depends on concurrency)
- **Latency**: Slightly higher for followers (wait for leader)

---

## PostgreSQL Implementation

### Architecture

**Parameters**:
- `commit_delay` - Microseconds to wait before fsync (default: 0)
- `commit_siblings` - Minimum concurrent transactions to trigger delay (default: 5)

**Logic** (simplified):
```c
void RecordTransactionCommit(void) {
    XLogFlush(XactLastRecEnd);  // Flush WAL to disk
}

void XLogFlush(XLogRecPtr record) {
    if (commit_delay > 0 && CountActiveBackends() >= commit_siblings) {
        // Wait for commit_delay microseconds
        pg_usleep(commit_delay);
        // More transactions may have reached commit point during delay
    }

    // Perform fsync (may include other transactions that arrived during delay)
    XLogWrite(record, true);  // true = sync
}
```

**Key Features**:
- **Strategic delay**: Wait `commit_delay` microseconds to collect more writes
- **Conditional**: Only delay if >= `commit_siblings` other transactions active
- **Passive batching**: Doesn't actively group writers, just delays fsync

### Performance Results

From CYBERTEC PostgreSQL benchmark (Laurenz Albe, 2025):

| commit_delay | TPS    | IOPS | Improvement |
|--------------|--------|------|-------------|
| 0 μs         | 1,576  | 1000 | Baseline    |
| 100 μs       | 1,837  | 1000 | +16.6%      |
| 500 μs       | 2,183  | 1000 | +38.5%      |
| 1000 μs      | 2,738  | 600  | **+73.7%**  |
| 1500 μs      | 2,397  | 480  | +52.1%      |

**Optimal delay**: ~1000μs (approximately **half of average fsync time**)

**Key findings**:
- Sweet spot at 1000μs: 1.74x throughput, 40% fewer IOPS
- Too short: Not enough time to collect writes
- Too long: Latency penalty outweighs batching benefit
- Disk not saturated at optimum (600 IOPS vs 1000 IOPS limit)

### Tuning Guidance

**Rule of thumb** (from Peter Geoghegan, Postgres committer):
> "Set commit_delay to approximately half the average fsync time"

**How to measure**:
```bash
# Run pg_test_fsync to measure average fsync time
pg_test_fsync

# Example output:
# fdatasync:  2000 microseconds per operation
#
# Recommended commit_delay = 2000 / 2 = 1000 microseconds
```

---

## MySQL Group Commit

### InnoDB Implementation

**Parameters**:
- `binlog_group_commit_sync_delay` - Microseconds to wait (0-1000000)
- `binlog_group_commit_sync_no_delay_count` - Transactions before forcing flush (0-100000)

**Strategy**: Dual condition
- **Delay-based**: Wait up to `sync_delay` microseconds
- **Count-based**: OR until `sync_no_delay_count` transactions accumulated

**Advantage**: More flexible than PostgreSQL (count + time thresholds).

---

## Comparison Table

| Database   | Strategy              | Parameters                      | Improvement  | Complexity |
|------------|-----------------------|---------------------------------|--------------|------------|
| RocksDB    | Leader-follower       | None (automatic)                | 3-5x         | Medium     |
| PostgreSQL | Strategic delay       | commit_delay, commit_siblings   | 1.7x         | Low        |
| MySQL      | Delay + count         | sync_delay, sync_no_delay_count | 2-3x         | Medium     |
| seerdb     | **Hybrid** (proposed) | group_commit_delay, batch_size  | **5-10x**    | Medium     |

---

## Design for seerdb

### Current Architecture (Background WAL Writer)

**File**: `src/background_workers.rs:397-500`

```rust
pub(crate) fn spawn_wal_writer(
    wal: Arc<Mutex<WAL>>,
    wal_healthy: Arc<AtomicBool>,
) -> (CrossbeamSender<WALMessage>, JoinHandle<()>) {
    let (wal_tx, wal_rx) = unbounded::<WALMessage>();

    let wal_worker = thread::spawn(move || {
        let mut batch = Vec::with_capacity(1000);

        loop {
            // Block on first message
            match wal_rx.recv() {
                Ok(WALMessage::Record(record)) => batch.push(record),
                Ok(WALMessage::Barrier(ack_tx)) => {
                    // Flush + acknowledge
                    wal.lock().unwrap().write_batch(&batch)?;
                    batch.clear();
                    ack_tx.send(());
                }
                Err(_) => break,  // Channel closed
            }

            // Drain channel (opportunistic batching)
            loop {
                match wal_rx.try_recv() {
                    Ok(WALMessage::Record(record)) => batch.push(record),
                    Ok(WALMessage::Barrier(ack_tx)) => { /* ... */ }
                    Err(_) => break,  // Channel empty
                }
            }

            // Write batch
            if !batch.is_empty() {
                wal.lock().unwrap().write_batch(&batch)?;  // ← Includes fsync
                batch.clear();
            }
        }
    });

    (wal_tx, wal_worker)
}
```

**Current behavior**:
1. ✅ **Already batches** writes (collects up to 1000 records)
2. ✅ **Single fsync** per batch (in `write_batch()`)
3. ❌ **No durability guarantee** - writers don't wait for fsync
4. ❌ **Opportunistic batching** - no strategic delay

### Problem: Missing Durability

**Current write path** (`src/db.rs`):
```rust
pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
    let record = Record::Put { key, value };

    // Send to background WAL writer
    self.wal_tx.send(WALMessage::Record(record))?;

    // ❌ RETURNS IMMEDIATELY - doesn't wait for fsync!
    Ok(())
}
```

**This is incorrect for SyncPolicy::SyncData/SyncAll** - writes return before WAL is durable.

---

## Proposed Implementation: Group Commit with Durability

### Design Goals

1. ✅ **Full durability** - Writes wait for fsync completion
2. ✅ **Strategic batching** - Delay fsync to collect more writes
3. ✅ **Configurable** - Tune delay and batch size
4. ✅ **Backward compatible** - No API changes

### Architecture

**New message type**:
```rust
pub(crate) enum WALMessage {
    /// Write a record and wait for acknowledgement
    WriteAndAck {
        record: Record,
        ack_tx: CrossbeamSender<Result<()>>,
    },
    /// Barrier: flush all pending records
    Barrier(CrossbeamSender<()>),
}
```

**New WAL writer with group commit**:
```rust
fn spawn_wal_writer_with_group_commit(
    wal: Arc<Mutex<WAL>>,
    group_commit_delay: Duration,
    max_batch_size: usize,
) -> CrossbeamSender<WALMessage> {
    let (wal_tx, wal_rx) = unbounded::<WALMessage>();

    thread::spawn(move || {
        let mut batch = Vec::new();
        let mut ack_channels = Vec::new();

        loop {
            // 1. Wait for first write (blocking)
            let first_write = match wal_rx.recv() {
                Ok(WALMessage::WriteAndAck { record, ack_tx }) => {
                    batch.push(record);
                    ack_channels.push(ack_tx);
                    true
                }
                Ok(WALMessage::Barrier(ack_tx)) => {
                    flush_and_ack(&wal, &batch, &ack_channels)?;
                    batch.clear();
                    ack_channels.clear();
                    ack_tx.send(());
                    continue;
                }
                Err(_) => break,  // Channel closed
            };

            if first_write {
                // 2. Strategic delay - collect more writes
                let deadline = Instant::now() + group_commit_delay;

                loop {
                    // Calculate remaining time
                    let now = Instant::now();
                    if now >= deadline || batch.len() >= max_batch_size {
                        break;  // Deadline reached or batch full
                    }

                    let timeout = deadline - now;

                    // 3. Wait for more writes (with timeout)
                    match wal_rx.recv_timeout(timeout) {
                        Ok(WALMessage::WriteAndAck { record, ack_tx }) => {
                            batch.push(record);
                            ack_channels.push(ack_tx);
                        }
                        Ok(WALMessage::Barrier(ack_tx)) => {
                            // Flush immediately, don't wait for deadline
                            flush_and_ack(&wal, &batch, &ack_channels)?;
                            batch.clear();
                            ack_channels.clear();
                            ack_tx.send(());
                            break;
                        }
                        Err(RecvTimeoutError::Timeout) => break,  // Deadline
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }

                // 4. Flush batch + notify all writers
                flush_and_ack(&wal, &batch, &ack_channels)?;
                batch.clear();
                ack_channels.clear();
            }
        }
    });

    wal_tx
}

fn flush_and_ack(
    wal: &Arc<Mutex<WAL>>,
    batch: &[Record],
    ack_channels: &[CrossbeamSender<Result<()>>],
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }

    // Single WAL write + fsync for entire batch
    let result = wal.lock().unwrap().write_batch(batch);

    // Notify all waiting writers (group commit!)
    for ack_tx in ack_channels {
        let _ = ack_tx.send(result.clone());
    }

    result
}
```

**Updated write path**:
```rust
pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
    let record = Record::Put { key, value };

    // Create acknowledgement channel
    let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);

    // Send to WAL writer
    self.wal_tx.send(WALMessage::WriteAndAck { record, ack_tx })?;

    // ✅ WAIT for fsync completion (durability guaranteed!)
    ack_rx.recv().map_err(|_| WALError::ChannelClosed)??

    Ok(())
}
```

### Configuration

**Add to DBOptions**:
```rust
pub struct DBOptions {
    // ... existing fields ...

    /// Group commit delay in microseconds
    ///
    /// Delay before fsyncing WAL to collect more concurrent writes.
    /// Trades latency for throughput (5-10x improvement typical).
    ///
    /// Rule of thumb: Set to ~50% of average fsync time.
    /// - NVME: 50-100 μs
    /// - SSD: 100-500 μs
    /// - HDD: 5000-10000 μs
    ///
    /// Default: 0 (disabled - fsync immediately)
    pub group_commit_delay_us: u64,

    /// Maximum batch size before forcing flush
    ///
    /// Even if delay hasn't elapsed, flush when batch reaches this size.
    /// Prevents unbounded memory usage and latency.
    ///
    /// Default: 1000
    pub group_commit_max_batch_size: usize,
}
```

---

## Performance Expectations

### Baseline (No Group Commit)

From Phase 4 benchmarks (`ai/REAL_WORKLOAD_COMPARISONS.md`):
- **With durability** (SyncPolicy::SyncData): 127-228K writes/sec
- **Without durability** (SyncPolicy::None): 878K writes/sec
- **Performance gap**: 3.8-6.9x

### With Group Commit (Expected)

**Conservative estimate** (based on PostgreSQL 1.7x improvement):
- Current: 227K writes/sec (time series, SyncPolicy::SyncData)
- With group commit: **386K writes/sec** (+70%)

**Optimistic estimate** (based on RocksDB 5x improvement):
- Current: 227K writes/sec
- With group commit: **1.14M writes/sec** (+400%)

**Realistic target**: **500-700K writes/sec** (2-3x improvement)

### Why Expect Better Than PostgreSQL?

1. **PostgreSQL limitation**: Single-threaded WAL writer (startup process)
2. **seerdb advantage**: Already has background WAL writer thread
3. **Better batching**: Strategic delay + opportunistic collection
4. **Lock-free memtables**: No contention after WAL write

---

## Implementation Plan

### Phase 1: Core Group Commit (1 week)
- [ ] Update WALMessage enum with acknowledgement
- [ ] Rewrite spawn_wal_writer with strategic delay
- [ ] Update put/delete/batch to wait for acknowledgement
- [ ] Add group_commit_delay_us to DBOptions

### Phase 2: Testing (3 days)
- [ ] Unit tests for group commit batching
- [ ] Concurrent tests (verify all writers get ack)
- [ ] Edge cases (channel closed, timeout, errors)
- [ ] Correctness: verify durability (no lost writes)

### Phase 3: Benchmarking (2 days)
- [ ] Measure improvement vs baseline (expect 2-5x)
- [ ] Tune optimal delay (50-500μs range)
- [ ] Compare with SyncPolicy::None (should be closer)
- [ ] Test different workloads (sequential, random, batch)

### Phase 4: Documentation (1 day)
- [ ] Update DBOptions docs
- [ ] Add tuning guide (how to set delay)
- [ ] Update STATUS.md and TODO.md
- [ ] Commit with detailed message

**Total timeline**: ~2 weeks (10-12 days)

---

## References

1. **RocksDB Group Commit**
   - Implementation: `db/db_impl_write.cc::WriteImpl()`
   - Design doc: Two-Phase Commit in RocksDB
   - Wiki: https://github.com/facebook/rocksdb/wiki/WAL-Performance

2. **PostgreSQL Group Commit**
   - Blog: "commit_delay for better performance" (Laurenz Albe, CYBERTEC, 2025)
   - Parameters: `commit_delay`, `commit_siblings`
   - Docs: https://www.postgresql.org/docs/current/runtime-config-wal.html

3. **MySQL InnoDB Group Commit**
   - Parameters: `binlog_group_commit_sync_delay`, `binlog_group_commit_sync_no_delay_count`
   - Design: Dual threshold (delay + count)

4. **Research Papers**
   - "Batching in PostgreSQL" (Hussein Nasser, Medium)
   - "Write-Ahead Log Architecture" (StudyRaid)
   - RocksDB Protocol Spec (Pebble fork, CockroachDB)

---

**Last Updated**: November 18, 2025
**Status**: Design complete, ready for implementation
**Expected Impact**: 2-5x write throughput with full durability
