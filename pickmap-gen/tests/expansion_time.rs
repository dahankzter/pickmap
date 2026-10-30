//! How long planning (the expensive part of macro expansion) takes for large sets.
//! The spec targets under 1 s for 3,000 keys, even unoptimized if reachable.

use std::time::Instant;

#[test]
fn plan_3000_keys_time() {
    for set in [keysets::random3000(), keysets::hard_prefix()] {
        let keys: Vec<&str> = set.keys.iter().map(String::as_str).collect();
        let t = Instant::now();
        pickmap_gen::plan::plan(&keys).expect("plan");
        let dt = t.elapsed();
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        println!("{} ({} keys, {profile}): {dt:?}", set.name, keys.len());
        if cfg!(not(debug_assertions)) {
            assert!(dt.as_secs_f64() < 1.0, "{} took {dt:?}", set.name);
        }
    }
}
