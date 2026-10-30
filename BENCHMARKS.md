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
  - 1.6-4.3x slower on hit-only;
  - 1.75-5.1x slower on miss-only;
  - 1.26-2.16x slower on a 50/50 mix.

  Plain `match` is slower still.
- **Against Lemire's C++ `perfect_hash`.** That library over-reads 16 bytes past the key, which `pickmap` never does. `pickmap` is:
  - 1.37-2.08x slower on hit-only and miss-only for most sets;
  - 1.04-1.23x slower on the mix;
  - faster on `hard_prefix` (500 keys of 26 bytes sharing a 14-byte prefix).
- **Branch misses.** `pickmap` takes 0.00 per lookup on hit-only and miss-only streams for every set. `cargo test -p bench --release --test branch_misses` guards this.
- **Against the phase 0 spike.** Most numbers are within 10%. Three are not: `http_methods` hits (1.15x), and `random1000` hits and misses (1.19x and 1.28x). Two plausible, unconfirmed causes:
  - the optimization barrier now on the final compare, needed to stop LLVM splitting it into per-word branches;
  - the value now living in a separate `VALUES` array instead of in the slot.

**Remaining gaps:**
- Sets with a wide key-length range (`http_headers`, 2-28 bytes) need many loads. A cheaper representation could help.
- A branch-free lookup variant could help unpredictable streams.
- The spike regressions above.
- aarch64 numbers.

---

# pickmap benchmarks

- Date: 2026-10-30T10:47:40+01:00
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
| pickmap | 2.01 (0.00) | 1.81 (0.00) | 5.55 (0.50) |
| lemire-cpp | 1.06 (0.00) | 0.87 (0.01) | 4.63 (0.51) |
| quickphf | 13.01 (1.01) | 14.73 (1.33) | 17.57 (1.45) |
| match | 10.15 (1.24) | 6.86 (0.88) | 11.00 (1.41) |
| phf | 12.87 (0.94) | 16.42 (1.56) | 18.08 (1.54) |
| frozen-collections | 8.69 (0.70) | 9.27 (0.86) | 12.01 (1.10) |
| hashmap-foldhash | 4.73 (0.23) | 6.05 (0.59) | 9.12 (0.78) |

### http_headers

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.77 (0.00) | 3.56 (0.00) | 7.53 (0.50) |
| lemire-cpp | 1.82 (0.01) | 1.76 (0.03) | 6.44 (0.52) |
| quickphf | 14.55 (1.45) | 14.77 (1.56) | 19.82 (1.93) |
| match | 13.42 (1.63) | 9.07 (0.95) | 12.80 (1.52) |
| phf | 16.11 (1.74) | 17.07 (2.02) | 21.37 (2.29) |
| frozen-collections | 10.29 (1.10) | 11.47 (1.59) | 13.52 (1.70) |
| hashmap-foldhash | 7.53 (0.70) | 13.51 (1.52) | 14.44 (1.58) |

### rust_keywords

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.56 (0.00) | 2.34 (0.00) | 6.37 (0.50) |
| lemire-cpp | 1.69 (0.10) | 1.67 (0.14) | 5.47 (0.61) |
| quickphf | 10.75 (0.84) | 13.46 (1.22) | 16.61 (1.37) |
| match | 17.18 (1.77) | 12.13 (0.89) | 15.30 (1.43) |
| phf | 13.00 (0.91) | 16.12 (1.53) | 18.74 (1.57) |
| frozen-collections | 4.02 (0.39) | 4.11 (0.57) | 8.03 (0.91) |
| hashmap-foldhash | 6.07 (0.43) | 6.57 (0.67) | 10.42 (0.99) |

### random1000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.38 (0.00) | 2.12 (0.00) | 6.73 (0.50) |
| lemire-cpp | 1.58 (0.00) | 1.45 (0.02) | 6.46 (0.51) |
| quickphf | 13.73 (1.32) | 14.67 (1.49) | 20.27 (1.86) |
| match | 69.29 (2.02) | 93.15 (0.91) | 82.32 (1.56) |
| phf | 15.01 (1.45) | 16.13 (1.72) | 20.96 (2.02) |
| frozen-collections | 7.88 (0.57) | 10.78 (1.20) | 13.97 (1.46) |
| hashmap-foldhash | 5.91 (0.47) | 5.79 (0.64) | 10.43 (1.03) |

### url_schemes

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.25 (0.00) | 2.10 (0.00) | 5.95 (0.50) |
| lemire-cpp | 1.16 (0.00) | 1.08 (0.01) | 5.26 (0.51) |
| quickphf | 11.24 (0.87) | 14.41 (1.35) | 16.76 (1.42) |
| match | 9.97 (1.31) | 7.12 (0.86) | 10.60 (1.37) |
| phf | 12.56 (0.79) | 15.92 (1.49) | 18.20 (1.49) |
| frozen-collections | 8.57 (0.79) | 10.66 (1.39) | 12.15 (1.40) |
| hashmap-foldhash | 5.89 (0.45) | 7.01 (0.75) | 10.40 (1.02) |

### js_reserved

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.37 (0.00) | 2.22 (0.00) | 6.12 (0.50) |
| lemire-cpp | 1.32 (0.00) | 1.31 (0.04) | 5.66 (0.52) |
| quickphf | 11.47 (0.94) | 13.37 (1.25) | 17.60 (1.48) |
| match | 15.45 (1.74) | 11.17 (0.90) | 14.30 (1.47) |
| phf | 13.75 (1.11) | 15.92 (1.57) | 19.45 (1.74) |
| frozen-collections | 7.31 (0.64) | 10.34 (1.29) | 11.55 (1.34) |
| hashmap-foldhash | 5.45 (0.34) | 6.55 (0.67) | 10.05 (0.94) |

### tickers100

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.33 (0.00) | 2.24 (0.00) | 6.65 (0.50) |
| lemire-cpp | 1.70 (0.00) | 1.54 (0.02) | 5.41 (0.51) |
| quickphf | 9.96 (0.66) | 13.94 (1.22) | 16.05 (1.27) |
| match | 24.69 (1.70) | 20.94 (0.83) | 23.23 (1.35) |
| phf | 11.79 (0.59) | 16.22 (1.42) | 17.45 (1.31) |
| frozen-collections | 8.55 (0.71) | 10.92 (1.43) | 12.14 (1.40) |
| hashmap-foldhash | 5.53 (0.35) | 8.58 (0.90) | 10.75 (1.03) |

### random3000

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 2.69 (0.00) | 2.28 (0.00) | 7.25 (0.50) |
| lemire-cpp | 1.68 (0.00) | 1.52 (0.01) | 6.63 (0.51) |
| quickphf | 14.00 (1.32) | 14.93 (1.49) | 20.54 (1.82) |
| match | 189.18 (2.02) | 289.29 (1.01) | 242.09 (1.60) |
| phf | 15.64 (1.48) | 16.48 (1.74) | 21.66 (2.03) |
| frozen-collections | 6.99 (0.39) | 9.95 (1.01) | 13.22 (1.30) |
| hashmap-foldhash | 6.36 (0.51) | 7.97 (0.87) | 11.56 (1.16) |

### hard_prefix

| competitor | hit | miss | mixed |
|---|---|---|---|
| pickmap | 3.08 (0.00) | 2.84 (0.00) | 7.18 (0.50) |
| lemire-cpp | 3.63 (0.00) | 6.74 (0.40) | 8.40 (0.54) |
| quickphf | 8.59 (0.02) | 12.31 (1.04) | 13.42 (0.74) |
| match | 250.03 (1.04) | 186.97 (0.39) | 220.79 (0.91) |
| phf | 13.31 (0.00) | 14.89 (1.04) | 16.62 (0.70) |
| frozen-collections | 9.46 (0.53) | 11.45 (1.20) | 13.94 (1.22) |
| hashmap-foldhash | 4.65 (0.00) | 5.49 (0.47) | 9.17 (0.63) |

## Flags

None.

## Against the spike (hit / miss ns/lookup; ratio over 1.10 flagged)

| set | pickmap hit | spike hit | ratio | pickmap miss | spike miss | ratio |
|---|---|---|---|---|---|---|
| http_methods | 2.01 | 1.75 | **1.15** | 1.81 | 1.77 | 1.02 |
| http_headers | 3.77 | 3.44 | 1.09 | 3.56 | 3.46 | 1.03 |
| rust_keywords | 2.56 | 2.36 | 1.09 | 2.34 | 2.33 | 1.01 |
| random1000 | 2.38 | 2.01 | **1.19** | 2.12 | 1.66 | **1.28** |

## Summary per workload

`best rival` is the fastest of `phf`, `frozen-collections` and `quickphf`.

### Hit

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 2.01 | 8.69 | 4.31x | 1.91 |
| http_headers | 3.77 | 10.29 | 2.73x | 2.07 |
| rust_keywords | 2.56 | 4.02 | 1.57x | 1.51 |
| random1000 | 2.38 | 7.88 | 3.31x | 1.51 |
| url_schemes | 2.25 | 8.57 | 3.81x | 1.95 |
| js_reserved | 2.37 | 7.31 | 3.09x | 1.79 |
| tickers100 | 2.33 | 8.55 | 3.67x | 1.37 |
| random3000 | 2.69 | 6.99 | 2.60x | 1.60 |
| hard_prefix | 3.08 | 8.59 | 2.79x | 0.85 |

### Miss

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 1.81 | 9.27 | 5.11x | 2.08 |
| http_headers | 3.56 | 11.47 | 3.22x | 2.02 |
| rust_keywords | 2.34 | 4.11 | 1.75x | 1.41 |
| random1000 | 2.12 | 10.78 | 5.09x | 1.46 |
| url_schemes | 2.10 | 10.66 | 5.09x | 1.94 |
| js_reserved | 2.22 | 10.34 | 4.66x | 1.69 |
| tickers100 | 2.24 | 10.92 | 4.87x | 1.46 |
| random3000 | 2.28 | 9.95 | 4.37x | 1.50 |
| hard_prefix | 2.84 | 11.45 | 4.03x | 0.42 |

### Mixed

| set | pickmap | best rival | speedup | pickmap / lemire |
|---|---|---|---|---|
| http_methods | 5.55 | 12.01 | 2.16x | 1.20 |
| http_headers | 7.53 | 13.52 | 1.80x | 1.17 |
| rust_keywords | 6.37 | 8.03 | 1.26x | 1.17 |
| random1000 | 6.73 | 13.97 | 2.07x | 1.04 |
| url_schemes | 5.95 | 12.15 | 2.04x | 1.13 |
| js_reserved | 6.12 | 11.55 | 1.89x | 1.08 |
| tickers100 | 6.65 | 12.14 | 1.82x | 1.23 |
| random3000 | 7.25 | 13.22 | 1.82x | 1.09 |
| hard_prefix | 7.18 | 13.42 | 1.87x | 0.86 |

