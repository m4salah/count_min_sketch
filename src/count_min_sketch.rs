use std::hash::{DefaultHasher, Hash, Hasher};
use std::marker::PhantomData;

#[derive(Debug)]
pub struct CountMinSketch<K: Hash> {
    width: usize,
    depth: usize,
    vec: Vec<Vec<u64>>,
    _phantom: PhantomData<K>,
}

impl<K: Hash> CountMinSketch<K> {
    pub fn new(width: usize, depth: usize) -> Self {
        assert!(width > 0 && depth > 0, "Width and depth must be positive");
        CountMinSketch {
            width,
            depth,
            vec: vec![vec![0; width]; depth],
            _phantom: PhantomData,
        }
    }

    fn hash_with_seed(&self, key: &K, seed: usize) -> u64 {
        let mut s = DefaultHasher::new();
        seed.hash(&mut s);
        key.hash(&mut s);
        s.finish() % self.width as u64
    }

    pub fn store(&mut self, key: &K) {
        for depth_index in 0..self.depth {
            let hash = self.hash_with_seed(key, depth_index);
            self.vec[depth_index][hash as usize] +=
                self.vec[depth_index][hash as usize].saturating_add(1)
        }
    }

    pub fn query(&self, key: &K) -> u64 {
        (0..self.depth)
            .map(|depth| self.vec[depth][self.hash_with_seed(key, depth) as usize])
            .min()
            .unwrap()
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
            let mut sketch = CountMinSketch::<String>::new(100, 5);
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
            let mut sketch = CountMinSketch::<String>::new(100, 5);
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
