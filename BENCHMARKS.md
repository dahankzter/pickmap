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
- **`get_in`** (`pickmap-get-in` rows): the same queries looked up inside one buffer, one space apart, as a lexer or parser would see them. One wide load from the buffer, masked to the key, replaces `get`'s piecewise loads:
  - 8-43% faster than `get` on hit-only and miss-only streams;
  - level with Lemire on `http_headers` (1.96 / 1.79 vs 1.86 / 1.77);
  - about the same as `get` on the 50/50 mix, where the hit/miss mispredict dominates.
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

- Date: 2026-10-30T14:21:30+01:00
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
| pickmap | 1.99 (0.00) | 1.82 (0.00) | 5.54 (0.50) |
| pickmap-get-in | 1.42 (0.00) | 1.28 (0.00) | 5.36 (0.50) |
| pickmap-keyword-scan | 1.87 (0.00) | 1.57 (0.00) | 5.31 (0.50) |
| lemire-cpp | 1.01 (0.00) | 0.83 (0.01) | 4.62 (0.51) |
| quickphf | 12.91 (1.01) | 14.65 (1.32) | 17.49 (1.45) |
| match | 10.08 (1.24) | 6.90 (0.88) | 11.09 (1.42) |
| phf | 13.24 (0.94) | 16.64 (1.56) | 18.46 (1.54) |
| frozen-collections | 8.72 (0.70) | 9.30 (0.87) | 12.10 (1.10) |
| hashmap-foldhash | 4.81 (0.23) | 5.97 (0.58) | 9.28 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.50 (0.00) | 3.34 (0.00) | 7.02 (0.50) |
| pickmap-get-in | 1.96 (0.00) | 1.85 (0.00) | 7.08 (0.50) |
| pickmap-keyword-scan | 3.22 (0.00) | 3.39 (0.10) | 7.00 (0.53) |
| lemire-cpp | 1.88 (0.01) | 1.77 (0.03) | 7.06 (0.65) |
| quickphf | 14.55 (1.44) | 14.86 (1.56) | 19.83 (1.92) |
| match | 13.75 (1.64) | 9.41 (0.96) | 13.24 (1.54) |
| phf | 16.26 (1.74) | 17.36 (2.03) | 21.76 (2.30) |
| frozen-collections | 10.20 (1.10) | 11.54 (1.60) | 13.60 (1.72) |
| hashmap-foldhash | 7.51 (0.69) | 12.81 (1.52) | 13.93 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.55 (0.00) | 2.38 (0.00) | 6.36 (0.50) |
| pickmap-get-in | 1.93 (0.00) | 1.83 (0.00) | 6.01 (0.50) |
| pickmap-keyword-scan | 2.49 (0.00) | 2.21 (0.00) | 6.18 (0.50) |
| lemire-cpp | 1.77 (0.10) | 1.64 (0.14) | 5.45 (0.62) |
| quickphf | 10.73 (0.84) | 13.29 (1.22) | 16.43 (1.37) |
| match | 16.59 (1.78) | 11.22 (0.89) | 14.66 (1.46) |
| phf | 13.18 (0.91) | 16.21 (1.53) | 18.96 (1.59) |
| frozen-collections | 3.99 (0.38) | 4.10 (0.57) | 7.79 (0.92) |
| hashmap-foldhash | 6.29 (0.43) | 6.56 (0.67) | 10.69 (1.00) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.40 (0.00) | 1.93 (0.00) | 6.58 (0.50) |
| pickmap-get-in | 2.00 (0.00) | 1.68 (0.00) | 6.65 (0.50) |
| pickmap-keyword-scan | 2.40 (0.00) | 1.93 (0.00) | 6.56 (0.50) |
| lemire-cpp | 1.58 (0.00) | 1.45 (0.02) | 6.54 (0.51) |
| quickphf | 13.83 (1.33) | 14.68 (1.49) | 20.21 (1.85) |
| match | 70.04 (2.03) | 94.29 (1.02) | 83.50 (1.60) |
| phf | 15.25 (1.45) | 16.48 (1.73) | 21.06 (2.02) |
| frozen-collections | 7.82 (0.56) | 10.77 (1.20) | 13.79 (1.46) |
| hashmap-foldhash | 5.96 (0.47) | 5.76 (0.64) | 10.44 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.32 (0.00) | 2.06 (0.00) | 5.90 (0.50) |
| pickmap-get-in | 1.44 (0.00) | 1.33 (0.00) | 5.88 (0.50) |
| pickmap-keyword-scan | 2.06 (0.00) | 1.59 (0.01) | 5.47 (0.50) |
| lemire-cpp | 1.15 (0.00) | 1.08 (0.01) | 5.71 (0.55) |
| quickphf | 11.07 (0.87) | 14.24 (1.34) | 16.47 (1.41) |
| match | 10.00 (1.31) | 7.19 (0.86) | 10.63 (1.36) |
| phf | 12.69 (0.79) | 16.20 (1.50) | 18.58 (1.51) |
| frozen-collections | 8.50 (0.79) | 11.17 (1.39) | 12.42 (1.40) |
| hashmap-foldhash | 5.85 (0.45) | 6.89 (0.75) | 10.50 (1.01) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.41 (0.00) | 2.23 (0.00) | 6.06 (0.50) |
| pickmap-get-in | 1.78 (0.00) | 1.68 (0.00) | 6.35 (0.50) |
| pickmap-keyword-scan | 2.19 (0.00) | 1.80 (0.01) | 5.83 (0.50) |
| lemire-cpp | 1.32 (0.00) | 1.31 (0.04) | 5.64 (0.52) |
| quickphf | 11.39 (0.94) | 13.27 (1.24) | 17.52 (1.48) |
| match | 14.97 (1.75) | 10.10 (0.90) | 13.41 (1.47) |
| phf | 13.88 (1.12) | 16.14 (1.58) | 19.41 (1.75) |
| frozen-collections | 7.48 (0.64) | 10.07 (1.30) | 11.60 (1.35) |
| hashmap-foldhash | 5.23 (0.34) | 6.49 (0.67) | 10.03 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.46 (0.00) | 2.17 (0.00) | 6.62 (0.50) |
| pickmap-get-in | 1.79 (0.00) | 1.57 (0.00) | 6.02 (0.50) |
| pickmap-keyword-scan | 2.45 (0.00) | 2.17 (0.00) | 6.64 (0.50) |
| lemire-cpp | 1.70 (0.00) | 1.52 (0.02) | 5.44 (0.51) |
| quickphf | 9.48 (0.66) | 13.68 (1.20) | 15.52 (1.23) |
| match | 24.77 (1.78) | 20.05 (0.84) | 22.87 (1.42) |
| phf | 11.92 (0.58) | 16.49 (1.42) | 17.73 (1.31) |
| frozen-collections | 8.50 (0.71) | 11.11 (1.44) | 12.35 (1.42) |
| hashmap-foldhash | 5.50 (0.35) | 8.40 (0.89) | 10.70 (1.02) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.71 (0.00) | 2.02 (0.00) | 6.93 (0.50) |
| pickmap-get-in | 2.37 (0.00) | 1.74 (0.00) | 6.81 (0.50) |
| pickmap-keyword-scan | 2.71 (0.00) | 2.02 (0.00) | 7.04 (0.51) |
| lemire-cpp | 1.68 (0.00) | 1.49 (0.01) | 6.65 (0.51) |
| quickphf | 13.72 (1.32) | 15.00 (1.50) | 20.23 (1.83) |
| match | 189.38 (2.07) | 290.30 (1.23) | 242.47 (1.76) |
| phf | 15.77 (1.48) | 16.66 (1.74) | 21.92 (2.03) |
| frozen-collections | 7.12 (0.39) | 9.88 (1.00) | 13.42 (1.29) |
| hashmap-foldhash | 6.46 (0.51) | 7.72 (0.87) | 11.54 (1.16) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.80 (0.00) | 2.30 (0.00) | 6.75 (0.50) |
| pickmap-get-in | 2.58 (0.00) | 2.05 (0.00) | 6.76 (0.50) |
| pickmap-keyword-scan | 2.80 (0.00) | 2.29 (0.00) | 6.75 (0.50) |
| lemire-cpp | 3.60 (0.01) | 6.57 (0.39) | 8.35 (0.54) |
| quickphf | 8.59 (0.02) | 12.49 (1.03) | 13.40 (0.74) |
| match | 250.08 (1.04) | 187.19 (0.39) | 222.03 (0.91) |
| phf | 13.37 (0.00) | 15.01 (1.04) | 16.49 (0.71) |
| frozen-collections | 9.35 (0.52) | 11.04 (1.21) | 13.56 (1.22) |
| hashmap-foldhash | 4.69 (0.00) | 5.59 (0.47) | 9.32 (0.63) |

### long_paths

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 6.72 (0.27) | 5.21 (0.47) | 10.10 (0.84) |
| pickmap-get-in | 6.48 (0.27) | 5.45 (0.48) | 10.18 (0.83) |
| pickmap-keyword-scan | 6.72 (0.27) | 5.21 (0.47) | 10.07 (0.83) |
| lemire-cpp | 3.40 (0.02) | 3.28 (0.03) | 8.85 (0.68) |
| quickphf | 10.13 (0.43) | 15.03 (1.49) | 15.44 (1.19) |
| match | 79.31 (1.42) | 55.21 (0.80) | 69.53 (1.36) |
| phf | 15.51 (0.41) | 18.07 (1.37) | 19.24 (1.09) |
| frozen-collections | 12.21 (0.86) | 12.71 (1.65) | 15.25 (1.58) |
| hashmap-foldhash | 6.37 (0.29) | 10.42 (1.13) | 12.22 (1.12) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 1.99 | 1.75 | **1.14** | 1.82 | 1.77 | 1.03 |
| http_headers | 3.50 | 3.44 | 1.02 | 3.34 | 3.46 | 0.96 |
| rust_keywords | 2.55 | 2.36 | 1.08 | 2.38 | 2.33 | 1.02 |
| random1000 | 2.40 | 2.01 | **1.19** | 1.93 | 1.66 | **1.16** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.99 | 8.72 | 4.38x | 1.97 |
| http_headers | 3.50 | 10.20 | 2.92x | 1.86 |
| rust_keywords | 2.55 | 3.99 | 1.56x | 1.44 |
| random1000 | 2.40 | 7.82 | 3.26x | 1.52 |
| url_schemes | 2.32 | 8.50 | 3.66x | 2.02 |
| js_reserved | 2.41 | 7.48 | 3.10x | 1.83 |
| tickers100 | 2.46 | 8.50 | 3.46x | 1.44 |
| random3000 | 2.71 | 7.12 | 2.63x | 1.61 |
| hard_prefix | 2.80 | 8.59 | 3.06x | 0.78 |
| long_paths | 6.72 | 10.13 | 1.51x | 1.98 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.82 | 9.30 | 5.11x | 2.19 |
| http_headers | 3.34 | 11.54 | 3.46x | 1.88 |
| rust_keywords | 2.38 | 4.10 | 1.72x | 1.45 |
| random1000 | 1.93 | 10.77 | 5.59x | 1.33 |
| url_schemes | 2.06 | 11.17 | 5.42x | 1.91 |
| js_reserved | 2.23 | 10.07 | 4.51x | 1.71 |
| tickers100 | 2.17 | 11.11 | 5.12x | 1.42 |
| random3000 | 2.02 | 9.88 | 4.89x | 1.35 |
| hard_prefix | 2.30 | 11.04 | 4.80x | 0.35 |
| long_paths | 5.21 | 12.71 | 2.44x | 1.59 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.54 | 12.10 | 2.18x | 1.20 |
| http_headers | 7.02 | 13.60 | 1.94x | 0.99 |
| rust_keywords | 6.36 | 7.79 | 1.22x | 1.17 |
| random1000 | 6.58 | 13.79 | 2.10x | 1.01 |
| url_schemes | 5.90 | 12.42 | 2.11x | 1.03 |
| js_reserved | 6.06 | 11.60 | 1.92x | 1.07 |
| tickers100 | 6.62 | 12.35 | 1.87x | 1.22 |
| random3000 | 6.93 | 13.42 | 1.94x | 1.04 |
| hard_prefix | 6.75 | 13.40 | 1.98x | 0.81 |
| long_paths | 10.10 | 15.25 | 1.51x | 1.14 |

