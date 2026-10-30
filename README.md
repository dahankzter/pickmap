# pickmap

Compile-time perfect-hash string maps for Rust. `static_map!` reads a map's keys at
build time and generates a lookup specialized to them: a perfect hash over a few chosen
bytes, or a PtHash fallback for larger sets. The key compare is free of data-dependent
branches.

```rust
use pickmap::static_map;

#[derive(Debug, PartialEq)]
pub enum Method { Get, Post, Put, Delete }

static_map! {
    pub METHODS: Method;
    "GET" => Method::Get,
    "POST" => Method::Post,
    "PUT" => Method::Put,
    "DELETE" => Method::Delete,
}

fn main() {
    assert_eq!(METHODS.get("POST"), Some(&Method::Post));
    assert!(!METHODS.contains_key("PATCH"));
    assert_eq!(METHODS.len(), 4);
    for (key, value) in METHODS.iter() { /* declaration order */ }
}
```

## API

- `static_map! { [vis] NAME: ValueType; "key" => value, ... }` declares a `static`.
  - Keys are string literals; raw and escaped strings work.
  - Values are any expression valid in a `static` of `ValueType`, which must be `Sync`.
  - A duplicate key is a compile error.
- `get(&str) -> Option<&'static ValueType>`, `contains_key`, `len` and `is_empty`.
- `iter()` yields entries in declaration order.
- Each map has its own generated type, which implements `pickmap::StaticMap`, so generic code can take `&impl StaticMap<Value = V>`.

## Performance

On a Zen 5 machine, with shuffled lookups and branch misses counted, `pickmap` is the
fastest Rust option on every key set measured:
- 1.6-5x faster than the best of `phf`, `frozen-collections` and `quickphf` on hit-only and miss-only streams;
- 1.3-2.2x faster on a 50/50 mix;
- within about 2x of Daniel Lemire's C++ `perfect_hash`, which reads past the end of keys.

See [BENCHMARKS.md](BENCHMARKS.md) for the full tables and how to reproduce them.

Planning a 3,000-key map takes about 50 ms in an unoptimized build and 5 ms optimized,
so macro expansion stays fast without profile tweaks.

## Limits (version 1)

- String keys only, exact byte match.
- Only `get`. A branch-free variant for unpredictable streams may come later.
- Keys longer than 32 bytes work, through a slower byte-by-byte compare that only maps
  containing such keys pay for.

## Safety

`pickmap` never reads outside the key you pass in. Short inputs read a static zero
buffer instead, selected without branching. The only `unsafe` is an empty `asm!` block
used as an optimization barrier, the technique constant-time code uses. It emits no
instructions and touches no memory, and it stops LLVM from turning the branch-free
compare back into branches. Under Miri it is compiled out, and the test suite runs
clean there.

## Credit

- The picked-byte hash and the single padded compare come from Daniel Lemire's
  [Billion-query-per-second lookup tables in C++](https://lemire.me/blog/2026/10/26/billion-query-per-second-lookup-tables-in-c/)
  and [ConstexprCore/perfect_hash](https://github.com/ConstexprCore/perfect_hash) (MIT).
- Choosing a strategy per dataset follows .NET's `FrozenDictionary` and the
  [`frozen-collections`](https://github.com/geeknoid/frozen-collections) crate.
- The fallback is [PTHash](https://arxiv.org/abs/2104.10402).
