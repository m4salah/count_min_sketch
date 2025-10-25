use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::marker::PhantomData;
use std::num::NonZeroUsize;

#[derive(Debug)]
pub struct CountMinSketch<K: Hash + Sync + Send + Eq> {
    width: usize,
    depth: usize,
    vec: Vec<Vec<u64>>,
    hash_builders: Vec<RandomState>,
    counter: usize,
    _phantom: PhantomData<K>,
}

impl<K: Hash + Sync + Send + Eq> CountMinSketch<K> {
    pub fn new(width: NonZeroUsize, depth: NonZeroUsize) -> Self {
        CountMinSketch {
            width: width.into(),
            depth: depth.into(),
            vec: vec![vec![0; width.into()]; depth.into()],
            hash_builders: (0..depth.into()).map(|_| RandomState::new()).collect(),
            _phantom: PhantomData,
            counter: 0,
        }
    }

    fn hash_with_seed(&self, key: &K, seed: usize) -> u64 {
        assert!(seed < self.depth);
        let mut hasher = self.hash_builders[seed].build_hasher();
        key.hash(&mut hasher);
        hasher.finish() % self.width as u64
    }

    pub fn store(&mut self, key: &K) {
        self.counter += 1;
        for depth_index in 0..self.depth {
            let hash = self.hash_with_seed(key, depth_index);
            self.vec[depth_index][hash as usize] =
                self.vec[depth_index][hash as usize].saturating_add(1)
        }
    }

    pub fn query(&self, key: &K) -> u64 {
        (0..self.depth)
            .map(|depth| {
                *self
                    .vec
                    .get(depth)
                    .unwrap()
                    .get(self.hash_with_seed(key, depth) as usize)
                    .unwrap()
            })
            .min()
            .unwrap()
    }

    pub fn total_count(&self) -> usize {
        self.counter
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
            let mut sketch = CountMinSketch::<String>::new(width, depth);
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
            let mut sketch = CountMinSketch::<String>::new(NonZeroUsize::new(100).unwrap(), NonZeroUsize::new(5).unwrap());
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
            let mut sketch = CountMinSketch::<String>::new(NonZeroUsize::new(100).unwrap(), NonZeroUsize::new(5).unwrap());
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
        let mut sketch = CountMinSketch::<String>::new(
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
        let mut sketch = CountMinSketch::<String>::new(
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

            let mut sketch = CountMinSketch::<u64>::new(
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
        let mut sketch = CountMinSketch::<usize>::new(
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
