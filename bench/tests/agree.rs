use bench::{Batch, batch, cases};
use keysets::WORKLOADS;

/// Every competitor gives the reference answer for every single query.
#[test]
fn all_competitors_agree_per_query() {
    let cases = cases();
    for set in keysets::all() {
        let reference = cases
            .iter()
            .find(|c| c.set == set.name && c.competitor == "hashmap-foldhash")
            .expect("reference case");
        let competitors: Vec<_> = cases.iter().filter(|c| c.set == set.name).collect();
        assert!(competitors.len() >= 7, "{}: only {} competitors", set.name, competitors.len());
        for w in WORKLOADS {
            for q in batch(&set, w).strs {
                let one = Batch::from_strs(vec![q]);
                let want = (reference.tput)(&one);
                for c in &competitors {
                    assert_eq!((c.tput)(&one), want, "{} {} {w:?} {q:?}", c.set, c.competitor);
                }
            }
        }
    }
}

#[test]
fn shuffled_is_a_permutation() {
    let set = keysets::rust_keywords();
    let b = batch(&set, keysets::Workload::Mixed);
    let s = b.shuffled(&mut keysets::SplitMix64(9));
    assert_ne!(s.strs, b.strs);
    let (mut x, mut y) = (b.strs.clone(), s.strs.clone());
    x.sort_unstable();
    y.sort_unstable();
    assert_eq!(x, y);
    assert!(s.strs.iter().zip(&s.ptrs).zip(&s.lens).all(|((q, &p), &l)| q.as_ptr() == p && q.len() == l));
}

/// Whole batches: every query sits inside one long buffer, so `get_in` takes its fast
/// path (one-query batches above only reach its fallback).
#[test]
fn all_competitors_agree_on_whole_batches() {
    let cases = cases();
    for set in keysets::all() {
        let reference = cases.iter().find(|c| c.set == set.name && c.competitor == "hashmap-foldhash").unwrap();
        for w in WORKLOADS {
            let b = batch(&set, w);
            let want = (reference.tput)(&b);
            for c in cases.iter().filter(|c| c.set == set.name) {
                assert_eq!((c.tput)(&b), want, "{} {} {w:?}", c.set, c.competitor);
            }
        }
    }
}
