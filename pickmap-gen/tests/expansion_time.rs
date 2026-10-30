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

/// `n` distinct uppercase keys of exactly `len` letters.
fn short_keys(n: usize, len: usize, seed: u64) -> Vec<String> {
    let mut rng = keysets::SplitMix64(seed);
    let mut seen = std::collections::HashSet::new();
    let mut keys = Vec::new();
    while keys.len() < n {
        let k: String = (0..len).map(|_| (b'A' + rng.below(26) as u8) as char).collect();
        if seen.insert(k.clone()) {
            keys.push(k);
        }
    }
    keys
}

/// Short keys clamp most candidate positions onto the same byte; the search must not
/// spend its budget on those duplicates.
#[test]
fn short_keys_plan_fast() {
    for (n, len) in [(249, 2), (300, 3)] {
        let owned = short_keys(n, len, 7);
        let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = Instant::now();
        pickmap_gen::plan::plan(&keys).expect("plan");
        let dt = t.elapsed();
        let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
        println!("{n} keys of {len} bytes ({profile}): {dt:?}");
        let limit = if cfg!(debug_assertions) { 1.0 } else { 0.25 };
        assert!(dt.as_secs_f64() < limit, "{n} keys of {len} bytes took {dt:?}");
    }
}
