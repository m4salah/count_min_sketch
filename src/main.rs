use std::num::NonZeroUsize;

use count_min_sketch::count_min_sketch::CountMinSketch;

fn main() {
    let mut cms = CountMinSketch::new(
        NonZeroUsize::new(100).unwrap(),
        NonZeroUsize::new(5).unwrap(),
    );
    cms.store(&"mohamed".to_string());
    cms.store(&"abdelkhalek".to_string());
    cms.store(&"salah".to_string());

    println!("{}", cms.query(&String::from("salah")))
}
