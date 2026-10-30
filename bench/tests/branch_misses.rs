//! Guards the spec's rule that `get` has no unpredictable branch before its final
//! hit/miss answer: on hit-only and miss-only streams in random order, a lookup must
//! take (almost) no branch misses.
//!
//! Maps with keys over 32 bytes are measured and printed but not enforced: hashing and
//! comparing keys of unbounded length takes length-dependent work, which mispredicts on
//! streams that mix input lengths (README, "Limits").
//!
//! Without hardware counters (a container, a VM, `perf_event_paranoid`) the guard fails
//! rather than passing unmeasured. Set `PICKMAP_NO_PERF_COUNTERS=1` to skip it on purpose.
//! It uses Linux perf events, so it only exists on Linux.
#![cfg(target_os = "linux")]

use bench::{batch, cases};
use keysets::{SplitMix64, Workload};
use perf_event::Builder;
use perf_event::events::Hardware;

const MAX_MISSES_PER_LOOKUP: f64 = 0.03;
const SKIP_VAR: &str = "PICKMAP_NO_PERF_COUNTERS";

#[test]
#[should_panic(expected = "PICKMAP_NO_PERF_COUNTERS")]
fn unavailable_counters_fail_without_opt_out() {
    on_unavailable(false);
}

#[test]
fn unavailable_counters_skip_with_opt_out() {
    on_unavailable(true);
}

#[test]
#[cfg_attr(debug_assertions, ignore = "release only")]
fn pickmap_has_no_data_dependent_branches() {
    let Ok(mut counter) = Builder::new(Hardware::BRANCH_MISSES).build() else {
        return on_unavailable(std::env::var_os(SKIP_VAR).is_some());
    };
    let cases = cases();
    let mut rng = SplitMix64(11);
    let mut failures = Vec::new();
    for set in keysets::all() {
        let enforced = set.keys.iter().all(|k| k.len() <= 32);
        for competitor in ["pickmap", "pickmap-get-in"] {
            let case = cases.iter().find(|c| c.set == set.name && c.competitor == competitor).expect("pickmap case");
            for w in [Workload::Hit, Workload::Miss] {
                let base = batch(&set, w);
                let (warm, timed) = (base.shuffled(&mut rng), base.shuffled(&mut rng));
                std::hint::black_box((case.tput)(&warm));
                counter.reset().unwrap();
                counter.enable().unwrap();
                std::hint::black_box((case.tput)(&timed));
                counter.disable().unwrap();
                let per = counter.read().unwrap() as f64 / timed.strs.len() as f64;
                let note = if enforced { "" } else { " (not enforced: keys over 32 bytes)" };
                println!("{} {competitor} {w:?}: {per:.3} branch misses per lookup{note}", set.name);
                if enforced && per > MAX_MISSES_PER_LOOKUP {
                    failures.push(format!("{} {competitor} {w:?}: {per:.3}", set.name));
                }
            }
        }
    }
    assert!(failures.is_empty(), "data-dependent branches (misses per lookup > {MAX_MISSES_PER_LOOKUP}): {failures:?}");
}

/// No counters: skip only when asked to, so CI can't pass this guard unmeasured.
fn on_unavailable(skip_requested: bool) {
    if skip_requested {
        eprintln!("perf counters unavailable; skipped because {SKIP_VAR} is set");
        return;
    }
    panic!("perf counters unavailable, so branch misses can't be measured; set {SKIP_VAR}=1 to skip this guard");
}
