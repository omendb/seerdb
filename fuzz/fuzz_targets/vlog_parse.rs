#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    // Skip empty inputs
    if data.is_empty() {
        return;
    }

    // Write fuzzed data to a temporary vLog file
    let mut temp_file = match NamedTempFile::new() {
        Ok(f) => f,
        Err(_) => return,
    };

    if temp_file.write_all(data).is_err() {
        return;
    }

    let path = temp_file.path();

    // Try to open and read the fuzzed vLog
    // We expect this to fail gracefully with an error, not panic
    if let Ok(vlog) = seerdb::vlog::VLog::open(path) {
        // Try to read a value at a random offset
        // The offset is derived from the fuzzed data
        if data.len() >= 8 {
            let offset = u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);

            // Try to read - should handle invalid offsets gracefully
            let _ = vlog.read(offset);
        }
    }

    // File is automatically cleaned up when temp_file is dropped
});
