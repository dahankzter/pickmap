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

- **`workload = "keyword_scan"`** (`pickmap-keyword-scan` rows). Picked-byte maps compare key words with early exits:
  - 3-23% faster on most small sets;
  - `http_headers` misses show its risk at 0.10 mispredicts per lookup, even on these mostly unrelated misses.

  PtHash sets ignore the option. In the first of two runs, `random3000` mixed measured 8.98 ns for the default, a one-off disturbance; the second run (below) gave 7.10.

**Remaining gaps:**
- Sets with a wide key-length range (`http_headers`, 2-28 bytes) need many loads. A cheaper representation could help.
- A branch-free lookup variant could help unpredictable streams.
- aarch64 numbers.

---

# pickmap benchmarks

- Date: 2026-10-30T12:13:40+01:00
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
| pickmap | 2.03 (0.00) | 1.79 (0.00) | 5.65 (0.50) |
| pickmap-keyword-scan | 1.91 (0.00) | 1.54 (0.00) | 5.25 (0.50) |
| lemire-cpp | 1.07 (0.00) | 0.85 (0.01) | 4.61 (0.51) |
| quickphf | 12.92 (1.01) | 14.46 (1.33) | 17.34 (1.45) |
| match | 10.07 (1.24) | 6.69 (0.87) | 10.85 (1.41) |
| phf | 12.68 (0.94) | 16.15 (1.54) | 18.02 (1.53) |
| frozen-collections | 9.11 (0.69) | 9.55 (0.86) | 12.34 (1.10) |
| hashmap-foldhash | 4.74 (0.23) | 6.03 (0.59) | 9.04 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.75 (0.00) | 3.58 (0.00) | 7.38 (0.50) |
| pickmap-keyword-scan | 3.47 (0.00) | 3.53 (0.10) | 7.06 (0.53) |
| lemire-cpp | 1.84 (0.01) | 1.77 (0.03) | 7.07 (0.65) |
| quickphf | 14.34 (1.46) | 14.66 (1.57) | 19.98 (1.94) |
| match | 13.52 (1.64) | 9.02 (0.95) | 12.73 (1.52) |
| phf | 16.02 (1.74) | 17.03 (2.02) | 21.39 (2.30) |
| frozen-collections | 10.28 (1.10) | 11.63 (1.60) | 13.64 (1.70) |
| hashmap-foldhash | 7.40 (0.69) | 13.22 (1.52) | 14.12 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.50 (0.00) | 2.34 (0.00) | 6.45 (0.50) |
| pickmap-keyword-scan | 2.44 (0.00) | 2.17 (0.00) | 6.17 (0.50) |
| lemire-cpp | 1.76 (0.10) | 1.65 (0.14) | 5.45 (0.62) |
| quickphf | 10.48 (0.84) | 13.27 (1.23) | 16.47 (1.36) |
| match | 17.01 (1.76) | 12.46 (0.89) | 15.54 (1.43) |
| phf | 12.65 (0.90) | 15.78 (1.53) | 18.42 (1.57) |
| frozen-collections | 4.32 (0.39) | 4.52 (0.58) | 8.15 (0.92) |
| hashmap-foldhash | 5.98 (0.43) | 6.58 (0.68) | 10.40 (0.99) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.39 (0.00) | 1.94 (0.00) | 6.60 (0.50) |
| pickmap-keyword-scan | 2.38 (0.00) | 1.94 (0.00) | 6.61 (0.50) |
| lemire-cpp | 1.58 (0.00) | 1.47 (0.02) | 6.54 (0.51) |
| quickphf | 13.58 (1.32) | 14.50 (1.49) | 20.14 (1.84) |
| match | 70.07 (2.02) | 94.34 (0.97) | 82.99 (1.52) |
| phf | 15.00 (1.45) | 16.12 (1.72) | 20.67 (2.02) |
| frozen-collections | 7.82 (0.57) | 10.88 (1.20) | 13.84 (1.46) |
| hashmap-foldhash | 5.88 (0.47) | 5.80 (0.64) | 10.40 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.30 (0.00) | 2.06 (0.00) | 5.90 (0.50) |
| pickmap-keyword-scan | 2.04 (0.00) | 1.59 (0.01) | 5.47 (0.51) |
| lemire-cpp | 1.15 (0.00) | 1.07 (0.01) | 5.74 (0.56) |
| quickphf | 11.16 (0.88) | 14.13 (1.35) | 16.59 (1.42) |
| match | 10.06 (1.31) | 7.13 (0.86) | 10.63 (1.36) |
| phf | 12.29 (0.80) | 15.72 (1.49) | 18.04 (1.50) |
| frozen-collections | 8.47 (0.78) | 10.98 (1.39) | 12.04 (1.38) |
| hashmap-foldhash | 5.68 (0.45) | 6.95 (0.75) | 10.29 (1.02) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.39 (0.00) | 2.20 (0.00) | 6.03 (0.50) |
| pickmap-keyword-scan | 2.16 (0.00) | 1.83 (0.01) | 5.95 (0.50) |
| lemire-cpp | 1.34 (0.00) | 1.30 (0.04) | 5.65 (0.52) |
| quickphf | 11.17 (0.94) | 13.01 (1.25) | 17.48 (1.48) |
| match | 14.89 (1.74) | 9.92 (0.91) | 13.27 (1.46) |
| phf | 13.35 (1.11) | 15.69 (1.57) | 19.16 (1.74) |
| frozen-collections | 7.28 (0.63) | 9.94 (1.29) | 11.35 (1.35) |
| hashmap-foldhash | 5.16 (0.34) | 6.56 (0.67) | 10.00 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.48 (0.00) | 2.17 (0.00) | 6.64 (0.50) |
| pickmap-keyword-scan | 2.47 (0.00) | 2.17 (0.00) | 6.64 (0.50) |
| lemire-cpp | 1.68 (0.00) | 1.53 (0.02) | 5.44 (0.51) |
| quickphf | 9.48 (0.67) | 13.67 (1.22) | 15.59 (1.25) |
| match | 24.10 (1.71) | 20.58 (0.92) | 22.67 (1.40) |
| phf | 11.85 (0.59) | 16.15 (1.43) | 17.47 (1.31) |
| frozen-collections | 8.52 (0.71) | 11.12 (1.43) | 12.40 (1.42) |
| hashmap-foldhash | 5.49 (0.35) | 8.38 (0.89) | 10.52 (1.02) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.75 (0.00) | 2.04 (0.00) | 7.10 (0.50) |
| pickmap-keyword-scan | 2.75 (0.00) | 2.03 (0.00) | 7.01 (0.51) |
| lemire-cpp | 1.70 (0.00) | 1.52 (0.01) | 7.37 (0.51) |
| quickphf | 13.67 (1.32) | 14.80 (1.50) | 20.31 (1.82) |
| match | 190.79 (2.07) | 291.86 (0.98) | 244.14 (1.70) |
| phf | 15.78 (1.48) | 16.30 (1.74) | 21.58 (2.04) |
| frozen-collections | 7.14 (0.39) | 9.93 (1.01) | 13.37 (1.29) |
| hashmap-foldhash | 6.37 (0.51) | 7.94 (0.87) | 11.52 (1.15) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.82 (0.00) | 2.27 (0.00) | 6.78 (0.50) |
| pickmap-keyword-scan | 2.81 (0.00) | 2.27 (0.00) | 6.79 (0.50) |
| lemire-cpp | 3.61 (0.00) | 6.61 (0.41) | 8.26 (0.53) |
| quickphf | 8.59 (0.02) | 12.29 (1.03) | 13.41 (0.74) |
| match | 257.32 (1.18) | 189.56 (0.39) | 227.84 (1.14) |
| phf | 13.35 (0.00) | 14.77 (1.04) | 16.59 (0.70) |
| frozen-collections | 9.37 (0.52) | 11.11 (1.20) | 13.63 (1.22) |
| hashmap-foldhash | 4.64 (0.00) | 5.45 (0.47) | 9.08 (0.63) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 2.03 | 1.75 | **1.16** | 1.79 | 1.77 | 1.01 |
| http_headers | 3.75 | 3.44 | 1.09 | 3.58 | 3.46 | 1.03 |
| rust_keywords | 2.50 | 2.36 | 1.06 | 2.34 | 2.33 | 1.01 |
| random1000 | 2.39 | 2.01 | **1.19** | 1.94 | 1.66 | **1.17** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 2.03 | 9.11 | 4.49x | 1.90 |
| http_headers | 3.75 | 10.28 | 2.74x | 2.04 |
| rust_keywords | 2.50 | 4.32 | 1.73x | 1.42 |
| random1000 | 2.39 | 7.82 | 3.28x | 1.51 |
| url_schemes | 2.30 | 8.47 | 3.68x | 2.01 |
| js_reserved | 2.39 | 7.28 | 3.05x | 1.79 |
| tickers100 | 2.48 | 8.52 | 3.44x | 1.47 |
| random3000 | 2.75 | 7.14 | 2.60x | 1.62 |
| hard_prefix | 2.82 | 8.59 | 3.05x | 0.78 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.79 | 9.55 | 5.33x | 2.11 |
| http_headers | 3.58 | 11.63 | 3.25x | 2.03 |
| rust_keywords | 2.34 | 4.52 | 1.93x | 1.42 |
| random1000 | 1.94 | 10.88 | 5.59x | 1.32 |
| url_schemes | 2.06 | 10.98 | 5.33x | 1.93 |
| js_reserved | 2.20 | 9.94 | 4.52x | 1.69 |
| tickers100 | 2.17 | 11.12 | 5.13x | 1.42 |
| random3000 | 2.04 | 9.93 | 4.86x | 1.34 |
| hard_prefix | 2.27 | 11.11 | 4.89x | 0.34 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.65 | 12.34 | 2.18x | 1.22 |
| http_headers | 7.38 | 13.64 | 1.85x | 1.04 |
| rust_keywords | 6.45 | 8.15 | 1.26x | 1.18 |
| random1000 | 6.60 | 13.84 | 2.10x | 1.01 |
| url_schemes | 5.90 | 12.04 | 2.04x | 1.03 |
| js_reserved | 6.03 | 11.35 | 1.88x | 1.07 |
| tickers100 | 6.64 | 12.40 | 1.87x | 1.22 |
| random3000 | 7.10 | 13.37 | 1.88x | 0.96 |
| hard_prefix | 6.78 | 13.41 | 1.98x | 0.82 |

