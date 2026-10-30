# Benchmarks

## Summary

**Hardware:** AMD Ryzen Threadripper PRO 9975WX (Zen 5), performance governor, boost on, pinned to one core. rustc 1.99.1, clang 23.1.3 for the C++ reference. Two runs agreed within 10% on every `pickmap` number.

**Reproduce:**

```sh
cargo build -p bench --release --bin report
taskset -c 4 target/release/report
```

**Method:**
- Throughput in ns per lookup. Every timed call sees a fresh random permutation of 8,192 queries, so the branch predictor can't memorize the sequence.
- Competitors are interleaved within each sample, and the reported value is the median over 101 samples.
- Branch misses per lookup are in parentheses.
- Every competitor is checked against `HashMap` on every query (`cargo test -p bench --release`).

**Results:**
- **Against Rust alternatives.** `pickmap` is the fastest on every key set and workload. The best of `phf`, `frozen-collections` and `quickphf` is:
  - 1.57-4.3x slower on hit-only;
  - 1.75-5.7x slower on miss-only;
  - 1.26-2.17x slower on a 50/50 mix.

  Plain `match` is slower still.
- **Against Lemire's C++ `perfect_hash`.** That library over-reads 16 bytes past the key, which `pickmap` never does. `pickmap` is:
  - up to 2.1x slower on hit-only and miss-only;
  - up to 1.23x slower on the mix;
  - faster on `hard_prefix` (500 keys of 26 bytes sharing a 14-byte prefix): 0.81x on hits and 0.34x on misses.
- **Branch misses.** `pickmap` takes 0.00 per lookup on hit-only and miss-only streams for every set, including near-miss-heavy ones. `cargo test -p bench --release --test branch_misses` guards this.
- **Against the phase 0 spike.** Three numbers are more than 10% slower:
  - `http_methods` hits: 1.14x;
  - `random1000` hits and misses: 1.18x and 1.15x.

  That is the price of never mispredicting. The spike compared key words with early exits, which is a little cheaper when misses look nothing like the keys, but costs a misprediction per lookup when they do: about 3x slower on `hard_prefix` misses. `pickmap` uses two compares instead:
  - Picked-byte maps compare length and all key words in one combined expression, then branch once.
  - PtHash maps check an 8-bit tag of the key's hash first, which is effectively the hit-or-miss question, then compare words only for likely hits.

**Remaining gaps:**
- Sets with a wide key-length range (`http_headers`, 2-28 bytes) need many loads. A cheaper representation could help.
- A branch-free lookup variant could help unpredictable streams.
- aarch64 numbers.

---

# pickmap benchmarks

- Date: 2026-10-30T12:02:50+01:00
- CPU: AMD Ryzen Threadripper PRO 9975WX 32-Cores (ran on cpu 4)
- Governor: performance
- Boost: 1
- rustc: rustc 1.99.1 (58cf8f9c3 2026-10-13)
- clang++: clang version 23.1.3
- Method: per sample, each competitor warms on one fresh permutation of the 8192 queries and is timed on another; competitors interleaved; median of 101 samples. Throughput only.

## Throughput (ns/lookup; branch misses/lookup in parentheses)

### http_methods

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.00 (0.00) | 1.79 (0.00) | 5.63 (0.50) |
| lemire-cpp | 1.01 (0.00) | 0.85 (0.01) | 4.64 (0.51) |
| quickphf | 12.77 (1.01) | 14.36 (1.32) | 17.41 (1.45) |
| match | 10.16 (1.24) | 6.83 (0.88) | 10.98 (1.41) |
| phf | 12.86 (0.94) | 16.27 (1.56) | 18.01 (1.54) |
| frozen-collections | 8.68 (0.69) | 9.28 (0.86) | 12.02 (1.10) |
| hashmap-foldhash | 4.71 (0.23) | 6.05 (0.59) | 9.11 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.74 (0.00) | 3.58 (0.00) | 7.49 (0.50) |
| lemire-cpp | 1.82 (0.01) | 1.78 (0.03) | 6.45 (0.53) |
| quickphf | 14.22 (1.45) | 14.52 (1.57) | 19.82 (1.93) |
| match | 13.49 (1.64) | 9.01 (0.96) | 12.78 (1.53) |
| phf | 16.07 (1.74) | 17.03 (2.02) | 21.29 (2.29) |
| frozen-collections | 10.46 (1.10) | 11.76 (1.59) | 13.68 (1.71) |
| hashmap-foldhash | 7.38 (0.69) | 13.22 (1.52) | 14.09 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.57 (0.00) | 2.34 (0.00) | 6.37 (0.50) |
| lemire-cpp | 1.72 (0.10) | 1.66 (0.14) | 5.48 (0.62) |
| quickphf | 10.43 (0.84) | 13.28 (1.22) | 16.38 (1.37) |
| match | 17.22 (1.78) | 12.24 (0.89) | 15.31 (1.42) |
| phf | 13.00 (0.91) | 15.89 (1.52) | 18.67 (1.57) |
| frozen-collections | 4.03 (0.39) | 4.10 (0.57) | 8.04 (0.92) |
| hashmap-foldhash | 6.05 (0.43) | 6.58 (0.67) | 10.41 (0.99) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.38 (0.00) | 1.92 (0.00) | 6.53 (0.50) |
| lemire-cpp | 1.58 (0.00) | 1.45 (0.02) | 6.42 (0.51) |
| quickphf | 13.37 (1.32) | 14.35 (1.49) | 19.79 (1.84) |
| match | 70.20 (2.02) | 94.25 (1.02) | 83.21 (1.58) |
| phf | 15.02 (1.45) | 16.08 (1.72) | 20.94 (2.02) |
| frozen-collections | 8.09 (0.57) | 10.86 (1.20) | 14.19 (1.46) |
| hashmap-foldhash | 5.86 (0.47) | 5.81 (0.64) | 10.41 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.30 (0.00) | 2.06 (0.00) | 5.89 (0.50) |
| lemire-cpp | 1.17 (0.00) | 1.09 (0.01) | 5.29 (0.51) |
| quickphf | 10.90 (0.87) | 14.04 (1.34) | 16.45 (1.42) |
| match | 9.99 (1.30) | 7.07 (0.86) | 10.59 (1.36) |
| phf | 12.50 (0.79) | 15.81 (1.49) | 18.22 (1.50) |
| frozen-collections | 8.57 (0.78) | 10.66 (1.39) | 12.09 (1.40) |
| hashmap-foldhash | 5.77 (0.45) | 7.00 (0.75) | 10.34 (1.02) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.38 (0.00) | 2.20 (0.00) | 6.12 (0.50) |
| lemire-cpp | 1.32 (0.00) | 1.31 (0.04) | 6.03 (0.59) |
| quickphf | 11.20 (0.94) | 12.99 (1.24) | 17.35 (1.48) |
| match | 15.11 (1.75) | 10.79 (0.91) | 13.94 (1.47) |
| phf | 13.73 (1.11) | 15.83 (1.57) | 19.48 (1.74) |
| frozen-collections | 7.49 (0.64) | 10.36 (1.29) | 11.61 (1.34) |
| hashmap-foldhash | 5.16 (0.34) | 6.57 (0.67) | 9.98 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.45 (0.00) | 2.17 (0.00) | 6.67 (0.50) |
| lemire-cpp | 1.70 (0.00) | 1.53 (0.02) | 5.41 (0.51) |
| quickphf | 9.56 (0.66) | 13.45 (1.21) | 15.46 (1.25) |
| match | 23.75 (1.70) | 19.86 (0.84) | 22.25 (1.38) |
| phf | 11.80 (0.59) | 16.11 (1.42) | 17.30 (1.31) |
| frozen-collections | 8.32 (0.71) | 10.96 (1.43) | 11.98 (1.40) |
| hashmap-foldhash | 5.49 (0.35) | 8.56 (0.89) | 10.74 (1.03) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.74 (0.00) | 2.03 (0.00) | 7.01 (0.50) |
| lemire-cpp | 1.68 (0.00) | 1.48 (0.01) | 6.60 (0.50) |
| quickphf | 13.37 (1.32) | 14.59 (1.49) | 20.26 (1.84) |
| match | 189.67 (2.05) | 290.19 (1.09) | 241.08 (1.57) |
| phf | 15.60 (1.48) | 16.28 (1.74) | 21.46 (2.04) |
| frozen-collections | 6.93 (0.39) | 9.69 (1.01) | 12.79 (1.30) |
| hashmap-foldhash | 6.31 (0.51) | 8.00 (0.87) | 11.52 (1.16) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.92 (0.00) | 2.29 (0.00) | 6.80 (0.50) |
| lemire-cpp | 3.61 (0.00) | 6.73 (0.40) | 8.39 (0.54) |
| quickphf | 8.71 (0.02) | 12.01 (1.03) | 13.25 (0.74) |
| match | 270.82 (2.43) | 198.19 (1.11) | 238.39 (1.98) |
| phf | 13.50 (0.00) | 14.83 (1.04) | 16.46 (0.71) |
| frozen-collections | 9.38 (0.53) | 11.05 (1.21) | 13.61 (1.22) |
| hashmap-foldhash | 4.87 (0.00) | 5.43 (0.48) | 9.12 (0.64) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 2.00 | 1.75 | **1.14** | 1.79 | 1.77 | 1.01 |
| http_headers | 3.74 | 3.44 | 1.09 | 3.58 | 3.46 | 1.04 |
| rust_keywords | 2.57 | 2.36 | 1.09 | 2.34 | 2.33 | 1.01 |
| random1000 | 2.38 | 2.01 | **1.18** | 1.92 | 1.66 | **1.15** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 2.00 | 8.68 | 4.34x | 1.97 |
| http_headers | 3.74 | 10.46 | 2.79x | 2.06 |
| rust_keywords | 2.57 | 4.03 | 1.57x | 1.49 |
| random1000 | 2.38 | 8.09 | 3.40x | 1.51 |
| url_schemes | 2.30 | 8.57 | 3.72x | 1.98 |
| js_reserved | 2.38 | 7.49 | 3.14x | 1.80 |
| tickers100 | 2.45 | 8.32 | 3.39x | 1.44 |
| random3000 | 2.74 | 6.93 | 2.53x | 1.64 |
| hard_prefix | 2.92 | 8.71 | 2.98x | 0.81 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.79 | 9.28 | 5.17x | 2.12 |
| http_headers | 3.58 | 11.76 | 3.28x | 2.01 |
| rust_keywords | 2.34 | 4.10 | 1.75x | 1.41 |
| random1000 | 1.92 | 10.86 | 5.67x | 1.33 |
| url_schemes | 2.06 | 10.66 | 5.16x | 1.90 |
| js_reserved | 2.20 | 10.36 | 4.70x | 1.68 |
| tickers100 | 2.17 | 10.96 | 5.06x | 1.42 |
| random3000 | 2.03 | 9.69 | 4.77x | 1.37 |
| hard_prefix | 2.29 | 11.05 | 4.81x | 0.34 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.63 | 12.02 | 2.14x | 1.21 |
| http_headers | 7.49 | 13.68 | 1.83x | 1.16 |
| rust_keywords | 6.37 | 8.04 | 1.26x | 1.16 |
| random1000 | 6.53 | 14.19 | 2.17x | 1.02 |
| url_schemes | 5.89 | 12.09 | 2.05x | 1.11 |
| js_reserved | 6.12 | 11.61 | 1.90x | 1.02 |
| tickers100 | 6.67 | 11.98 | 1.80x | 1.23 |
| random3000 | 7.01 | 12.79 | 1.83x | 1.06 |
| hard_prefix | 6.80 | 13.25 | 1.95x | 0.81 |

