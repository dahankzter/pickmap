# Changelog

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
