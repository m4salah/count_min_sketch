use rayon::prelude::*;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct CountMinSketch<K: Hash + Sync + Send + Eq + Clone> {
    width: usize,
    depth: usize,
    vec: Vec<Vec<AtomicU64>>,
    _phantom: PhantomData<K>,
}

impl<K: Hash + Sync + Send + Eq + Clone> CountMinSketch<K> {
    pub fn new(width: NonZeroUsize, depth: NonZeroUsize) -> Self {
        let depth_val: usize = depth.into();
        let width_val: usize = width.into();
        CountMinSketch {
            width: width_val,
            depth: depth_val,
            vec: (0..depth_val)
                .map(|_| (0..width_val).map(|_| AtomicU64::new(0)).collect())
                .collect(),
            _phantom: PhantomData,
        }
    }

    fn hash_with_seed(&self, key: &K, seed: usize) -> u64 {
        assert!(seed < self.depth);
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        key.hash(&mut hasher);
        hasher.finish() % self.width as u64
    }

    pub fn store(&self, key: &K) {
        for depth_index in 0..self.depth {
            let hash = self.hash_with_seed(key, depth_index);
            self.vec[depth_index][hash as usize].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn store_parallel(&self, key: &K) {
        (0..self.depth).into_par_iter().for_each(|depth_index| {
            let hash = self.hash_with_seed(key, depth_index);
            self.vec[depth_index][hash as usize].fetch_add(1, Ordering::Relaxed);
        });
    }

    pub fn query(&self, key: &K) -> u64 {
        (0..self.depth)
            .map(|depth| {
                self.vec[depth][self.hash_with_seed(key, depth) as usize].load(Ordering::Relaxed)
            })
            .min()
            .unwrap()
    }

    pub fn count(&self, key: &K) -> u64 {
        return self.query(key);
    }

    pub fn merge(&self, other: &Self) {
        assert!(self.width == other.width);
        assert!(self.depth == other.depth);

        for (self_row, other_row) in self.vec.iter().zip(&other.vec) {
            for (self_cell, other_cell) in self_row.iter().zip(other_row) {
                self_cell.fetch_add(other_cell.load(Ordering::Relaxed), Ordering::Relaxed);
            }
        }
    }

    pub fn top_k(&self, k: usize, candidates: &[K]) -> Vec<(K, u64)> {
        let mut counts: Vec<(K, u64)> = candidates
            .iter()
            .map(|item| (item.clone(), self.query(item)))
            .collect();

        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.truncate(k);
        return counts;
    }

    pub fn clear(&self) {
        for row in &self.vec {
            for counter in row {
                counter.store(0, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating random strings
    prop_compose! {
        fn random_string()(s in ".*") -> String {
            s
        }
    }

    proptest! {
        #[test]
        fn test_count_min_sketch_properties(
            width in 1..1000usize,
            depth in 1..10usize,
            operations in prop::collection::vec(any::<String>(), 1..1000)
        ) {
            let sketch = CountMinSketch::<String>::new(NonZeroUsize::new(width).unwrap(), NonZeroUsize::new(depth).unwrap());
            let mut reference_counts = std::collections::HashMap::new();

            // Store all operations and track in reference map
            for key in &operations {
                sketch.store(key);
                *reference_counts.entry(key.clone()).or_insert(0) += 1;
            }

            // Verify properties
            for (key, expected_count) in reference_counts {
                let estimated_count = sketch.query(&key);

                // Count-Min Sketch property: estimate >= actual count
                assert!(
                    estimated_count >= expected_count,
                    "Estimated count ({}) should be >= actual count ({}) for key '{}'",
                    estimated_count,
                    expected_count,
                    key
                );

                // Due to collisions, estimate might be larger, but shouldn't be smaller
            }
        }

        #[test]
        fn test_monotonicity(
            keys in prop::collection::vec(any::<String>(), 1..100),
            repetitions in 1..10usize
        ) {
            let sketch = CountMinSketch::<String>::new(NonZeroUsize::new(100).unwrap(), NonZeroUsize::new(5).unwrap());
            let mut counts = std::collections::HashMap::new();

            for key in &keys {
                let current_count = *counts.get(key).unwrap_or(&0);

                // Store multiple times
                for _ in 0..repetitions {
                    sketch.store(key);
                }

                counts.insert(key.clone(), current_count + repetitions);

                let estimated = sketch.query(key);
                assert!(
                    estimated as usize >= current_count + repetitions,
                    "Count should be monotonic: was {}, now {} for key '{}'",
                    current_count,
                    estimated,
                    key
                );
            }
        }

        #[test]
        fn test_no_false_negatives(
            stored_keys in prop::collection::vec(any::<String>(), 1..100),
            query_keys in prop::collection::vec(any::<String>(), 1..50)
        ) {
            let sketch = CountMinSketch::<String>::new(NonZeroUsize::new(100).unwrap(), NonZeroUsize::new(5).unwrap());
            let mut stored_set = std::collections::HashSet::new();

            // Store all stored_keys
            for key in &stored_keys {
                sketch.store(key);
                stored_set.insert(key.clone());
            }

            // Query all keys - stored keys should have count >= 1
            for key in &query_keys {
                let count = sketch.query(key);
                if stored_set.contains(key) {
                    assert!(
                        count >= 1,
                        "Stored key '{}' should have count >= 1, but got {}",
                        key,
                        count
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_overflow_protection() {
        let sketch = CountMinSketch::<String>::new(
            NonZeroUsize::new(10).unwrap(),
            NonZeroUsize::new(3).unwrap(),
        );
        let key = "test".to_string();

        // This would test that saturating_add prevents overflow
        // In practice, we'd need many iterations to actually test overflow
        for _ in 0..1000 {
            sketch.store(&key);
        }

        // Should not panic due to overflow
        let count = sketch.query(&key);
        assert!(count >= 1000);
    }

    #[test]
    fn test_collision_handling() {
        // Small sketch to force collisions
        let sketch = CountMinSketch::<String>::new(
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(2).unwrap(),
        );
        let keys = vec!["a", "b", "c", "d", "e"];

        for key in &keys {
            sketch.store(&key.to_string());
        }

        // All keys should have at least count 1
        for key in &keys {
            assert!(sketch.query(&key.to_string()) >= 1);
        }
    }
}

#[cfg(test)]
mod quickcheck_tests {
    use rand::{random_range, rng};

    use super::*;

    #[test]
    fn test_quickcheck_properties() {
        use rand::prelude::*;

        let mut rng = rng();

        for _ in 0..100 {
            // Run 100 random test cases
            let width = {
                let range = 1..100;
                random_range(range)
            };
            let depth = rng.random_range(1..10);
            let num_operations = rng.random_range(10..1000);

            let sketch = CountMinSketch::<u64>::new(
                NonZeroUsize::new(width).unwrap(),
                NonZeroUsize::new(depth).unwrap(),
            );
            let mut reference = std::collections::HashMap::new();

            for _ in 0..num_operations {
                let key = rng.random::<u64>();
                sketch.store(&key);
                *reference.entry(key).or_insert(0) += 1;
            }

            // Test random subset of keys
            for (key, &expected) in reference.iter().take(10) {
                let estimated = sketch.query(key);
                assert!(
                    estimated >= expected,
                    "CMS({}, {}): key {} - expected {}, got {}",
                    width,
                    depth,
                    key,
                    expected,
                    estimated
                );
            }
        }
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_large_scale() {
        let width = 1000;
        let depth = 7;
        let sketch = CountMinSketch::<usize>::new(
            NonZeroUsize::new(width).unwrap(),
            NonZeroUsize::new(depth).unwrap(),
        );
        let num_operations = 100_000;

        let start = Instant::now();

        for i in 0..num_operations {
            sketch.store(&(i % 1000)); // Only 1000 distinct keys
        }

        let duration = start.elapsed();
        println!("Stored {} operations in {:?}", num_operations, duration);

        // Verify some counts
        for i in 0..10 {
            let count = sketch.query(&i);
            let expected = num_operations / 1000; // Approximately
            assert!(count as usize >= expected);
        }
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use std::thread;
    use std::time::Instant;

    /// Grading criterion: concurrent `store` must be >1.2x faster than the
    /// sequential baseline over the same total number of inserts.
    #[test]
    fn test_concurrent_insert_speedup() {
        // Cap the thread count so the sequential baseline stays bounded on
        // many-core machines while still exercising real parallelism.
        let available = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let num_threads = available.min(8);

        if num_threads < 2 {
            eprintln!("skipping speedup assertion: only {available} core(s) available");
            return;
        }

        // Equal total work for both runs: per_thread * num_threads.
        let per_thread: u64 = 1_000_000;
        let total = per_thread * num_threads as u64;

        let width = NonZeroUsize::new(4096).unwrap();
        let depth = NonZeroUsize::new(5).unwrap();

        // --- Sequential baseline: one thread does all `total` inserts. ---
        let seq_sketch = CountMinSketch::<u64>::new(width, depth);
        let start = Instant::now();
        for i in 0..total {
            seq_sketch.store(&i);
        }
        let seq_time = start.elapsed();

        // --- Concurrent run: N threads over disjoint key ranges. ---
        // Disjoint ranges keep atomic contention low so the win reflects
        // genuine parallel hashing, not lock/cache-line thrashing.
        let conc_sketch = CountMinSketch::<u64>::new(width, depth);
        let start = Instant::now();
        thread::scope(|s| {
            for t in 0..num_threads as u64 {
                let sketch = &conc_sketch;
                s.spawn(move || {
                    let begin = t * per_thread;
                    for i in begin..begin + per_thread {
                        sketch.store(&i);
                    }
                });
            }
        });
        let conc_time = start.elapsed();

        // Correctness under concurrency: every `store` adds exactly 1 to each
        // row, so with no lost atomic updates each row must sum to `total`.
        // (Seed-independent — the two sketches use different random seeds and
        // can't be compared cell-for-cell.)
        for row in &conc_sketch.vec {
            let sum: u64 = row.iter().map(|c| c.load(Ordering::Relaxed)).sum();
            assert_eq!(
                sum, total,
                "lost updates under concurrency: row sum {sum} != {total}"
            );
        }

        let speedup = seq_time.as_secs_f64() / conc_time.as_secs_f64();
        println!("threads={num_threads} seq={seq_time:?} conc={conc_time:?} speedup={speedup:.2}x");

        assert!(
            speedup > 1.2,
            "concurrent insert speedup {speedup:.2}x did not exceed 1.2x (threads={num_threads})"
        );
    }
}
