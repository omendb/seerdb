#![no_main]

use libfuzzer_sys::fuzz_target;
use libfuzzer_sys::arbitrary::{Arbitrary, Unstructured};
use seerdb::{DB, DBOptions};
use tempfile::TempDir;

#[derive(Debug, Clone)]
enum DBOp {
    Put { key: Vec<u8>, value: Vec<u8> },
    Get { key: Vec<u8> },
    Delete { key: Vec<u8> },
    Scan { start: Vec<u8>, end: Vec<u8> },
    Flush,
}

impl<'a> Arbitrary<'a> for DBOp {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let op_type: u8 = u.int_in_range(0..=4)?;

        match op_type {
            0 => {
                let key = u.arbitrary::<Vec<u8>>()?;
                let value = u.arbitrary::<Vec<u8>>()?;
                Ok(DBOp::Put { key, value })
            }
            1 => {
                let key = u.arbitrary::<Vec<u8>>()?;
                Ok(DBOp::Get { key })
            }
            2 => {
                let key = u.arbitrary::<Vec<u8>>()?;
                Ok(DBOp::Delete { key })
            }
            3 => {
                let start = u.arbitrary::<Vec<u8>>()?;
                let end = u.arbitrary::<Vec<u8>>()?;
                Ok(DBOp::Scan { start, end })
            }
            4 => Ok(DBOp::Flush),
            _ => unreachable!(),
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Parse fuzzed data into a sequence of DB operations
    let mut u = Unstructured::new(data);

    let ops: Vec<DBOp> = match u.arbitrary() {
        Ok(ops) => ops,
        Err(_) => return,
    };

    // Skip if no operations
    if ops.is_empty() {
        return;
    }

    // Create temporary database
    let temp_dir = match TempDir::new() {
        Ok(dir) => dir,
        Err(_) => return,
    };

    let db = match DB::open_default(temp_dir.path().to_str().unwrap()) {
        Ok(db) => db,
        Err(_) => return,
    };

    // Execute all operations
    // None should panic - all errors should be graceful
    for op in ops {
        match op {
            DBOp::Put { key, value } => {
                // Limit size to avoid OOM
                if key.len() > 64 * 1024 || value.len() > 1024 * 1024 {
                    continue;
                }
                let _ = db.put(&key, &value);
            }
            DBOp::Get { key } => {
                if key.len() > 64 * 1024 {
                    continue;
                }
                let _ = db.get(&key);
            }
            DBOp::Delete { key } => {
                if key.len() > 64 * 1024 {
                    continue;
                }
                let _ = db.delete(&key);
            }
            DBOp::Scan { start, end } => {
                if start.len() > 64 * 1024 || end.len() > 64 * 1024 {
                    continue;
                }
                let _ = db.scan(&start, &end);
            }
            DBOp::Flush => {
                let _ = db.flush();
            }
        }
    }

    // Database is automatically cleaned up when temp_dir is dropped
});
