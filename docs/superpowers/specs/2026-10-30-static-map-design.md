# Static string map: design

Date: 2026-10-30
Status: approved; updated 2026-10-30 with phase 0 spike results (see `docs/spike-results.md` on branch `spike/round3`)

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
- More than one lookup method. Version 1 ships `get` only. The spike's branch-free variant, faster on unpredictable streams when the caller doesn't branch, is a later investigation, and no API names that describe branching are added before then.
- Over-reading loads (Lemire's 16-byte read past the key, via inline asm), even as an opt-in feature.
- AVX-512 masked loads or any other path that needs target-feature dispatch.
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
- No SIMD and no runtime CPU detection. The only architecture-specific code is the optimization barrier (an empty `asm!` on x86_64 and aarch64). Other targets get a plain function, and they lose only the branch-free guarantee.
- The only `unsafe` is that barrier: no raw-pointer loads, no unchecked indexing. Every `unsafe` block gets a `// SAFETY:` comment. The whole test suite also runs under Miri, with the barrier compiled out.
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

- Attributes may precede the header (added after phase 1):
  - `///` docs and ordinary attributes apply to the static.
  - `#[cfg]` and `#[cfg_attr]` apply to every generated item.
  - `#[pickmap(workload = "general" | "keyword_scan" | "mostly_hits")]` is a performance hint that never changes results. `keyword_scan` and `mostly_hits` make picked-byte maps use an early-exit key compare: 3-23% faster when misses look nothing like the keys, up to about 3x slower on misses that resemble keys. PtHash maps ignore it, since their tagged compare is already as fast.
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

## How it works (decided; validated by the phase 0 spike)

The spike (`spike/round3`, `docs/spike-results.md` there) measured every choice below. Numbers in this section come from it.

### Crate layout

Four crates in one Cargo workspace, plus the kept benchmark crates:

1. **Core** (`pickmap-core`, `no_std`). The load, hash and slot primitives. Both the generator (at build time) and generated code (at run time) call these, so the two can't drift apart.
2. **Generator** (`pickmap-gen`, plain library, no proc-macro dependency). Parses the input, picks a key representation and a hash strategy, builds the tables, and emits Rust tokens. Testable as ordinary code. Later it can be reused from `build.rs` and as the basis for runtime freeze.
3. **Proc macro** (`pickmap-macros`, thin). Calls the generator and reports errors with good spans.
4. **Runtime facade** (`pickmap`, `no_std`, tiny). The `StaticMap` trait, the iterator, a re-export of the macro, and the core under a hidden `__private` module. This is the crate users depend on.

Core is separate from the facade because the generator needs the primitives and the facade re-exports the macro; one crate holding both would be a dependency cycle.
4. **`keysets` and `bench`**: carried over from the spike, with the spike-only crate dropped. `bench` gains a `pickmap` competitor that uses the real macro.

### Key representation (chosen per set at build time)

Every key, and every input, is turned into a fixed-size value of 1, 2 or 4 `u64` words (8, 16 or 32 bytes): the fewest words that hold the set's longest key. Two keys of the same length are equal exactly when their values are, so a lookup compares one value instead of bytes. Computing it must be **branch-free and never read outside the input**. The spike's way, which the builder keeps unless a benchmark beats it:

- **Length classes.** 1-3 bytes use the first, middle and last byte. 4-7 bytes use two overlapping 4-byte words. 8-16 bytes use two overlapping 8-byte words. 17-32 bytes use four words covering the first and last 16 bytes. Each class reads from the input when the input is long enough for it, and from a static zero buffer otherwise, so every load is in bounds. The class is then picked with branch-free selects. Only the classes the set's key lengths need are emitted.
- **Word signature.** `C` words of `W` bytes at offsets `min(i*W, len-W)`, where `W` is the largest power of two that fits in the shortest key. Inputs shorter than `W` read the zero buffer instead.
- The generator emits whichever needs fewer loads. In the spike, the signature won for methods, keywords and 1,000 random words, and classes won for headers (keys of 2-28 bytes).
- Inputs of a length no key has still get a value; the length check rejects them.
- **Keys longer than 32 bytes** are supported through a slice compare against the stored key. Only sets that contain such keys pay for that path.

### Hash strategy (chosen per set at build time)

1. **Picked bytes (Lemire/gperf style):** `slot = (len + asso_1[key[p1]] + ... ) & mask`, with 0 to 3 byte positions counted from the start or the end (clamped onto the key, branch-free; 0 positions is "length only"). There is **one 256-entry association table per position**, so byte order matters. A depth-first search with backtracking assigns the table values, trying table sizes from `next_power_of_two(n)` up to 8x that. It wins for small and medium sets (the spike's methods, headers and keywords).
2. **PtHash fallback** for sets where picked bytes fail within budget (the spike's 1,000 random words):
   - one folded 64x64-to-128-bit multiply over the key value and length;
   - buckets taken from the top hash bits, with a power-of-two count of about `n/2`;
   - per-bucket pilots stored pre-mixed (`(pilot * C) as u32`), so the slot is `(h as u32 ^ pilot) & mask`;
   - the table is `next_power_of_two(n)` slots (load factor up to about 1), doubled only if the pilot search fails.

   That keeps 1,000 keys in about 1,024 slots, which fit in L1.

Empty slots carry a length no input can have.

### Table layout and lookup

- Each slot holds the key value (`[u64; WORDS]`), the key length (`u32`) and the value's index (`u32`). Values live in a `static VALUES: [V; N]` in declaration order, which also serves `iter`.
- `get(key)`:
  1. compute the key value;
  2. compute the slot;
  3. compare the key:
     - **picked-byte maps:** length and every key word in **one** combined comparison, `(len ^ n) | (key_0 ^ v_0) | ...` equals 0;
     - **PtHash maps:** first length plus an 8-bit tag of the key's hash. A near-miss has a different hash, so this branch is in effect hit-or-miss. Then the key words, which only likely hits reach. A tag on picked-byte maps would need an extra multiply, and measured slower;
  4. on a match, return `Some(&VALUES[index])`.

  Every branch before the answer must be predictable on any input stream, including near-misses. The tag check qualifies because it is effectively the hit/miss branch itself.
- **LLVM re-creates branches** unless prevented. The spike found four ways it does this, all despite `core::hint::select_unpredictable`:
  - it turns a select that involves a memory load into a branch;
  - it branches around work a mask made irrelevant;
  - it splits `(len == n) & (key == v)` into a short-circuit;
  - it sinks a `pure` asm block.

  What held is an **optimization barrier**: an empty, non-`pure` `asm!` block that takes and returns a register value, touches no memory and emits no instructions. This is the constant-time-code technique. It is allowed in the runtime crate, with a `// SAFETY:` comment, and compiled out under Miri.
- A **branch-miss guard** protects this. It runs every benchmark key set through a lookup loop on shuffled hit-only and miss-only streams, reads hardware branch-miss counters, and fails above 0.03 misses per lookup. Maps with keys over 32 bytes are measured but exempt: their length-dependent hashing and byte compare mispredict on mixed-length streams (about 0.3-0.5 per lookup on `long_paths`). Without hardware counters the guard fails, unless `PICKMAP_NO_PERF_COUNTERS=1` asks it to skip. Measuring behavior is sturdier than counting jumps in disassembly, which also contains harmless loop and bounds-check branches.
- SIMD is not used. Every load lands in general registers, and the spike found no need for it.

## Testing

- Unit tests on the generator for every strategy, including key sets chosen to force each one.
- Property tests: random key sets of many sizes and shapes (shared prefixes, shared suffixes, all same length, very long keys, empty string, non-ASCII UTF-8). Every key must be found with the right value, and random non-keys must return `None`. Compare against `std::collections::HashMap` as the reference.
- Compile-fail tests (for example with `trybuild`) for duplicate keys, non-string keys and a malformed header.
- Miri on the runtime's load and lookup code, over exact-length allocations of every length 0..=40.
- The branch-miss guard (see "Table layout and lookup").
- Tests for every representation: key values are exact (any byte change alters them) for every length a class or signature covers.

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

Workloads per key set: all hits, all misses, and a 50/50 mix.

Measurement (decided by the spike, after the first harness was found invalid):
- **Throughput only.** Independent lookups back to back.
- **No memorized sequences.** Every timed call sees a fresh random permutation of the queries, and its warm-up call uses a different one, so the branch predictor can't memorize the sequence. The spike's first harness replayed one sequence, and Zen 5 learned it.
- **Interleaved competitors** within each sample, with the median over samples.
- **Branch misses per lookup** are reported next to the time, read from hardware counters.
- **Per-query agreement:** every competitor gives the reference answer on every query.
- Latency mode is dropped, because branchy lookups break the data dependency it relies on.

The C++ reference is Lemire's library in its default, published configuration (the one his blog post benchmarks), even though it uses a 16-byte over-read that we forbid. That makes the comparison against the best known number, and the gap shows what the safety rule costs.

**Success targets (my call on the numbers):**
- **No regression from the spike:** on the spike's four key sets, `pickmap`'s hit-only and miss-only numbers are no more than 10% slower than the spike's `pickmap-spike` numbers in `docs/spike-results.md`.
- **Margin:** 2x or more over the better of `phf` and `frozen-collections` on mixed workloads, for small and medium sets.
- **Lemire:** within about 2x of his library on hit-only and miss-only workloads. The spike reached 1.15-2.1x, and the gap is the cost of the safety rule.

These are targets to report against, not release gates. Results go in a `BENCHMARKS.md` with the commands to reproduce them.

## Phases

**Phase 0: performance spike, with a go/no-go decision (decided).** Before building the macro or generator, hand-write the lookup (picked-byte hash plus safe padded compare) for 3 to 4 of the benchmark key sets, for example HTTP methods, header names, Rust keywords, and 1,000 random words. Tables may be computed by a throwaway script or by hand. Benchmark these against `phf`, `frozen-collections`, `match`, `HashMap` with `foldhash`, and Lemire's C++ library, using the methodology above.

- **Go:** clearly faster than `phf` and `frozen-collections` on these sets. Continue to phase 1, and carry over what the spike learned (best compare method, layout).
- **No-go:** not clearly faster. Stop and rethink with the user before writing more code.

The spike code is throwaway. The benchmark harness may be kept and grown.

**Phase 0 outcome: GO** (three rounds; see `docs/spike-results.md` on `spike/round3`).

**Phase 1: the crate.** Generator, proc macro, runtime, tests, codegen test, and the full benchmark suite as described above. It starts from `master`, carries over `keysets/` and `bench/` from the spike, and drops the spike crate.

**Later, driven by benchmarks:**
- a cheaper representation for sets with a wide key-length range (headers);
- the branch-free lookup variant;
- aarch64 numbers.

## Credit

The README credits Daniel Lemire's post and `ConstexprCore/perfect_hash` (MIT) for the picked-byte hash and padded-compare ideas, and the .NET `FrozenDictionary` / `frozen-collections` approach for choosing a strategy per dataset.

## Left to the builder

- Final crate names before publishing. Working names: `pickmap` (runtime, which users depend on), `pickmap-macros`, and `pickmap-gen`. `pickmap` was free on crates.io on 2026-10-30.
- Benchmark framework: the spike's custom harness is the default (see Benchmarks).
- Exact search budgets, position candidates and table size limits. The spike's values are the starting point: positions `Start(0..8)` and `End(0..8)`, 200,000 search steps per attempt, tables up to 8x `next_power_of_two(n)`, and for PtHash up to 64 seeds and 2^20 pilots.
- **Expansion speed:** proc macros build unoptimized by default (`build-override` has `opt-level = 0`). The search must stay fast unoptimized: under 1 s for 3,000 keys is the target. If that's not reachable, the README recommends `[profile.dev.build-override] opt-level = 3`, and the expansion-time numbers say so.
- Choice of proc-macro parsing approach, as long as dependencies stay light.
- Exact `iter` signature and whether the trait uses an associated iterator type.
- How tables are laid out in generated code, for example whether `len` moves out of the slot, decided by benchmark.
