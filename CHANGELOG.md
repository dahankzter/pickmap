# Changelog

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
