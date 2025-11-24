
use crossbeam_skiplist::SkipMap;

fn main() {
    let map = SkipMap::new();
    map.insert(1, 10);
    map.insert(2, 20);
    map.insert(3, 30);

    println!("Forward:");
    for e in map.range(..) {
        println!("{} -> {}", e.key(), e.value());
    }

    println!("Reverse:");
    for e in map.range(..).rev() {
        println!("{} -> {}", e.key(), e.value());
    }
}
