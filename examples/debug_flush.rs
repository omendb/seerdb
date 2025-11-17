use seerdb::{DBOptions, DB};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = PathBuf::from(temp_dir.path());

    println!("Data dir: {:?}", data_dir);

    let opts = DBOptions {
        data_dir: data_dir.clone(),
        ..Default::default()
    };
    let db = DB::open(opts).unwrap();

    db.put(b"key", b"value").unwrap();
    println!("After put");

    db.flush().unwrap();
    println!("After flush");

    drop(db);
    println!("After drop");

    // List files
    println!("\nFiles in {:?}:", data_dir);
    for entry in std::fs::read_dir(&data_dir).unwrap() {
        let entry = entry.unwrap();
        println!(
            "  {} ({} bytes)",
            entry.file_name().to_string_lossy(),
            entry.metadata().unwrap().len()
        );
    }
}
