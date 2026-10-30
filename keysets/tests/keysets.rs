use std::collections::HashSet;

use keysets::{KeySet, QUERY_COUNT, Workload, all, queries};

fn key_set(set: &KeySet) -> HashSet<&str> {
    set.keys.iter().map(String::as_str).collect()
}

#[test]
fn sizes() {
    let sets = all();
    let lens: Vec<usize> = sets.iter().map(|s| s.keys.len()).collect();
    assert_eq!(lens, [9, 56, 53, 1000, 12, 46, 100, 3000, 500]);
    for set in &sets {
        assert_eq!(key_set(set).len(), set.keys.len(), "duplicate key in {}", set.name);
    }
}

#[test]
fn queries_deterministic() {
    for set in all() {
        for w in keysets::WORKLOADS {
            let a = queries(&set, w);
            assert_eq!(a, queries(&set, w));
            assert_eq!(a.len(), QUERY_COUNT);
        }
    }
}

#[test]
fn hit_queries_all_in_set() {
    for set in all() {
        let keys = key_set(&set);
        assert!(queries(&set, Workload::Hit).iter().all(|q| keys.contains(q.as_str())));
    }
}

#[test]
fn miss_queries_none_in_set() {
    for set in all() {
        let keys = key_set(&set);
        assert!(queries(&set, Workload::Miss).iter().all(|q| !keys.contains(q.as_str())));
    }
}

#[test]
fn miss_queries_include_edge_inputs() {
    for set in all() {
        let misses = queries(&set, Workload::Miss);
        for edge in ["", "x", "é"] {
            assert!(misses.iter().any(|q| q == edge), "{edge:?} missing in {}", set.name);
        }
    }
}

#[test]
fn mixed_is_roughly_half() {
    for set in all() {
        let keys = key_set(&set);
        let hits = queries(&set, Workload::Mixed).iter().filter(|q| keys.contains(q.as_str())).count();
        assert!((3600..=4600).contains(&hits), "{} hits in {}", hits, set.name);
    }
}

#[test]
fn hard_prefix_shape() {
    let set = keysets::hard_prefix();
    assert!(set.keys.iter().all(|k| k.len() == 26 && k.starts_with("/api/v1/items/")));
}
