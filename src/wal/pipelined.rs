use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};
use crate::wal::{WAL, Record, Result};

struct Writer {
    record: Record,
    thread: Thread,
    // Result of the WAL write operation
    result: Mutex<Option<Result<u64>>>,
    // Flag to indicate completion (to handle spurious wakeups)
    done: Mutex<bool>,
}

impl Writer {
    fn new(record: Record) -> Self {
        Self {
            record,
            thread: thread::current(),
            result: Mutex::new(None),
            done: Mutex::new(false),
        }
    }

    fn signal_done(&self, res: Result<u64>) {
        {
            let mut result = self.result.lock().expect("mutex poisoned");
            *result = Some(res);
        }
        {
            let mut done = self.done.lock().expect("mutex poisoned");
            *done = true;
        }
        self.thread.unpark();
    }
    
    fn is_done(&self) -> bool {
        *self.done.lock().expect("mutex poisoned")
    }
    
    fn take_result(&self) -> Result<u64> {
        self.result.lock().expect("mutex poisoned").take().expect("result must be set before waking writer")
    }
}

struct State {
    queue: VecDeque<Arc<Writer>>,
    writer_active: bool,
}

pub struct PipelinedWAL {
    wal: Arc<Mutex<WAL>>,
    state: Mutex<State>,
    delay: Duration,
    max_batch_size: usize,
}

impl PipelinedWAL {
    pub fn new(wal: Arc<Mutex<WAL>>, delay: Duration, max_batch_size: usize) -> Self {
        Self {
            wal,
            state: Mutex::new(State {
                queue: VecDeque::new(),
                writer_active: false,
            }),
            delay,
            max_batch_size,
        }
    }

    pub fn put<F>(&self, record: Record, on_memtable: F) -> Result<u64>
    where
        F: Fn(&[Record]),
    {
        let writer = Arc::new(Writer::new(record));

        // 1. Enqueue
        let is_leader = {
            let mut state = self.state.lock().expect("mutex poisoned");
            state.queue.push_back(writer.clone());
            if !state.writer_active {
                state.writer_active = true;
                true
            } else {
                false
            }
        };

        // 2. If Leader, process batches until done
        if is_leader {
            self.process_batches(on_memtable);
        } else {
            // 3. If Follower, wait
            loop {
                thread::park();
                if writer.is_done() {
                    break;
                }
            }
        }

        // 4. Return result
        writer.take_result()
    }

    /// Sync the WAL to ensure all data is written to disk
    ///
    /// This should be called before shutdown to prevent data loss.
    pub fn sync(&self) -> Result<()> {
        let wal = self.wal.lock().expect("WAL mutex poisoned");
        wal.sync()
    }

    fn process_batches<F>(&self, on_memtable: F)
    where
        F: Fn(&[Record]),
    {
        loop {
            let mut batch_writers = Vec::new();
            
            // 1. Wait strategy (Group Commit Delay)
            if self.delay > Duration::ZERO {
                let deadline = Instant::now() + self.delay;
                
                loop {
                    let mut state = self.state.lock().expect("mutex poisoned");
                    if state.queue.is_empty() {
                        state.writer_active = false;
                        return;
                    }
                    
                    // If batch is full or deadline reached, proceed
                    if state.queue.len() >= self.max_batch_size || Instant::now() >= deadline {
                        batch_writers = state.queue.drain(..).collect();
                        break;
                    }
                    
                    // Release lock and spin/yield
                    drop(state);
                    thread::yield_now(); 
                    // spin loop is better than sleep for short delays (<1ms)
                }
            } else {
                // No delay - greedy consumption
                let mut state = self.state.lock().expect("mutex poisoned");
                if state.queue.is_empty() {
                    state.writer_active = false;
                    return;
                }
                batch_writers = state.queue.drain(..).collect();
            }
            
            if batch_writers.is_empty() {
                continue;
            }

            // 2. Extract records
            let records: Vec<Record> = batch_writers.iter().map(|w| w.record.clone()).collect();

            // 3. Write to WAL
            let wal_result = {
                let mut wal = self.wal.lock().expect("WAL mutex poisoned");
                wal.write_batch(&records)
            };

            // 4. Write to Memtable (Callback)
            if wal_result.is_ok() {
                on_memtable(&records);
            }

            // 5. Wake up writers with result
            match wal_result {
                Ok(offsets) => {
                    for (writer, offset) in batch_writers.iter().zip(offsets.into_iter()) {
                        writer.signal_done(Ok(offset));
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    for writer in batch_writers.iter() {
                        let err = crate::wal::WALError::Io(std::io::Error::other(
                            err_str.clone(),
                        ));
                        writer.signal_done(Err(err));
                    }
                }
            }
        }
    }
}
