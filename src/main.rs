use rayon::prelude::*;
use std::num::NonZeroUsize;

use count_min_sketch::count_min_sketch::CountMinSketch;

fn main() {
    use std::sync::Arc;
    use std::time::Instant;

    println!("Parallel Insert Strategies");
    println!("===========================\n");

    let sketch = Arc::new(CountMinSketch::new(
        NonZeroUsize::new(10000).unwrap(),
        NonZeroUsize::new(7).unwrap(),
    ));

    let items: Vec<String> = (0..100000).map(|i| format!("item_{}", i % 1000)).collect();

    // ============================================================
    // METHOD 1: Parallel across multiple insert() calls (BEST!)
    // ============================================================
    println!("1. RECOMMENDED: Parallel across items");
    {
        let sketch_clone = Arc::clone(&sketch);
        let start = Instant::now();

        // This parallelizes ACROSS items (good!)
        items.par_iter().for_each(|item| {
            sketch_clone.store_parallel(item); // Each insert is sequential internally
        });

        let duration = start.elapsed();
        println!("   Time: {:?}", duration);
        println!(
            "   Sample: item_0 = {}",
            sketch.count(&"item_0".to_string())
        );
        println!();
    }

    println!("2. Sequential across items");
    {
        let sketch_clone = Arc::clone(&sketch);
        let start = Instant::now();

        // This parallelizes ACROSS items (good!)
        items.par_iter().for_each(|item| {
            sketch_clone.store(item); // Each insert is sequential internally
        });

        let duration = start.elapsed();
        println!("   Time: {:?}", duration);
        println!(
            "   Sample: item_0 = {}",
            sketch.count(&"item_0".to_string())
        );
        println!();
    }
}
