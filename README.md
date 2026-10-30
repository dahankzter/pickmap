# pickmap

[![crates.io](https://img.shields.io/crates/v/pickmap.svg)](https://crates.io/crates/pickmap)
[![docs.rs](https://img.shields.io/docsrs/pickmap)](https://docs.rs/pickmap)
[![license](https://img.shields.io/crates/l/pickmap.svg)](#license)

Fast, immutable string maps built at compile time.

`static_map!` looks at your keys while your crate compiles and generates a lookup made
for exactly those keys: a perfect hash over one to three well-chosen bytes for small
maps, or a PtHash table for large ones. The result is a plain `static` that never
allocates, works in `no_std`, and answers a typical lookup in 2-3.5 ns.

- **Fast.** 1.5-5.6x faster than `phf`, `frozen-collections` and `quickphf` on the
  [benchmarks](https://github.com/dahankzter/pickmap/blob/main/BENCHMARKS.md), and within
  about 2x of Daniel Lemire's C++ `perfect_hash`.
- **Predictable.** Lookups don't slow down on inputs that nearly match a key (shared
  prefixes, typos, hostile input). The key compare doesn't branch on partial matches,
  so the CPU never mispredicts it.
- **Safe.** No input byte is ever read past the end of the string. The only `unsafe` is
  an empty `asm!` optimization barrier.

## Installation

```sh
cargo add pickmap
```

## Example

```rust
use pickmap::static_map;

#[derive(Debug, PartialEq)]
pub enum Method { Get, Post, Put, Delete }

static_map! {
    /// HTTP methods we handle.
    pub METHODS: Method;
    "GET" => Method::Get,
    "POST" => Method::Post,
    "PUT" => Method::Put,
    "DELETE" => Method::Delete,
}

fn main() {
    assert_eq!(METHODS.get("POST"), Some(&Method::Post));
    assert_eq!(METHODS.get("PATCH"), None);
    assert_eq!(METHODS.len(), 4);
    for (key, method) in METHODS.iter() {
        println!("{key} -> {method:?}"); // declaration order
    }
}
```

## API

```text
static_map! { [attributes] [vis] NAME: ValueType; "key" => value, ... }
```

- **Keys** are string literals; escaped and raw strings work. A duplicate key is a
  compile error.
- **Values** are any expression valid in a `static` of `ValueType`, which must be
  `Sync`. Elided lifetimes are allowed (`&[u8]`, `Cow<'_, str>`), since a static's
  references are `'static` anyway.
- **Methods:** `get(&str) -> Option<&'static ValueType>`, `contains_key`, `len`,
  `is_empty`, and `iter()` in declaration order.
- **Generic code:** each map has its own generated type implementing
  [`pickmap::StaticMap`](https://docs.rs/pickmap/latest/pickmap/trait.StaticMap.html),
  so functions can take `&impl StaticMap<Value = V>`.
- **Attributes:** `///` docs and other attributes apply to the static.
  `#[cfg(...)]` and `#[cfg_attr(...)]` apply to everything the macro generates.

## Workloads

`#[pickmap(workload = "...")]` says what your lookups mostly look like. It is only a
performance hint; every workload returns the same answers.

| workload | use it for |
|---|---|
| `"general"` (default) | Any input: near-misses, typos, shared prefixes, input from users or the network. |
| `"keyword_scan"` | Most lookups miss with words unlike the keys, e.g. a lexer checking identifiers for keywords. |
| `"mostly_hits"` | Nearly every lookup finds a key, e.g. decoding names your own program wrote. |

```rust
#[derive(Debug)]
pub enum Keyword { Fn, Let, Match }

pickmap::static_map! {
    /// Rust keywords; most lookups are ordinary identifiers.
    #[pickmap(workload = "keyword_scan")]
    pub KEYWORDS: Keyword;
    "fn" => Keyword::Fn,
    "let" => Keyword::Let,
    "match" => Keyword::Match,
}
```

The last two make small maps compare keys with early exits: 3-23% faster when the hint
is right, but up to about 3x slower on misses that resemble keys. Large maps ignore the
hint, because their default compare is already as fast. When unsure, keep the default.

## Performance

Nanoseconds per lookup on an AMD Zen 5 (Threadripper PRO 9975WX), queries in a fresh
random order every run, so the branch predictor can't memorize them:

| key set | pickmap hit / miss | best of phf, frozen-collections, quickphf | Lemire's C++ |
|---|---|---|---|
| 9 HTTP methods | 2.0 / 1.8 | 9.0 / 9.5 | 1.1 / 0.9 |
| 56 HTTP headers | 3.4 / 3.3 | 10.1 / 11.6 | 1.8 / 1.8 |
| 53 Rust keywords | 2.6 / 2.4 | 4.0 / 4.1 | 1.7 / 1.7 |
| 1,000 random words | 2.4 / 1.9 | 7.8 / 10.7 | 1.6 / 1.4 |
| 500 paths with a shared prefix | 2.9 / 2.3 | 8.7 / 11.1 | 3.7 / 6.7 |

Lemire's library reads up to 16 bytes past the end of each key, which pickmap
deliberately never does; that is most of the remaining gap. Full tables, branch-miss
counts and how to reproduce them are in
[BENCHMARKS.md](https://github.com/dahankzter/pickmap/blob/main/BENCHMARKS.md).

Planning a map is fast enough that no build-profile tweaks are needed: in an
unoptimized build, 3,000 keys take about 70 ms.

## How it works

1. **Key value.** Every key, and every input, becomes 1, 2 or 4 machine words chosen per
   map: overlapping loads cover every byte, and inputs too short for a load read a
   static zero buffer instead. Two strings of the same length are equal exactly when
   their words are.
2. **Slot.** Small maps hash the length plus one to three picked bytes through small
   lookup tables, found by a backtracking search at compile time (Lemire's and gperf's
   idea). Large maps use PtHash: one multiply over the key value, then a per-bucket
   displacement.
3. **Compare.** Small maps compare the length and every word in one combined
   expression and branch once. Large maps first check an 8-bit tag of the key's hash;
   a near-miss has a different hash, so that branch is in effect "hit or miss?". Either
   way, no branch depends on how much of the input happens to match.

## Limits

- String keys only, exact byte match.
- Keys longer than 32 bytes work through a byte-by-byte compare. Their hashing and
  compare depend on the input length, so on streams mixing long and short inputs those
  maps mispredict about 0.3-0.5 times per lookup.
- No attributes on individual entries.

## Platforms

- `no_std`, with no allocation; builds for bare-metal targets such as
  `thumbv7m-none-eabi`.
- The optimization barrier is an `asm!` block on x86_64 and aarch64. Other targets,
  such as `wasm32`, get a plain function: lookups are still correct, only the
  no-misprediction guarantee needs the barrier.
- Checked builds: x86_64 Linux, aarch64 Linux and macOS, `wasm32`, `thumbv7m`. Tests and
  benchmarks have run on x86_64 so far.

## Safety

pickmap never reads outside the string you pass in. The only `unsafe` code is an empty
`asm!` block, the technique constant-time cryptography uses, that hides a value from the
optimizer so LLVM can't turn the branch-free compare back into branches. It emits no
instructions and touches no memory. Under Miri it is compiled out, and the test suite
runs clean there.

## Minimum supported Rust version

Rust 1.88.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this crate by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.

## Credit

- The picked-byte hash and the single padded compare come from Daniel Lemire's
  [Billion-query-per-second lookup tables in C++](https://lemire.me/blog/2026/10/26/billion-query-per-second-lookup-tables-in-c/)
  and [ConstexprCore/perfect_hash](https://github.com/ConstexprCore/perfect_hash) (MIT).
- Choosing a strategy per key set follows .NET's `FrozenDictionary` and the
  [`frozen-collections`](https://github.com/geeknoid/frozen-collections) crate.
- The large-map fallback is [PTHash](https://arxiv.org/abs/2104.10402).
