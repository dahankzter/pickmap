//! Guards the spec's rule that `get` has no data-dependent branch before its final
//! hit/miss branch: on hit-only and miss-only streams in random order, a lookup must
//! take (almost) no branch misses.

use bench::{batch, cases};
use keysets::{SplitMix64, Workload};
use perf_event::Builder;
use perf_event::events::Hardware;

const MAX_MISSES_PER_LOOKUP: f64 = 0.03;

#[test]
#[cfg_attr(debug_assertions, ignore = "release only")]
fn pickmap_has_no_data_dependent_branches() {
    let Ok(mut counter) = Builder::new(Hardware::BRANCH_MISSES).build() else {
        eprintln!("perf counters unavailable; skipping");
        return;
    };
    let cases = cases();
    let mut rng = SplitMix64(11);
    let mut failures = Vec::new();
    for set in keysets::all() {
        let case = cases.iter().find(|c| c.set == set.name && c.competitor == "pickmap").expect("pickmap case");
        for w in [Workload::Hit, Workload::Miss] {
            let base = batch(&set, w);
            let (warm, timed) = (base.shuffled(&mut rng), base.shuffled(&mut rng));
            std::hint::black_box((case.tput)(&warm));
            counter.reset().unwrap();
            counter.enable().unwrap();
            std::hint::black_box((case.tput)(&timed));
            counter.disable().unwrap();
            let per = counter.read().unwrap() as f64 / timed.strs.len() as f64;
            println!("{} {w:?}: {per:.3} branch misses per lookup", set.name);
            if per > MAX_MISSES_PER_LOOKUP {
                failures.push(format!("{} {w:?}: {per:.3}", set.name));
            }
        }
    }
    assert!(failures.is_empty(), "data-dependent branches (misses per lookup > {MAX_MISSES_PER_LOOKUP}): {failures:?}");
}
