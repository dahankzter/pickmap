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
  - 1.55-4.2x slower on hit-only;
  - 1.7-5.05x slower on miss-only;
  - 1.25-2.1x slower on a 50/50 mix.

  Plain `match` is slower still.
- **Against Lemire's C++ `perfect_hash`.** That library over-reads 16 bytes past the key, which `pickmap` never does. `pickmap` is:
  - 1.44-2.19x slower on hit-only and miss-only for most sets;
  - 1.05-1.22x slower on the mix;
  - faster on `hard_prefix` (500 keys of 26 bytes sharing a 14-byte prefix).
- **Branch misses.** `pickmap` takes 0.00 per lookup on hit-only and miss-only streams for every set. `cargo test -p bench --release --test branch_misses` guards this.
- **Against the phase 0 spike.** Most numbers are within 10%. Four are not:
  - `http_methods` hits: 1.19x;
  - `http_headers` hits: 1.11x;
  - `random1000` hits and misses: 1.22x and 1.30x.

  Three plausible, unconfirmed causes:
  - the optimization barrier now on the final compare, needed to stop LLVM splitting it into per-word branches;
  - the value now living in a separate `VALUES` array instead of in the slot;
  - the input-length clamp that keeps a 4 GiB input from matching an empty slot.

**Remaining gaps:**
- Sets with a wide key-length range (`http_headers`, 2-28 bytes) need many loads. A cheaper representation could help.
- A branch-free lookup variant could help unpredictable streams.
- The spike regressions above.
- aarch64 numbers.

---

# pickmap benchmarks

- Date: 2026-10-30T11:03:08+01:00
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
| pickmap | 2.09 (0.00) | 1.89 (0.00) | 5.69 (0.50) |
| lemire-cpp | 1.04 (0.00) | 0.86 (0.01) | 4.66 (0.51) |
| quickphf | 12.61 (1.01) | 14.42 (1.33) | 17.35 (1.45) |
| match | 10.33 (1.23) | 7.24 (0.88) | 11.27 (1.41) |
| phf | 12.86 (0.94) | 16.28 (1.56) | 17.98 (1.53) |
| frozen-collections | 8.72 (0.70) | 9.21 (0.86) | 11.98 (1.10) |
| hashmap-foldhash | 4.82 (0.23) | 6.09 (0.59) | 9.25 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.82 (0.00) | 3.66 (0.00) | 7.62 (0.50) |
| lemire-cpp | 1.82 (0.01) | 1.75 (0.03) | 6.45 (0.52) |
| quickphf | 14.15 (1.45) | 14.48 (1.56) | 19.70 (1.93) |
| match | 13.25 (1.64) | 9.18 (0.95) | 12.84 (1.54) |
| phf | 16.07 (1.74) | 17.00 (2.02) | 21.33 (2.29) |
| frozen-collections | 10.28 (1.10) | 11.61 (1.60) | 13.55 (1.71) |
| hashmap-foldhash | 7.35 (0.69) | 13.19 (1.52) | 13.98 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.59 (0.00) | 2.41 (0.00) | 6.40 (0.50) |
| lemire-cpp | 1.69 (0.10) | 1.67 (0.14) | 5.44 (0.62) |
| quickphf | 10.43 (0.84) | 13.19 (1.22) | 16.49 (1.38) |
| match | 16.72 (1.76) | 11.54 (0.89) | 14.77 (1.42) |
| phf | 12.99 (0.91) | 16.04 (1.53) | 18.75 (1.57) |
| frozen-collections | 4.01 (0.39) | 4.08 (0.57) | 8.02 (0.91) |
| hashmap-foldhash | 6.06 (0.43) | 6.60 (0.68) | 10.40 (0.99) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.46 (0.00) | 2.15 (0.00) | 6.80 (0.50) |
| lemire-cpp | 1.60 (0.00) | 1.45 (0.02) | 6.47 (0.51) |
| quickphf | 13.50 (1.32) | 14.35 (1.49) | 19.79 (1.84) |
| match | 70.01 (2.07) | 94.47 (1.15) | 82.99 (1.59) |
| phf | 15.03 (1.45) | 16.05 (1.73) | 20.92 (2.02) |
| frozen-collections | 7.92 (0.57) | 10.87 (1.19) | 13.87 (1.46) |
| hashmap-foldhash | 5.86 (0.47) | 5.82 (0.64) | 10.51 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.26 (0.00) | 2.11 (0.00) | 5.90 (0.50) |
| lemire-cpp | 1.18 (0.00) | 1.07 (0.01) | 5.36 (0.51) |
| quickphf | 10.89 (0.87) | 14.01 (1.35) | 16.30 (1.42) |
| match | 10.10 (1.30) | 7.13 (0.87) | 10.62 (1.36) |
| phf | 12.56 (0.79) | 15.80 (1.50) | 18.16 (1.50) |
| frozen-collections | 8.61 (0.79) | 10.41 (1.38) | 11.75 (1.37) |
| hashmap-foldhash | 5.90 (0.45) | 7.00 (0.75) | 10.44 (1.02) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.44 (0.00) | 2.26 (0.00) | 6.19 (0.50) |
| lemire-cpp | 1.32 (0.00) | 1.31 (0.04) | 5.75 (0.55) |
| quickphf | 11.24 (0.94) | 13.00 (1.24) | 17.46 (1.48) |
| match | 15.11 (1.75) | 10.67 (0.91) | 13.84 (1.47) |
| phf | 13.66 (1.11) | 15.86 (1.57) | 19.43 (1.75) |
| frozen-collections | 7.51 (0.63) | 10.02 (1.29) | 11.73 (1.36) |
| hashmap-foldhash | 5.23 (0.34) | 6.59 (0.67) | 10.15 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.70 (0.00) | 2.22 (0.00) | 6.73 (0.50) |
| lemire-cpp | 1.71 (0.00) | 1.54 (0.02) | 5.71 (0.51) |
| quickphf | 9.60 (0.66) | 13.53 (1.21) | 15.65 (1.26) |
| match | 24.59 (1.70) | 20.70 (0.84) | 23.31 (1.38) |
| phf | 11.83 (0.60) | 16.17 (1.42) | 17.51 (1.31) |
| frozen-collections | 8.54 (0.71) | 11.04 (1.43) | 12.33 (1.41) |
| hashmap-foldhash | 5.63 (0.35) | 8.49 (0.89) | 10.72 (1.03) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.76 (0.00) | 2.45 (0.00) | 7.15 (0.50) |
| lemire-cpp | 1.68 (0.00) | 1.49 (0.01) | 6.72 (0.51) |
| quickphf | 13.40 (1.32) | 14.61 (1.49) | 20.16 (1.83) |
| match | 189.28 (2.03) | 289.70 (1.09) | 241.86 (1.64) |
| phf | 15.61 (1.48) | 16.26 (1.74) | 21.55 (2.04) |
| frozen-collections | 7.14 (0.39) | 9.90 (1.01) | 13.39 (1.29) |
| hashmap-foldhash | 6.50 (0.51) | 7.97 (0.87) | 11.63 (1.16) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.17 (0.00) | 2.89 (0.00) | 7.28 (0.50) |
| lemire-cpp | 3.63 (0.00) | 6.70 (0.40) | 8.41 (0.54) |
| quickphf | 8.64 (0.02) | 12.16 (1.03) | 13.33 (0.74) |
| match | 252.98 (1.04) | 188.03 (0.39) | 223.85 (0.91) |
| phf | 13.25 (0.00) | 14.86 (1.04) | 16.55 (0.70) |
| frozen-collections | 9.37 (0.52) | 11.57 (1.21) | 13.85 (1.22) |
| hashmap-foldhash | 4.64 (0.00) | 5.44 (0.48) | 9.03 (0.63) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 2.09 | 1.75 | **1.19** | 1.89 | 1.77 | 1.07 |
| http_headers | 3.82 | 3.44 | **1.11** | 3.66 | 3.46 | 1.06 |
| rust_keywords | 2.59 | 2.36 | 1.10 | 2.41 | 2.33 | 1.03 |
| random1000 | 2.46 | 2.01 | **1.22** | 2.15 | 1.66 | **1.30** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 2.09 | 8.72 | 4.18x | 2.00 |
| http_headers | 3.82 | 10.28 | 2.69x | 2.10 |
| rust_keywords | 2.59 | 4.01 | 1.55x | 1.53 |
| random1000 | 2.46 | 7.92 | 3.22x | 1.53 |
| url_schemes | 2.26 | 8.61 | 3.81x | 1.91 |
| js_reserved | 2.44 | 7.51 | 3.08x | 1.85 |
| tickers100 | 2.70 | 8.54 | 3.17x | 1.58 |
| random3000 | 2.76 | 7.14 | 2.59x | 1.64 |
| hard_prefix | 3.17 | 8.64 | 2.73x | 0.87 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.89 | 9.21 | 4.88x | 2.19 |
| http_headers | 3.66 | 11.61 | 3.17x | 2.09 |
| rust_keywords | 2.41 | 4.08 | 1.70x | 1.45 |
| random1000 | 2.15 | 10.87 | 5.05x | 1.48 |
| url_schemes | 2.11 | 10.41 | 4.94x | 1.97 |
| js_reserved | 2.26 | 10.02 | 4.43x | 1.73 |
| tickers100 | 2.22 | 11.04 | 4.98x | 1.44 |
| random3000 | 2.45 | 9.90 | 4.04x | 1.64 |
| hard_prefix | 2.89 | 11.57 | 4.01x | 0.43 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.69 | 11.98 | 2.11x | 1.22 |
| http_headers | 7.62 | 13.55 | 1.78x | 1.18 |
| rust_keywords | 6.40 | 8.02 | 1.25x | 1.18 |
| random1000 | 6.80 | 13.87 | 2.04x | 1.05 |
| url_schemes | 5.90 | 11.75 | 1.99x | 1.10 |
| js_reserved | 6.19 | 11.73 | 1.90x | 1.08 |
| tickers100 | 6.73 | 12.33 | 1.83x | 1.18 |
| random3000 | 7.15 | 13.39 | 1.87x | 1.06 |
| hard_prefix | 7.28 | 13.33 | 1.83x | 0.86 |

