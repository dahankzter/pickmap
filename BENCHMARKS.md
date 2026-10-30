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
  - 1.5-4.4x slower on hit-only;
  - 1.76-5.6x slower on miss-only;
  - 1.26-2.16x slower on a 50/50 mix.

  Plain `match` is slower still.
- **Against Lemire's C++ `perfect_hash`.** That library over-reads 16 bytes past the key, which `pickmap` never does. `pickmap` is:
  - up to 2.1x slower on hit-only and miss-only;
  - up to 1.25x slower on the mix;
  - faster on `hard_prefix` (500 keys of 26 bytes sharing a 14-byte prefix): 0.79x on hits and 0.36x on misses.
- **Branch misses.** `pickmap` takes 0.00 per lookup on hit-only and miss-only streams for every set whose keys are 32 bytes or shorter, including near-miss-heavy ones. `cargo test -p bench --release --test branch_misses` guards this on Linux.
- **Keys over 32 bytes** (`long_paths`: 150 long URL paths sharing prefixes and suffixes, 50 short ones). Hashing and comparing keys of unbounded length depends on the input's length:
  - 0.28 branch misses per lookup on hits, 0.49 on misses;
  - still 1.46-2.41x faster than the best Rust alternative.

  The guard prints these numbers but doesn't enforce its limit for such maps.
- **`workload = "keyword_scan"`** (`pickmap-keyword-scan` rows). Small (picked-byte) maps compare key words with early exits:
  - 3-23% faster on most small sets;
  - `http_headers` misses show its risk at 0.10 mispredicts per lookup, even on these mostly unrelated misses.

  Large (PtHash) maps ignore the option.
- **Against the phase 0 spike.** `http_headers` is now slightly faster than the spike, thanks to the split key representation (9 loads instead of 11). Three numbers are still more than 10% slower:
  - `http_methods` hits: 1.16x;
  - `random1000` hits and misses: 1.19x and 1.16x.

  That is the price of never mispredicting. The spike compared key words with early exits, which is a little cheaper when misses look nothing like the keys, but costs a misprediction per lookup when they do: about 3x slower on `hard_prefix` misses. `pickmap` uses two compares instead:
  - Picked-byte maps compare length and all key words in one combined expression, then branch once.
  - PtHash maps check an 8-bit tag of the key's hash first, which is effectively the hit-or-miss question, then compare words only for likely hits.

**Remaining gaps:**
- aarch64 numbers: the code builds for aarch64 Linux and macOS, but nothing has been measured yet (README, Platforms).
- Keys over 32 bytes mispredict on mixed-length streams (above).
- A branch-free lookup variant could help unpredictable streams.

---

# pickmap benchmarks

- Date: 2026-10-30T13:37:52+01:00
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
| pickmap | 2.01 (0.00) | 1.80 (0.00) | 5.63 (0.50) |
| pickmap-keyword-scan | 1.87 (0.00) | 1.55 (0.00) | 5.25 (0.50) |
| lemire-cpp | 1.05 (0.00) | 0.87 (0.01) | 4.65 (0.51) |
| quickphf | 12.68 (1.01) | 14.34 (1.33) | 17.17 (1.45) |
| match | 10.28 (1.23) | 6.55 (0.88) | 10.77 (1.41) |
| phf | 12.93 (0.94) | 16.41 (1.56) | 18.23 (1.54) |
| frozen-collections | 8.98 (0.70) | 9.46 (0.86) | 12.34 (1.10) |
| hashmap-foldhash | 4.69 (0.23) | 6.02 (0.59) | 9.06 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.40 (0.00) | 3.34 (0.00) | 7.04 (0.50) |
| pickmap-keyword-scan | 3.11 (0.00) | 3.40 (0.10) | 6.95 (0.53) |
| lemire-cpp | 1.84 (0.01) | 1.77 (0.03) | 6.45 (0.52) |
| quickphf | 14.14 (1.45) | 14.39 (1.57) | 19.64 (1.93) |
| match | 13.73 (1.66) | 9.28 (0.97) | 13.26 (1.57) |
| phf | 16.12 (1.74) | 17.11 (2.02) | 21.56 (2.30) |
| frozen-collections | 10.12 (1.10) | 11.55 (1.61) | 13.53 (1.71) |
| hashmap-foldhash | 7.37 (0.69) | 13.26 (1.52) | 14.10 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.56 (0.00) | 2.35 (0.00) | 6.36 (0.50) |
| pickmap-keyword-scan | 2.44 (0.00) | 2.18 (0.00) | 6.13 (0.50) |
| lemire-cpp | 1.70 (0.10) | 1.67 (0.14) | 5.52 (0.62) |
| quickphf | 10.38 (0.84) | 13.10 (1.23) | 16.17 (1.36) |
| match | 17.13 (1.76) | 12.31 (0.88) | 15.45 (1.43) |
| phf | 12.97 (0.90) | 16.01 (1.53) | 18.80 (1.58) |
| frozen-collections | 4.01 (0.38) | 4.13 (0.57) | 8.03 (0.92) |
| hashmap-foldhash | 6.08 (0.43) | 6.59 (0.67) | 10.38 (0.99) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.39 (0.00) | 1.91 (0.00) | 6.67 (0.50) |
| pickmap-keyword-scan | 2.39 (0.00) | 1.91 (0.00) | 6.67 (0.50) |
| lemire-cpp | 1.58 (0.00) | 1.45 (0.02) | 6.41 (0.51) |
| quickphf | 13.44 (1.33) | 14.31 (1.49) | 19.93 (1.83) |
| match | 70.03 (2.10) | 93.45 (1.10) | 82.95 (1.62) |
| phf | 14.98 (1.45) | 16.14 (1.72) | 21.00 (2.02) |
| frozen-collections | 7.81 (0.57) | 10.71 (1.19) | 13.75 (1.45) |
| hashmap-foldhash | 5.97 (0.47) | 5.90 (0.64) | 10.49 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.32 (0.00) | 2.06 (0.00) | 5.91 (0.50) |
| pickmap-keyword-scan | 2.06 (0.00) | 1.59 (0.01) | 5.46 (0.50) |
| lemire-cpp | 1.19 (0.00) | 1.07 (0.01) | 5.33 (0.51) |
| quickphf | 11.41 (0.88) | 14.02 (1.34) | 16.59 (1.42) |
| match | 10.44 (1.31) | 7.54 (0.87) | 10.98 (1.36) |
| phf | 12.56 (0.80) | 15.90 (1.49) | 18.28 (1.50) |
| frozen-collections | 8.44 (0.79) | 10.53 (1.39) | 11.87 (1.40) |
| hashmap-foldhash | 5.73 (0.45) | 7.02 (0.75) | 10.33 (1.02) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.38 (0.00) | 2.21 (0.00) | 6.03 (0.50) |
| pickmap-keyword-scan | 2.16 (0.00) | 1.75 (0.01) | 5.75 (0.52) |
| lemire-cpp | 1.32 (0.00) | 1.32 (0.04) | 5.74 (0.54) |
| quickphf | 11.02 (0.94) | 12.91 (1.25) | 17.23 (1.48) |
| match | 14.80 (1.75) | 10.23 (0.90) | 13.48 (1.46) |
| phf | 13.65 (1.11) | 15.92 (1.58) | 19.51 (1.74) |
| frozen-collections | 7.28 (0.63) | 9.95 (1.28) | 11.29 (1.34) |
| hashmap-foldhash | 5.16 (0.34) | 6.59 (0.67) | 10.02 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.47 (0.00) | 2.17 (0.00) | 6.60 (0.50) |
| pickmap-keyword-scan | 2.47 (0.00) | 2.17 (0.00) | 6.60 (0.50) |
| lemire-cpp | 1.69 (0.00) | 1.53 (0.02) | 5.73 (0.51) |
| quickphf | 9.54 (0.67) | 13.55 (1.21) | 15.43 (1.24) |
| match | 23.95 (1.71) | 20.17 (0.88) | 22.43 (1.38) |
| phf | 11.84 (0.59) | 16.23 (1.42) | 17.59 (1.31) |
| frozen-collections | 8.52 (0.71) | 10.93 (1.44) | 12.20 (1.41) |
| hashmap-foldhash | 5.49 (0.35) | 8.55 (0.89) | 10.72 (1.03) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.75 (0.00) | 2.03 (0.00) | 7.02 (0.50) |
| pickmap-keyword-scan | 2.75 (0.00) | 2.03 (0.00) | 6.89 (0.50) |
| lemire-cpp | 1.69 (0.00) | 1.49 (0.01) | 6.74 (0.51) |
| quickphf | 13.48 (1.32) | 14.56 (1.50) | 20.31 (1.83) |
| match | 191.92 (2.15) | 298.31 (1.43) | 246.08 (1.78) |
| phf | 15.64 (1.48) | 16.41 (1.75) | 21.70 (2.04) |
| frozen-collections | 6.98 (0.39) | 9.61 (1.01) | 12.87 (1.30) |
| hashmap-foldhash | 6.34 (0.51) | 8.01 (0.87) | 11.56 (1.16) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.85 (0.00) | 2.33 (0.00) | 6.84 (0.50) |
| pickmap-keyword-scan | 2.85 (0.00) | 2.32 (0.00) | 6.84 (0.50) |
| lemire-cpp | 3.67 (0.00) | 6.66 (0.40) | 8.42 (0.54) |
| quickphf | 8.70 (0.02) | 11.96 (1.03) | 13.18 (0.74) |
| match | 267.97 (1.05) | 193.41 (0.39) | 234.65 (0.92) |
| phf | 13.37 (0.00) | 14.86 (1.04) | 16.62 (0.70) |
| frozen-collections | 9.35 (0.52) | 11.05 (1.20) | 13.56 (1.22) |
| hashmap-foldhash | 4.72 (0.00) | 5.44 (0.47) | 9.11 (0.64) |

### long_paths

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 6.77 (0.27) | 5.39 (0.47) | 10.35 (0.84) |
| pickmap-keyword-scan | 6.76 (0.27) | 5.38 (0.47) | 10.33 (0.83) |
| lemire-cpp | 3.44 (0.03) | 3.54 (0.05) | 8.23 (0.54) |
| quickphf | 10.14 (0.43) | 14.68 (1.49) | 15.14 (1.19) |
| match | 80.14 (1.41) | 56.98 (0.79) | 70.38 (1.34) |
| phf | 15.58 (0.41) | 18.23 (1.37) | 19.34 (1.09) |
| frozen-collections | 12.07 (0.86) | 12.85 (1.66) | 15.07 (1.58) |
| hashmap-foldhash | 6.49 (0.29) | 10.66 (1.14) | 12.27 (1.13) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 2.01 | 1.75 | **1.15** | 1.80 | 1.77 | 1.02 |
| http_headers | 3.40 | 3.44 | 0.99 | 3.34 | 3.46 | 0.97 |
| rust_keywords | 2.56 | 2.36 | 1.08 | 2.35 | 2.33 | 1.01 |
| random1000 | 2.39 | 2.01 | **1.19** | 1.91 | 1.66 | **1.15** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 2.01 | 8.98 | 4.48x | 1.90 |
| http_headers | 3.40 | 10.12 | 2.97x | 1.86 |
| rust_keywords | 2.56 | 4.01 | 1.57x | 1.51 |
| random1000 | 2.39 | 7.81 | 3.27x | 1.51 |
| url_schemes | 2.32 | 8.44 | 3.64x | 1.95 |
| js_reserved | 2.38 | 7.28 | 3.06x | 1.80 |
| tickers100 | 2.47 | 8.52 | 3.45x | 1.46 |
| random3000 | 2.75 | 6.98 | 2.54x | 1.63 |
| hard_prefix | 2.85 | 8.70 | 3.05x | 0.78 |
| long_paths | 6.77 | 10.14 | 1.50x | 1.97 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.80 | 9.46 | 5.25x | 2.07 |
| http_headers | 3.34 | 11.55 | 3.45x | 1.89 |
| rust_keywords | 2.35 | 4.13 | 1.76x | 1.41 |
| random1000 | 1.91 | 10.71 | 5.61x | 1.32 |
| url_schemes | 2.06 | 10.53 | 5.11x | 1.93 |
| js_reserved | 2.21 | 9.95 | 4.51x | 1.67 |
| tickers100 | 2.17 | 10.93 | 5.03x | 1.42 |
| random3000 | 2.03 | 9.61 | 4.73x | 1.36 |
| hard_prefix | 2.33 | 11.05 | 4.74x | 0.35 |
| long_paths | 5.39 | 12.85 | 2.39x | 1.52 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.63 | 12.34 | 2.19x | 1.21 |
| http_headers | 7.04 | 13.53 | 1.92x | 1.09 |
| rust_keywords | 6.36 | 8.03 | 1.26x | 1.15 |
| random1000 | 6.67 | 13.75 | 2.06x | 1.04 |
| url_schemes | 5.91 | 11.87 | 2.01x | 1.11 |
| js_reserved | 6.03 | 11.29 | 1.87x | 1.05 |
| tickers100 | 6.60 | 12.20 | 1.85x | 1.15 |
| random3000 | 7.02 | 12.87 | 1.83x | 1.04 |
| hard_prefix | 6.84 | 13.18 | 1.93x | 0.81 |
| long_paths | 10.35 | 15.07 | 1.46x | 1.26 |

