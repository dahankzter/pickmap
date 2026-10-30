//! Times every case and prints a markdown report with a go/no-go verdict.
//! Run pinned: `taskset -c 4 cargo run -p bench --release --bin report`.
//!
//! Every timed call sees a fresh random permutation of its batch, so the branch
//! predictor cannot memorize the query sequence. Competitors are interleaved within
//! each sample so drift hits them equally.

use std::collections::HashMap;
use std::hint::black_box;
use std::process::Command;
use std::time::Instant;

use bench::{Batch, Case, batch, cases};
use keysets::{QUERY_COUNT, SplitMix64, WORKLOADS, Workload};

const SAMPLES: usize = 101;
const SUSPICIOUS_NS: f64 = 0.3;

const COMPETITORS: [&str; 8] = [
    "pickmap",
    "pickmap-keyword-scan",
    "lemire-cpp",
    "quickphf",
    "match",
    "phf",
    "frozen-collections",
    "hashmap-foldhash",
];

/// `pickmap-spike` (round 3) hit and miss ns/lookup, from the spike's report.
const SPIKE_BASELINE: [(&str, f64, f64); 4] = [
    ("http_methods", 1.75, 1.77),
    ("http_headers", 3.44, 3.46),
    ("rust_keywords", 2.36, 2.33),
    ("random1000", 2.01, 1.66),
];

/// Hardware branch-miss counter: Linux perf events, or nothing on other systems.
#[cfg(target_os = "linux")]
struct MissCounter(Option<perf_event::Counter>);

#[cfg(target_os = "linux")]
impl MissCounter {
    fn new() -> Self {
        MissCounter(perf_event::Builder::new(perf_event::events::Hardware::BRANCH_MISSES).build().ok())
    }

    fn start(&mut self) {
        if let Some(c) = self.0.as_mut() {
            let _ = c.reset();
            let _ = c.enable();
        }
    }

    fn stop(&mut self) -> Option<u64> {
        let c = self.0.as_mut()?;
        let _ = c.disable();
        c.read().ok()
    }
}

#[cfg(not(target_os = "linux"))]
struct MissCounter;

#[cfg(not(target_os = "linux"))]
impl MissCounter {
    fn new() -> Self {
        MissCounter
    }

    fn start(&mut self) {}

    fn stop(&mut self) -> Option<u64> {
        None
    }
}

/// (ns per lookup, branch misses per lookup), medians over samples.
type Results = HashMap<(&'static str, &'static str, Workload), (f64, Option<f64>)>;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn sh(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_owned())
        .unwrap_or_else(|_| "n/a".into())
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).map(|s| s.trim().to_owned()).unwrap_or_else(|_| "n/a".into())
}

fn header() {
    let cpuinfo = read("/proc/cpuinfo");
    let model = cpuinfo.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).unwrap_or("n/a").trim();
    // Field 39 of /proc/self/stat is the CPU this thread last ran on.
    let stat = read("/proc/self/stat");
    let cpu = stat.rsplit(')').next().and_then(|s| s.split_whitespace().nth(36)).unwrap_or("0");
    println!("# pickmap benchmarks\n");
    println!("- Date: {}", sh("date", &["-Iseconds"]));
    println!("- CPU: {model} (ran on cpu {cpu})");
    println!("- Governor: {}", read(&format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor")));
    println!("- Boost: {}", read("/sys/devices/system/cpu/cpufreq/boost"));
    println!("- rustc: {}", sh("rustc", &["-V"]));
    println!("- clang++: {}", sh("clang++", &["--version"]));
    println!(
        "- Method: per sample, each competitor warms on one fresh permutation of the {QUERY_COUNT} queries and is timed on another; \
         competitors interleaved; median of {SAMPLES} samples. Throughput only.\n"
    );
}

fn measure(set: &keysets::KeySet, w: Workload, cases: &[&Case], rng: &mut SplitMix64, results: &mut Results) {
    let base = batch(set, w);
    let mut counter = MissCounter::new();
    let mut ns: Vec<Vec<f64>> = vec![Vec::new(); cases.len()];
    let mut misses: Vec<Vec<f64>> = vec![Vec::new(); cases.len()];
    for sample in 0..SAMPLES {
        for k in 0..cases.len() {
            // Rotate the starting competitor each sample.
            let i = (sample + k) % cases.len();
            let (warm, timed): (Batch, Batch) = (base.shuffled(rng), base.shuffled(rng));
            black_box((cases[i].tput)(black_box(&warm)));
            counter.start();
            let t = Instant::now();
            black_box((cases[i].tput)(black_box(&timed)));
            let dt = t.elapsed();
            if let Some(m) = counter.stop() {
                misses[i].push(m as f64 / QUERY_COUNT as f64);
            }
            ns[i].push(dt.as_nanos() as f64 / QUERY_COUNT as f64);
        }
    }
    for (i, c) in cases.iter().enumerate() {
        let m = (!misses[i].is_empty()).then(|| median(std::mem::take(&mut misses[i])));
        results.insert((c.set, c.competitor, w), (median(std::mem::take(&mut ns[i])), m));
    }
}

fn table(results: &Results, flags: &mut Vec<String>) {
    println!("## Throughput (ns/lookup; branch misses/lookup in parentheses)\n");
    for set in keysets::all() {
        println!("### {}\n\n| competitor | hit | miss | mixed |\n|---|---|---|---|", set.name);
        for comp in COMPETITORS {
            let cells: Vec<String> = WORKLOADS
                .iter()
                .map(|&w| match results.get(&(set.name, comp, w)) {
                    Some(&(ns, m)) => {
                        if ns < SUSPICIOUS_NS {
                            flags.push(format!("{} {comp} {w:?}: {ns:.2} ns, suspiciously fast", set.name));
                        }
                        match m {
                            Some(m) => format!("{ns:.2} ({m:.2})"),
                            None => format!("{ns:.2}"),
                        }
                    }
                    None => "n/a".into(),
                })
                .collect();
            println!("| {comp} | {} |", cells.join(" | "));
        }
        println!();
    }
}

fn baseline(results: &Results) {
    println!("## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)\n");
    println!("| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |\n|---|---|---|---|---|---|---|");
    for (set, hit, miss) in SPIKE_BASELINE {
        let get = |w| results.get(&(set, "pickmap", w)).map(|r| r.0).unwrap();
        let (h, m) = (get(Workload::Hit), get(Workload::Miss));
        let flag = |r: f64| if r > 1.10 { format!("**{r:.2}**") } else { format!("{r:.2}") };
        println!("| {set} | {h:.2} | {hit:.2} | {} | {m:.2} | {miss:.2} | {} |", flag(h / hit), flag(m / miss));
    }
    println!();
}

fn verdict(results: &Results) {
    println!("## Summary per workload\n");
    println!("`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.\n");
    for w in WORKLOADS {
        println!("### {w:?}\n\n| set | pickmap | best rival | speedup | pickmap / lemire |\n|---|---|---|---|---|");
        for set in keysets::all() {
            let s = set.name;
            let get = |c| results.get(&(s, c, w)).map(|r| r.0);
            let pick = get("pickmap").unwrap();
            let rival = ["phf", "frozen-collections", "quickphf"].iter().filter_map(|c| get(c)).fold(f64::MAX, f64::min);
            let lemire = get("lemire-cpp").map_or("n/a".into(), |l| format!("{:.2}", pick / l));
            println!("| {s} | {pick:.2} | {rival:.2} | {:.2}x | {lemire} |", rival / pick);
        }
        println!();
    }
}

fn main() {
    header();
    let all = cases();
    let mut rng = SplitMix64(0xBE7C);
    let mut results = Results::new();
    for set in keysets::all() {
        let cs: Vec<&Case> = all.iter().filter(|c| c.set == set.name).collect();
        for w in WORKLOADS {
            measure(&set, w, &cs, &mut rng, &mut results);
        }
    }
    let mut flags = Vec::new();
    table(&results, &mut flags);
    println!("## Flags\n");
    if flags.is_empty() {
        println!("None.\n");
    }
    for f in &flags {
        println!("- {f}");
    }
    baseline(&results);
    verdict(&results);
}
