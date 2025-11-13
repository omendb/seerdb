// Simple test to verify DB reopen works

use seerdb::{DB, DBOptions};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    println!("Writing data...");
    {
        let opts = DBOptions {
            data_dir: data_dir.clone(),
            ..Default::default()
        };

        let db = DB::open(opts)?;

        for i in 0..1000 {
            db.put(format!("key_{}", i).as_bytes(), b"value")?;
        }

        println!("Wrote 1000 keys");
    }

    println!("Reopening DB...");
    {
        let opts = DBOptions {
            data_dir: data_dir.clone(),
            ..Default::default()
        };

        let db = DB::open(opts)?;

        println!("Verifying data...");
        for i in 0..1000 {
            let key = format!("key_{}", i);
            assert!(db.get(key.as_bytes())?.is_some(), "Key {} not found", i);
        }

        println!("All data verified!");
    }

    Ok(())
}
