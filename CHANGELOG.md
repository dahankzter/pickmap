# Changelog

## 0.3.0

- AVX-512: in builds compiled with AVX-512BW and AVX-512VL (e.g. `-C target-cpu=native` on
  Zen 4/5 or recent Intel), `get` and `get_in` read each key with one masked load that
  touches only the key's bytes. On small maps that makes `get` 25-45% faster on hits
  (HTTP headers 3.4 -> 1.9 ns) and up to 27% faster on misses. PtHash maps of 2 or 4
  words keep the scalar path, where the vector-to-register moves on the hash's critical
  path measured slower. Other builds are unchanged.

## 0.2.0

- `get_in(buf, range)`: looks up the key at `buf[range]`, as `get(&buf[range])` would,
  but reads it with one wide load from `buf` masked to the key's length, which is safe
  because every byte read is inside `buf`. 8-43% faster than `get` on hit-only and
  miss-only lookups in the benchmarks; level with Lemire's C++ on HTTP headers. Also on
  the `StaticMap` trait, with a default that calls `get`.
- Maps generate `get_in`'s tables inside `get_in` itself, so programs that don't call it
  don't carry them.
- The internal crates (`pickmap-core`, `pickmap-gen`, `pickmap-macros`) are now pinned to
  the exact same version, so a `cargo update` can't mix a newer macro with an older
  runtime.

## 0.1.0

First release.

- `static_map!` declares an immutable string map whose lookup is generated for its
  exact keys: a picked-byte perfect hash for small maps, PtHash for large ones.
- Lookups never read past the end of the input and don't mispredict on near-misses
  (keys up to 32 bytes).
- `get`, `contains_key`, `len`, `is_empty`, `iter` (declaration order), and the
  `StaticMap` trait for generic code.
- Attributes on maps: docs, `cfg`/`cfg_attr`, and
  `#[pickmap(workload = "general" | "keyword_scan" | "mostly_hits")]`.
- `no_std`, no allocation, Rust 1.88+.
