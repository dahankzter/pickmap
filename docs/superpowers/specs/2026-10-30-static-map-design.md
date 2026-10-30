# Static string map: design

Date: 2026-10-30
Status: draft for review

## Intent

A Rust crate for immutable lookup tables whose contents are written in source and fixed at compile time. Think Guava's `ImmutableMap`, but every key is known when the code is built. A proc macro analyzes the exact key set at build time and generates the fastest lookup structure for those keys.

Why: Rust has `phf`, `quickphf` and `frozen-collections`, but none of them are fast on string keys. `phf` uses SipHash and CHD and runs at roughly ordinary hash table speed. `frozen-collections` is only about 1.2x faster than `HashMap` with `foldhash` on strings. Daniel Lemire's recent C++ work ([Billion-query-per-second lookup tables in C++](https://lemire.me/blog/2026/10/26/billion-query-per-second-lookup-tables-in-c/), library [ConstexprCore/perfect_hash](https://github.com/ConstexprCore/perfect_hash), MIT) reaches 1.2 to 2 ns per lookup on small and medium string sets, 7 to 13x faster than `std::unordered_map`. No Rust crate does this. The goal is to bring that class of performance to Rust, built the way Rust works, not as a port.

**Performance is the top priority.** When ease of use and lookup speed conflict, speed wins.

## Goals (version one)

1. String keys (`&str`), exact byte-for-byte match.
2. Sets of up to a few thousand keys. Small and medium sets (5 to 500 keys: keywords, HTTP methods and headers, URL schemes, ticker symbols) are the main target.
3. One macro form only (see "API"). Convenience sugar comes later.
4. Lookup speed measurably and clearly better than every Rust alternative on this machine, and close to Lemire's C++ library on the same key sets.
5. Honest, reproducible benchmarks that are published alongside the crate.

## Non-goals (version one; candidates for later, only if v1 numbers justify them)

- Sets as a separate type (a map to `()` is fine internally, but no `static_set!` yet).
- Integer, char, byte-slice or enum keys.
- Case-insensitive matching.
- Expression form (`let m = static_map! { ... }`) and any other syntax sugar.
- Inferring the value type.
- Runtime "freeze" (building an optimized map from data at startup).
- Very large maps (100k+ keys), for example Lemire's binary fuse `constmap` design.
- Being a general collection of macros. This crate lives in `pickmap/` inside the local-only `macroz` folder, which will later hold other macro ideas.

## Anti-goals (would make this a failure even if it works)

- **Not clearly faster.** If, after honest benchmarking, it doesn't beat `frozen-collections` and `phf` on string keys, the project has no reason to exist. Benchmarks decide; we don't massage them.
- **Unsound code.** No out-of-bounds reads, even "harmless" ones. Lemire's C++ library reads 16 bytes past short keys with a page-boundary guard. That is undefined behavior in Rust, and we won't do it.
- **Heavy builds.** A proc macro that noticeably slows the user's build (large dependency tree, multi-second expansion for normal-sized maps) hurts adoption. Expansion for a few thousand keys should take well under a second (my call), and the macro's dependencies should stay light.
- **Wrong answers.** A lookup of a key not in the map must return `None`, always. No "undefined value for missing keys" mode in v1.

## Constraints

- Stable Rust only. Developed on 1.99.1. Set the minimum supported version (MSRV) before publishing, and avoid very new features unless they pay off clearly.
- The runtime crate is `no_std` (no allocation, no std).
- Targets: x86_64 and aarch64 get tuned paths. Every other target must compile and be correct using a portable fallback.
- Architecture code is chosen at compile time (`cfg(target_feature)` / `cfg(target_arch)`). No runtime CPU detection. SSE2 (always on for x86_64) and NEON (always on for aarch64) are the baselines. AVX2 and AVX-512 are out of scope.
- `unsafe` is allowed only where it's needed (unaligned loads, intrinsics). Keep it small, comment every block with its safety argument, and run the portable path under Miri.
- Benchmark machine: this Linux box, an AMD Ryzen Threadripper PRO 9975WX (Zen 5). An Apple M4 Pro is available later for aarch64 numbers but isn't part of v1 work.

## API (decided)

One macro form, which declares a `static`:

```rust
use pickmap::static_map;

static_map! {
    pub METHODS: Method;
    "GET"    => Method::Get,
    "POST"   => Method::Post,
    "PUT"    => Method::Put,
    "DELETE" => Method::Delete,
}

let m: Option<&Method> = METHODS.get("POST");
assert!(!METHODS.contains_key("PATCH"));
for (k, v) in METHODS.iter() { /* declaration order */ }
assert_eq!(METHODS.len(), 4);
```

Rules:

- Header: optional visibility (`pub`, `pub(crate)`, ...), the static's name, `:`, the value type, `;`. The value type is required because a `static` needs a written type and a proc macro can't see types.
- Entries: `"string literal" => const expression`, comma separated, trailing comma allowed.
- The key type is `&str`, from the literals. A non-string key is a compile error with a clear message.
- Duplicate keys are a compile error that points at the second occurrence.
- Values may be any expression valid in a `static` initializer of the given type. The value type must therefore be `Sync`.
- An empty map is allowed.
- Iteration order is declaration order (my call, matching Guava).
- The macro generates a dedicated hidden type per map, so lookup code and tables can be specialized and inlined. The generated type implements a shared trait so generic code can accept any map:

```rust
pub trait StaticMap {
    type Value: 'static;
    fn get(&self, key: &str) -> Option<&Self::Value>;
    fn contains_key(&self, key: &str) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn iter(&self) -> impl Iterator<Item = (&'static str, &Self::Value)>; // exact signature: builder's call
}
```

Inherent methods with the same names exist on the generated type too, so callers don't need to import the trait.

Generic code uses the trait as a bound (`fn f(m: &impl StaticMap<Value = Method>)`). Trait objects (`&dyn StaticMap`) are not required in v1 (my call), so `iter` may return `impl Iterator` and the table layout stays free to choose by benchmark.

## How it works (decided at the level of ideas)

### Crate layout

Three crates in one Cargo workspace:

1. **Generator** (plain library, no proc-macro dependency). Takes a list of keys, analyzes them, picks a strategy, and produces a description of tables and parameters, then emits Rust tokens. Testable as ordinary code. Later it can be reused from `build.rs` and as the basis for runtime freeze.
2. **Proc macro** (thin). Parses the macro input, calls the generator, and reports errors with good spans.
3. **Runtime** (`no_std`, tiny). The `StaticMap` trait, the shared compare and load helpers (including the arch-specific code), and a re-export of the macro. This is the crate users depend on.

### Strategy selection at build time

The generator checks strategies in order of expected speed and uses the first one that works for the key set. The order and thresholds below are starting points; benchmarks set the final values.

1. **Tiny set:** a few keys (threshold set by benchmark) are compared directly. A length check rules out most misses for free.
2. **Length only:** if every key has a different length, the length alone is the slot.
3. **Picked bytes (Lemire/gperf style):** `slot = (len + asso[key[p1]] + asso[key[p2]] ...) & mask`, with one to three byte positions and a 256-entry association table. Positions may count from the start or from the end of the key, since many key sets differ only near the end. Unlike Lemire's C++, which is limited by `constexpr` to a greedy search, the generator runs native code and can **backtrack**: try different position sets, retry `asso` assignments, and grow the table size, within a time budget.
4. **Fallback hash:** for key sets where picked bytes don't work, a fast multiply-based hash over 8-byte chunks of the key, with a seed found by search so the result is collision-free (perfect).

Table size is a power of two so the reduction is a mask. Empty slots are allowed (non-minimal), which keeps search fast and helps reject misses. Keep memory reasonable: no huge sparse tables for small sets (builder sets the limit).

### Lookup

1. Compute the slot from the strategy above. This must never read outside the input key: positions past the end of a short key need a safe answer (clamp, mask, or limit positions to the shortest key length; builder's call).
2. Check the stored key at that slot: compare length first, then bytes.
3. On a match, return the value at the same index.

The byte compare is where most speed comes from:

- Stored keys are kept zero-padded in fixed-width slots (for example 16 bytes) for keys up to that width.
- The input is loaded **without reading past its end**: overlapping unaligned loads by length class (for example two possibly overlapping 8-byte loads for lengths 8 to 16, two 4-byte loads for 4 to 7), or a copy into a padded buffer. Pick whatever benchmarks fastest and safe.
- Then one compare (SSE2 or NEON for 16 bytes, or two `u64` compares). Benchmark whether SIMD actually beats scalar `u64` compares here; use it only if it does.
- Keys longer than the fixed width use an ordinary slice compare after the hash and length checks.

## Testing

- Unit tests on the generator for every strategy, including key sets chosen to force each one.
- Property tests: random key sets of many sizes and shapes (shared prefixes, shared suffixes, all same length, very long keys, empty string, non-ASCII UTF-8). Every key must be found with the right value, and random non-keys must return `None`. Compare against `std::collections::HashMap` as the reference.
- Compile-fail tests (for example with `trybuild`) for duplicate keys, non-string keys and a malformed header.
- Miri on the portable path.
- Run the test suite on the portable fallback too, not just SSE2.

## Benchmarks

On this box (Zen 5), single thread pinned to one core. Record CPU frequency or boost settings with every result.

Competitors:
- `phf`
- `quickphf`
- `frozen-collections` (its string map macro)
- `std::collections::HashMap` with `foldhash`
- a plain Rust `match` on the string
- Lemire's `ConstexprCore/perfect_hash` (C++23), built and called from the benchmark through `cc` as an outside reference. If the FFI call adds overhead that changes the comparison, time it in a C++ harness on the same inputs instead, and say which method was used.

Key sets (use real data where possible):
- HTTP methods
- common HTTP header names
- URL schemes
- Rust keywords
- JavaScript reserved words
- S&P 100 tickers
- random words at 1,000 and 3,000 keys
- a set built to be hard: long shared prefixes and same lengths

Workloads per key set: all hits, all misses, and a 50/50 mix. Lookups in shuffled order so branch prediction can't memorize them. Report ns per lookup and include build-time (macro expansion) numbers.

Measurement (my call): the main number is throughput, independent lookups back to back in shuffled order, the same method as Lemire's published benchmarks. Also report latency (each lookup's input depends on the previous result) as a secondary number. Every competitor, including the C++ reference, runs on identical inputs with identical methodology.

The C++ reference is Lemire's library in its default, published configuration (the one his blog post benchmarks), even though it uses a 16-byte over-read that we forbid. That makes the comparison against the best known number, and the gap shows what the safety rule costs.

**Success targets (my call on the numbers):** on this box, faster than every Rust competitor on every small and medium string key set, with a large margin (2x or more) over `phf` and `frozen-collections` on typical keyword sets, and within about 1.5x of Lemire's C++ library. These are targets to report against, not release gates. A tie with plain `match` on tiny sets (a handful of keys) is acceptable, since there the best strategy may compile to about the same code, and is reported as a tie. Results go in a `BENCHMARKS.md` with the commands to reproduce them.

## Phases

**Phase 0: performance spike, with a go/no-go decision (decided).** Before building the macro or generator, hand-write the lookup (picked-byte hash plus safe padded compare) for 3 to 4 of the benchmark key sets, for example HTTP methods, header names, Rust keywords, and 1,000 random words. Tables may be computed by a throwaway script or by hand. Benchmark these against `phf`, `frozen-collections`, `match`, `HashMap` with `foldhash`, and Lemire's C++ library, using the methodology above.

- **Go:** clearly faster than `phf` and `frozen-collections` on these sets. Continue to phase 1, and carry over what the spike learned (best compare method, whether SIMD helps, layout).
- **No-go:** not clearly faster. Stop and rethink with the user before writing more code.

The spike code is throwaway. The benchmark harness may be kept and grown.

**Phase 1: the crate.** Generator, proc macro, runtime, tests, and the full benchmark suite as described above.

## Credit

The README credits Daniel Lemire's post and `ConstexprCore/perfect_hash` (MIT) for the picked-byte hash and padded-compare ideas, and the .NET `FrozenDictionary` / `frozen-collections` approach for choosing a strategy per dataset.

## Left to the builder

- Final crate names before publishing. Working names: `pickmap` (runtime, which users depend on), `pickmap-macros`, and `pickmap-gen`. `pickmap` was free on crates.io on 2026-10-30.
- Benchmark framework (`criterion` or `divan`).
- Exact thresholds, position-search limits, time budgets, table size limits, and the fixed key width.
- Choice of proc-macro parsing approach, as long as dependencies stay light.
- Exact `iter` signature and whether the trait uses an associated iterator type.
- How tables are laid out in generated code (struct of arrays vs array of structs), decided by benchmark.
- Whether SIMD is used for the compare at all, decided by benchmark.
