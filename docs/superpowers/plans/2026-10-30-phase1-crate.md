# pickmap Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `pickmap` crate: a `static_map!` proc macro that generates a branch-free, perfect-hash string map per key set, with tests, a branch-miss guard, and benchmarks against the spike's numbers.

**Architecture:** There are four library crates:
- `pickmap-core` (`no_std`): the load, hash and slot primitives.
- `pickmap-gen` (plain library): parsing, representation choice, perfect-hash search, and token emission. It calls `pickmap-core` at build time, so build-time and run-time math are the same code.
- `pickmap-macros` (proc-macro): a thin wrapper around `pickmap-gen`.
- `pickmap` (`no_std` facade): the `StaticMap` trait, `Iter`, the macro re-export, and the core under `__private`.

`keysets` and `bench` come over from `spike/round3`, with `bench` switched from the spike to the real macro.

**Tech Stack:** Rust 1.99.1 stable, edition 2024. `proc-macro2` 1, `quote` 1, `litrs` 1 (no `syn`). `trybuild` 1 for compile-fail tests. The bench uses `phf`, `quickphf`, `frozen-collections`, `foldhash`, `perf-event2`, `cc`, and Lemire's vendored `perfect_hash.h`.

**Spec:** `docs/superpowers/specs/2026-10-30-static-map-design.md`. The reference implementation for every primitive is the spike on branch `spike/round3`: `spike/src/hash.rs`, `spike/src/lib.rs`, `spike/gen/search.rs`, `spike/build.rs`. Read it with `git show spike/round3:<path>`.

## Global Constraints

- Stable Rust only.
- `pickmap-core` and `pickmap` are `#![no_std]`, with no allocation.
- The only `unsafe` in the project's own code (vendored C++ aside) is the optimization barrier `opaque`: an empty, non-`pure` `asm!` on x86_64 and aarch64, compiled out under `cfg(miri)` and on other targets. It needs a `// SAFETY:` comment. There are no raw-pointer loads and no unchecked indexing.
- No input byte is ever read outside the input slice. Short inputs read from the static `ZEROS` buffer instead.
- Everything in `get` before the final hit/miss branch is free of data-dependent branches.
- Version 1 has one lookup method, `get`, ending in an ordinary branch. No API names describe branching.
- No SIMD, no over-reading loads, and no runtime CPU detection.
- `pickmap-gen` and `pickmap-macros` depend only on `proc-macro2`, `quote`, `litrs`, and `pickmap-core`.
- Macro syntax: `static_map! { [vis] NAME: ValueType; "key" => value_expr, ... }` with an optional trailing comma. String-literal keys only. A duplicate key is a compile error at the second occurrence.
- Values are expressions valid in a `static` initializer of `ValueType`, and `ValueType: Sync`. `get` returns `Option<&'static ValueType>`.
- `iter()` yields entries in declaration order.
- Benchmark machine: AMD Ryzen Threadripper PRO 9975WX, run with `taskset -c 4`. The harness is the spike's: a fresh permutation for each timed call, interleaved competitors, branch misses per lookup, and per-query agreement.
- The spike baseline for the regression target is `pickmap-spike` hit/miss ns/lookup:

  | set | hit | miss |
  |---|---|---|
  | http_methods | 1.75 | 1.77 |
  | http_headers | 3.44 | 3.46 |
  | rust_keywords | 2.36 | 2.33 |
  | random1000 | 2.01 | 1.66 |

  It is reported against, and is not a gate.

## Review Focus

1. **Value expressions containing top-level-looking commas:** turbofish generics (`Foo::<A, B>::NEW`) or closures. Each entry must stay one value. Pinned by `parse_turbofish_value` (Task 6).
2. **Escaped and raw string keys** (`"a\"b"`, `r#"x"#`, `"\u{e9}"`) must match the runtime string they denote. Pinned by `parse_escaped_keys` (Task 6) and `escaped_keys_lookup` (Task 7).
3. **Name clashes:**
   - Two maps in one module.
   - User items named like generated internals (`SLOTS`, `VALUES`, `Slot`).
   - A user `type Option = ();` or `type Some = ...` in scope.

   All must compile and work. Pinned by `two_maps_and_shadowing_names` (Task 7).
4. **Keys over 32 bytes** that share their first and last 16 bytes and their length must stay distinct, and so must 40-byte inputs differing in a middle byte. Pinned by `long_keys_share_ends` (Task 5) and `long_keys_lookup` (Task 7).
5. **Inputs no key resembles:**
   - the empty string (a map without `""`);
   - a 1-byte input;
   - a 100-byte input on a map with only short keys;
   - non-ASCII input.

   All must return `None` without panicking. Pinned by `odd_inputs_miss` (Task 7) and the Miri run (Task 10).

---

### Task 1: Workspace, keysets carry-over, new key sets

**Files:**
- Create: `Cargo.toml`. A workspace with members `keysets`, `pickmap-core`, `pickmap-gen`, `pickmap-macros`, `pickmap`, `bench`; members are added as each task creates them. `resolver = "3"`, `[workspace.package] edition = "2024"`. `[profile.release] codegen-units = 1, lto = "fat", debug = 1`.
- Create: `.gitignore` (`/target`, `/.superpowers`).
- Create: `keysets/`, taken from `git show spike/round3:keysets/...` (Cargo.toml, src/lib.rs, data/*.txt, tests/keysets.rs).
- Create: `keysets/data/url_schemes.txt`, `keysets/data/js_reserved.txt`, `keysets/data/tickers100.txt`.
- Modify: `keysets/src/lib.rs`, `keysets/tests/keysets.rs`.

**Interfaces:**
- Produces (on top of everything the spike's `keysets` exports: `KeySet`, `Workload`, `WORKLOADS`, `QUERY_COUNT`, `queries`, `SplitMix64`):
  - `pub fn url_schemes() -> KeySet` (`"url_schemes"`)
  - `pub fn js_reserved() -> KeySet` (`"js_reserved"`)
  - `pub fn tickers100() -> KeySet` (`"tickers100"`)
  - `pub fn random_words_n(n: usize, seed: u64, name: &'static str) -> KeySet`, with `random_words()` becoming `random_words_n(1000, 42, "random1000")`
  - `pub fn random3000() -> KeySet`, which is `random_words_n(3000, 43, "random3000")`
  - `pub fn hard_prefix() -> KeySet` (`"hard_prefix"`): 500 keys `format!("/api/v1/items/{i:05}/detail")`, all 25 bytes long with a long shared prefix.
  - `pub fn all() -> Vec<KeySet>` in the order http_methods, http_headers, rust_keywords, random1000, url_schemes, js_reserved, tickers100, random3000, hard_prefix.

Data, one key per line:
- `url_schemes.txt`: `http https ws wss ftp file data mailto blob javascript about tel`
- `js_reserved.txt`: `break case catch class const continue debugger default delete do else enum export extends false finally for function if implements import in instanceof interface let new null package private protected public return static super switch this throw true try typeof var void while with yield await`
- `tickers100.txt`: `AAPL ABBV ABT ACN ADBE AIG AMD AMGN AMT AMZN AVGO AXP BA BAC BK BKNG BLK BMY BRK.B C CAT CHTR CL CMCSA COF COP COST CRM CSCO CVS CVX DE DHR DIS DUK EMR F FDX GD GE GILD GM GOOG GOOGL GS HD HON IBM INTC INTU ISRG JNJ JPM KO LIN LLY LMT LOW MA MCD MDLZ MDT META MMM MO MRK MS MSFT NEE NFLX NKE NOW NVDA ORCL PEP PFE PG PLTR PM PYPL QCOM RTX SBUX SCHW SO T TGT TMO TMUS TSLA TXN UNH UNP UPS USB V VZ WFC WMT XOM`

- [ ] **Step 1: Copy the spike's keysets and update the `sizes` test** in `keysets/tests/keysets.rs` to `assert_eq!(lens, [9, 56, 53, 1000, 12, 46, 100, 3000, 500])`. Add a test `hard_prefix_shape`: every key has `len() == 25` and starts with `"/api/v1/items/"`.
- [ ] **Step 2: Run** `cargo test -p keysets`. Expected: FAIL (the new functions are missing).
- [ ] **Step 3: Implement** the data files and functions above.
- [ ] **Step 4: Run** `cargo test -p keysets`. Expected: PASS, 7 tests.
- [ ] **Step 5: Commit** `git commit -m "keysets: carry over from spike, add url, js, tickers, random3000, hard_prefix sets"`

---

### Task 2: `pickmap-core` load and hash primitives

**Files:**
- Create: `pickmap-core/Cargo.toml` (no dependencies; dev-dep `keysets` for its RNG)
- Create: `pickmap-core/src/lib.rs` (`#![no_std]`, `pub mod load; pub mod hash; pub mod slot;`, with `slot` empty until Task 3)
- Create: `pickmap-core/src/load.rs`, `pickmap-core/src/hash.rs`
- Test: `pickmap-core/tests/load.rs`, `pickmap-core/tests/hash.rs`

**Interfaces:**
- Produces, ported from `spike/round3:spike/src/hash.rs`. Everything is `#[inline(always)]` unless noted.
  - `load`:
    - `pub const SMALL: u8 = 1; pub const MID: u8 = 2; pub const BIG: u8 = 4; pub const HUGE: u8 = 8;`
    - `pub fn opaque(x: u64) -> u64`: the non-`pure` empty `asm!`, `cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri)))`, and a plain identity otherwise.
    - `pub fn key_value<const WORDS: usize, const CLASSES: u8>(k: &[u8]) -> [u64; WORDS]`
    - `pub fn key_value_with<const WORDS: usize>(k: &[u8], classes: u8) -> [u64; WORDS]`, which the const form calls.
    - `pub fn signature<const W: usize, const C: usize, const WORDS: usize>(k: &[u8]) -> [u64; WORDS]`
    - `pub fn signature_with<const WORDS: usize>(k: &[u8], w: usize, c: usize) -> [u64; WORDS]`
    - `pub fn classes_for(lens: impl Iterator<Item = usize>) -> (u8, usize)`, returning (classes, load count). Not inline.
  - `hash`:
    - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum Pos { Start(u8), End(u8) }`
    - `pub fn pick_byte(k: &[u8], pos: Pos) -> u8`
    - `pub fn picked_slot(k: &[u8], positions: &[Pos], asso: &[[u16; 256]], mask: u32) -> usize`
    - `pub fn h_value<const WORDS: usize>(v: &[u64; WORDS], len: usize, seed: u64) -> u64`
    - `pub fn h_long(k: &[u8], seed: u64) -> u64`: the spike's `h64`, over every byte.
    - `pub const PILOT_MUL: u64 = 0x9E37_79B9_7F4A_7C15;`
    - `pub fn pthash_bucket(h: u64, bucket_bits: u32) -> usize`
    - `pub fn pthash_slot(h: u64, bucket_bits: u32, pilots: &[u32], mask: u32) -> usize`

Bodies are the spike's, including:
- the `ZEROS` source selection via `core::hint::select_unpredictable`;
- the opaque-mask blend for `HUGE`;
- the `WORDS == 1` paths.

- [ ] **Step 1: Write the failing tests.**
  - `tests/load.rs`, ported from `spike/round3:spike/tests/hash.rs`:
    - `key_value_is_exact` (all WORDS/CLASSES cases from the spike);
    - `signature_is_exact` (sig 2x4 in 1 and 2 words, 4x3, 2x14, 8x3);
    - `with_forms_match_const_forms` (for lengths 0..=40 and every classes mask 1..=15, `key_value_with::<4>(k, c) == ` the const form for c in {7, 15}, and `signature_with` matches `signature::<2, 4, 1>` and `signature::<4, 3, 2>`);
    - `miri_all_lengths` (exact-length `Box<[u8]>` for 0..=40 through every function);
    - `classes_for_counts` (`classes_for([2, 5, 9, 20].into_iter()) == (15, 11)`, `classes_for([4, 12].into_iter()) == (MID | BIG, 4)`).
  - `tests/hash.rs`:
    - `pick_byte_clamps` (from the spike);
    - `h_long_sensitive` (every single-bit flip changes `h_long` for random keys of lengths 0..=80);
    - `pthash_bucket_zero_bits` (`pthash_bucket(u64::MAX, 0) == 0` and `pthash_bucket(u64::MAX, 3) == 7`).
- [ ] **Step 2: Run** `cargo test -p pickmap-core`. Expected: FAIL (the modules are empty).
- [ ] **Step 3: Implement** `load.rs` and `hash.rs` by porting the spike.
- [ ] **Step 4: Run** `cargo test -p pickmap-core`, then `cargo +nightly miri test -p pickmap-core`. Expected: both PASS with no UB reports.
- [ ] **Step 5: Commit** `git commit -m "core: branch-free key values and hash primitives"`

---

### Task 3: Slots, `Iter`, `StaticMap`, facade crate

**Files:**
- Modify: `pickmap-core/src/slot.rs`
- Create: `pickmap/Cargo.toml` (deps: `pickmap-core`, plus `pickmap-macros` from Task 7, so leave the re-export out until then), `pickmap/src/lib.rs`, `pickmap/src/iter.rs`
- Test: `pickmap-core/tests/slot.rs`, `pickmap/tests/iter.rs`

**Interfaces:**
- Consumes: `load::opaque` (Task 2).
- Produces:
  - `pickmap_core::slot`:
    - `#[repr(C)] pub struct Slot<const WORDS: usize> { pub key: [u64; WORDS], pub len: u32, pub idx: u32 }`
    - `impl<const WORDS: usize> Slot<WORDS>`:
      - `pub const EMPTY: Self` (with `len: u32::MAX`)
      - `#[inline(always)] pub fn find(&self, v: &[u64; WORDS], n: usize) -> Option<usize>`: one combined `(len ^ n) | (key_i ^ v_i)…` compare, then `if diff == 0 { Some(idx) } else { None }`.
      - `#[inline(always)] pub fn find_long(&self, v: &[u64; WORDS], k: &[u8], keys: &[&str]) -> Option<usize>`: for `k.len() > 32`, compares `len` and `keys[idx].as_bytes() == k`; otherwise `find(v, k.len())`.
  - `pickmap`:
    - `pub trait StaticMap: Sync + 'static`, with:
      - `type Value: 'static;`
      - `fn get(&self, key: &str) -> Option<&'static Self::Value>;`
      - `fn contains_key(&self, key: &str) -> bool { self.get(key).is_some() }`
      - `fn len(&self) -> usize;`
      - `fn is_empty(&self) -> bool { self.len() == 0 }`
      - `fn iter(&self) -> Iter<Self::Value>;`
    - `pub struct Iter<V: 'static>`, implementing `Iterator<Item = (&'static str, &'static V)>`, `ExactSizeIterator`, `DoubleEndedIterator` and `Clone`, with `#[doc(hidden)] pub const fn new(keys: &'static [&'static str], values: &'static [V]) -> Self`.
    - `#[doc(hidden)] pub mod __private { pub use pickmap_core::{load::*, hash::*, slot::*}; pub use crate::iter::Iter; }`

- [ ] **Step 1: Write the failing tests.**
  - `pickmap-core/tests/slot.rs`:
    - `find_matches_only_equal`: a slot with `key = [1, 2]`, `len = 9`, `idx = 4` gives `Some(4)` for `([1, 2], 9)`, and `None` for `([1, 2], 8)`, `([1, 3], 9)` and `([0, 2], 9)`.
    - `empty_never_matches`: `Slot::<1>::EMPTY.find(&[0], n)` is `None` for n in 0..=64.
    - `find_long_compares_bytes`: with `keys = ["x".repeat(40), "y".repeat(40)]` leaked, a slot with `idx = 1, len = 40` matches only `"y"*40`.
  - `pickmap/tests/iter.rs`:
    - `iter_in_order_and_exact_size`: `Iter::new(&["a", "b"], &[1, 2])` yields `[("a", &1), ("b", &2)]`, `len()` is 2, and `.rev()` gives b then a.
- [ ] **Step 2: Run** `cargo test -p pickmap-core --test slot && cargo test -p pickmap --test iter`. Expected: FAIL.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "core, pickmap: slots, iterator and StaticMap trait"`

---

### Task 4: Generator, representation choice

**Files:**
- Create: `pickmap-gen/Cargo.toml` (deps: `pickmap-core`, `proc-macro2 = "1"`, `quote = "1"`, `litrs = "1"`; dev-dep `keysets`)
- Create: `pickmap-gen/src/lib.rs`, `pickmap-gen/src/repr.rs`
- Test: unit tests in `repr.rs`

**Interfaces:**
- Consumes: `classes_for`, `key_value_with`, `signature_with` (Task 2).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Repr { Classes { words: usize, classes: u8 }, Sig { w: usize, c: usize, words: usize } }`
  - `impl Repr { pub fn words(self) -> usize; pub fn value(self, k: &[u8]) -> Vec<u64> }`. `value` dispatches on `words` (1, 2, 4) to the `_with` functions.
  - `pub fn repr_for(keys: &[&[u8]]) -> Repr`: the spike's `repr_for` over the keys of 32 bytes or fewer. With none of those (or no keys at all), `Classes { words: 1, classes: 0 }`. `words` for `Sig` is the fewest of 1, 2 or 4 holding `c * w` bytes; for `Classes` it holds the longest key of 32 bytes or fewer.

- [ ] **Step 1: Write the failing tests** in `repr.rs`:
  - `repr_matches_spike`:
    - `http_methods` → `Sig { w: 2, c: 4, words: 1 }`
    - `rust_keywords` → `Sig { w: 2, c: 4, words: 1 }`
    - `random1000` → `Sig { w: 4, c: 3, words: 2 }`
    - `http_headers` → `Classes { words: 4, classes: 15 }`
  - `long_keys_ignored_for_repr`: `["ab", <40 x 'z'>]` → `Sig { w: 2, c: 1, words: 1 }`.
  - `empty_and_empty_string`: `[]` and `[""]` → `Classes { words: 1, classes: 0 }`.
  - `values_are_exact_per_set`: for every `keysets::all()` set, `repr.value` is distinct across keys of equal length.
- [ ] **Step 2: Run** `cargo test -p pickmap-gen repr`. Expected: FAIL.
- [ ] **Step 3: Implement** `repr.rs`.
- [ ] **Step 4: Run** `cargo test -p pickmap-gen repr`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "gen: per-set key representation choice"`

---

### Task 5: Generator, perfect-hash search and simulated plan

**Files:**
- Create: `pickmap-gen/src/search.rs`, `pickmap-gen/src/plan.rs`
- Test: unit tests in both files

**Interfaces:**
- Consumes: `Repr` (Task 4); `pick_byte`, `picked_slot`, `h_value`, `h_long`, `pthash_bucket`, `pthash_slot`, `PILOT_MUL` (Task 2).
- Produces:
  - `#[derive(Clone, Debug)] pub enum Strategy { Picked { positions: Vec<Pos>, asso: Vec<[u16; 256]>, mask: u32 }, PtHash { seed: u64, bucket_bits: u32, pilots: Vec<u32>, mask: u32 } }`
  - `pub fn key_hash(repr: Repr, k: &[u8], seed: u64) -> u64`: `h_long` for keys over 32 bytes, otherwise `h_value` over `repr.value(k)`.
  - `pub fn search(keys: &[&[u8]], repr: Repr) -> Result<Strategy, String>`: picked first, then PtHash.
  - `pub fn slot_of(s: &Strategy, repr: Repr, k: &[u8]) -> usize`
  - `pub fn table_len(s: &Strategy) -> usize`
  - `pub struct Plan { pub repr: Repr, pub strategy: Strategy, pub slots: Vec<Option<u32>>, pub has_long: bool }`, where `slots[i]` is the key index in slot `i`.
  - `pub fn plan(keys: &[&str]) -> Result<Plan, String>`
  - `impl Plan { pub fn lookup(&self, keys: &[&str], key: &str) -> Option<usize> }`: a simulation using the same core functions, in the same order, as the emitted `get`.

Algorithms: port `spike/round3:spike/gen/search.rs`, with these values and changes:
- Picked: positions `Start(0..8)` and `End(0..8)`; k = 0..=3; a feasibility filter on distinct `(len, picked bytes)`; table sizes `base, 2·base, 4·base, 8·base` with `base = next_power_of_two(n)`; 200,000 steps per attempt.
- Picked is skipped when `8·base > 65_536`, because `asso` values are `u16`.
- PtHash: `bucket_bits = log2(next_power_of_two(max(1, n/2)))`; `m = next_power_of_two(n)`, then `2m`; seeds `0..64`; pilots `0..2^20` stored as `(p * PILOT_MUL) as u32`.
- **No allocation inside the pilot loop.** Keep a reusable slot buffer and check duplicates within a bucket by linear scan, since buckets average 2 keys. This keeps unoptimized expansion fast.
- Empty key set: `Picked { positions: [], asso: [], mask: 0 }` with a 1-slot table.
- Error messages name the failing set size and say "no perfect hash found".

- [ ] **Step 1: Write the failing tests.**
  - `search.rs`:
    - `perfect_for_every_keyset`: every slot is distinct and `< table_len` for each `keysets::all()` set.
    - `small_sets_use_picked`: methods, headers and keywords give `Picked`.
    - `random1000_uses_pthash_1024`: `PtHash` with `table_len == 1024`.
    - `long_keys_share_ends`: 300 keys of 40 bytes each, `"A"*16 + format!("{i:08}") + "Z"*16`, give a perfect strategy.
  - `plan.rs`:
    - `plan_lookup_matches_hashmap`: for every keyset, every key returns its index, and every query of every `WORKLOADS` workload matches a `HashMap`.
    - `property_random_sets`: SplitMix64 seeds 1..=40 build sets of random sizes in {0, 1, 2, 3, 5, 17, 64, 300, 2000} and random shapes, chosen per seed:
      - (a) lowercase lengths 1..=12;
      - (b) a shared 10-byte prefix plus 3 random bytes;
      - (c) all the same length, 6;
      - (d) mixed lengths 0..=60, including `""`;
      - (e) non-ASCII: random `char`s from `'à'..='ž'`.

      For each, `plan` succeeds; every key is found; 2,000 random non-keys of lengths 0..=70 return `None`; every key with one byte changed misses unless it equals another key.
    - `empty_set_plan`: `plan(&[])`'s `lookup` of `""` and `"a"` is `None`.
- [ ] **Step 2: Run** `cargo test -p pickmap-gen --release search plan`. Expected: FAIL.
- [ ] **Step 3: Implement** `search.rs` and `plan.rs`.
- [ ] **Step 4: Run** `cargo test -p pickmap-gen --release`. Expected: PASS. Also run `cargo test -p pickmap-gen` (debug) and note its time in the ledger.
- [ ] **Step 5: Commit** `git commit -m "gen: picked-byte and PtHash search with simulated plan"`

---

### Task 6: Generator, macro input parsing

**Files:**
- Create: `pickmap-gen/src/parse.rs`
- Test: unit tests in `parse.rs`, built with `quote!` and `str::parse::<TokenStream>()`

**Interfaces:**
- Produces:
  - `pub struct MapInput { pub vis: TokenStream, pub name: proc_macro2::Ident, pub value_ty: TokenStream, pub entries: Vec<Entry> }`
  - `pub struct Entry { pub key: String, pub key_span: Span, pub value: TokenStream }`
  - `pub struct Error { pub span: Span, pub message: String }` with `impl Error { pub fn to_compile_error(&self) -> TokenStream }`, which emits `::core::compile_error!(...)` at `span`.
  - `pub fn parse(input: TokenStream) -> Result<MapInput, Error>`

Grammar:
- `vis` is empty, `pub`, or `pub` followed by a parenthesized group.
- Then `IDENT`, `:`, the type tokens up to the first top-level `;`, then `;`.
- Then zero or more entries: a string literal, `=>`, and value tokens up to the next top-level `,`, which is optional after the last entry.
- A comma counts as top-level unless it sits inside a turbofish (`::` immediately followed by `<`, tracked to its matching `>`) or inside a group. Groups are already token trees.

Keys are decoded with `litrs::StringLit` (normal and raw strings). Byte strings and other literals are errors.

Error messages:
- missing name: `"expected a map name, like `pub NAME: Type;`"`
- missing `:` or `;`: `"expected `:`"` / `"expected `;` after the value type"`
- non-string key: `"keys must be string literals"`
- missing `=>`: `"expected `=>` after the key"`
- empty value: `"expected a value expression"`
- duplicate key: `"duplicate key \"{key}\""`, at the second key's span

- [ ] **Step 1: Write the failing tests:**
  - `parse_basic`: `pub(crate) M: u8; "a" => 1, "b" => 2,` gives the vis tokens `pub(crate)`, name `M`, type `u8`, and two entries.
  - `parse_turbofish_value`: `M: V; "a" => Foo::<A, B>::NEW, "b" => [1, 2]` gives 2 entries, the first value being `Foo :: < A , B > :: NEW`.
  - `parse_escaped_keys`: `"a\"b"`, `r#"x"y"#` and `"\u{e9}"` decode to `a"b`, `x"y` and `é`.
  - `parse_errors`: each error message above comes out for a minimal input that triggers it.
  - `parse_empty_map`: `M: u8;` gives 0 entries.
- [ ] **Step 2: Run** `cargo test -p pickmap-gen parse`. Expected: FAIL.
- [ ] **Step 3: Implement** `parse.rs`.
- [ ] **Step 4: Run** `cargo test -p pickmap-gen parse`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "gen: macro input parser"`

---

### Task 7: Emitter, proc macro, end-to-end tests

**Files:**
- Create: `pickmap-gen/src/emit.rs`, `pickmap-macros/Cargo.toml` (`proc-macro = true`; dep `pickmap-gen`), `pickmap-macros/src/lib.rs`
- Modify: `pickmap-gen/src/lib.rs` (`pub fn expand(input: TokenStream) -> TokenStream`), `pickmap/Cargo.toml` and `pickmap/src/lib.rs` (`pub use pickmap_macros::static_map;`)
- Test: `pickmap/tests/macro.rs`, `pickmap/tests/ui.rs`, `pickmap/tests/ui/*.rs` and `*.stderr`

**Interfaces:**
- Consumes: `parse` (Task 6), `plan` and `Plan` (Task 5), `Repr` (Task 4), `pickmap::__private` (Task 3).
- Produces: `pickmap::static_map!`, and `pickmap_gen::expand`, which parses, plans and emits, turning every error into `to_compile_error`.

The expansion for `pub METHODS: Method; "GET" => Method::Get, ...` is fixed in shape. Every runtime path is absolute (`::pickmap::__private::...`, `::core::option::Option::{Some, None}`):

```rust
#[doc(hidden)]
#[allow(non_camel_case_types)]
pub struct __Pickmap_METHODS;
pub static METHODS: __Pickmap_METHODS = __Pickmap_METHODS;
const _: () = {
    const N: usize = 2;
    static KEYS: [&str; N] = ["GET", "POST"];
    static VALUES: [Method; N] = [Method::Get, Method::Post];
    static SLOTS: [::pickmap::__private::Slot<WORDS>; TABLE] = [/* Slot { key, len, idx } or Slot::EMPTY */];
    // Picked: POSITIONS const array and ASSO static. PtHash: PILOTS static plus SEED/BITS/MASK consts.
    impl __Pickmap_METHODS {
        #[inline]
        pub fn get(&self, key: &str) -> ::core::option::Option<&'static Method> { /* see below */ }
        #[inline] pub fn contains_key(&self, key: &str) -> bool { self.get(key).is_some() }
        #[inline] pub const fn len(&self) -> usize { N }
        #[inline] pub const fn is_empty(&self) -> bool { N == 0 }
        #[inline] pub fn iter(&self) -> ::pickmap::__private::Iter<Method> { ::pickmap::__private::Iter::new(&KEYS, &VALUES) }
    }
    impl ::pickmap::StaticMap for __Pickmap_METHODS { type Value = Method; /* each method delegates */ }
};
```

`get` body:
1. `let k = key.as_bytes();`
2. `let v = ` either `key_value::<WORDS, CLASSES>(k)` or `signature::<W, C, WORDS>(k)`, per `plan.repr`.
3. Slot computation:
   - `Picked`: `picked_slot(k, &POSITIONS, &ASSO, MASK)`
   - `PtHash`: `pthash_slot(h, BITS, &PILOTS, MASK)`, where `h` is `h_value(&v, k.len(), SEED)`. In sets with long keys only, `h` is `if k.len() > 32 { h_long(k, SEED) } else { h_value(..) }`.
4. `match SLOTS[slot].find(&v, k.len())`, or `.find_long(&v, k, &KEYS)` when `plan.has_long`, then `{ Some(i) => Some(&VALUES[i]), None => None }`.

For an empty map, `get` is `None` with no tables. Every generated value is exactly what `Plan::lookup` computes. The emitter's unit test `emit_mentions_only_absolute_paths` asserts that the token string contains no bare `Option`, `Some` or `None`.

- [ ] **Step 1: Write the failing tests.** In `pickmap/tests/macro.rs`, with maps declared at module level:
  - `methods_lookup`: an enum `Method` map; all 9 HTTP methods; `PATCHX`, `get` and `""` miss; `len() == 9`; `iter()` in declaration order.
  - `values_any_static_type`: values of type `&'static str`, `(u8, &'static str)` and a struct with a `const fn new`.
  - `escaped_keys_lookup`: keys `"a\"b"`, `r#"x"y"#` and `"\u{e9}"` are found as `"a\"b"`, `"x\"y"` and `"é"`.
  - `long_keys_lookup`:
    - keys of 33, 40 and 64 bytes that share their first and last 16 bytes are each found;
    - a 40-byte input differing only at byte 20 misses;
    - a 2-byte key in the same map works.
  - `empty_map_and_empty_key`: `static_map! { E: u8; }` has `len() == 0` and `get("") == None`; a map `{ "" => 1, "a" => 2 }` finds both.
  - `odd_inputs_miss`: for the methods map, `""`, `"G"`, `"x".repeat(100)`, `"GÉT"` and `"日本語"` return `None`.
  - `two_maps_and_shadowing_names`: in one module, two maps `A` and `B`, user items `static SLOTS: u8 = 0; static VALUES: u8 = 0; struct Slot;` and `type Option = ();`. Both maps compile and work.
  - `trait_generic_use`: `fn count<M: StaticMap>(m: &M) -> usize { m.iter().count() }` works on both maps.
  - `visibility`: a `pub` map in a submodule is usable from the parent.

  In `pickmap/tests/ui.rs`, use `trybuild::TestCases::new().compile_fail("tests/ui/*.rs")`, with `ui/duplicate_key.rs`, `ui/non_string_key.rs`, `ui/missing_type.rs` and `ui/wrong_value_type.rs` (`M: u8; "a" => "x"`). Generate the `.stderr` files with `TRYBUILD=overwrite`, then review them: each must point at the offending token.
- [ ] **Step 2: Run** `cargo test -p pickmap`. Expected: FAIL (no `static_map`).
- [ ] **Step 3: Implement** `emit.rs`, `expand`, `pickmap-macros`, and the re-export.
- [ ] **Step 4: Run** `cargo test -p pickmap && cargo test -p pickmap-gen`. Expected: PASS, with `.stderr` files reviewed and committed.
- [ ] **Step 5: Commit** `git commit -m "macro: static_map! emitter and end-to-end tests"`

---

### Task 8: Bench carry-over on the real macro

**Files:**
- Create: `bench/` from `spike/round3` (`Cargo.toml`, `build.rs`, `src/lib.rs`, `src/bin/report.rs`, `tests/agree.rs`, `cpp/vendor/perfect_hash/*`)
- Modify: `bench/Cargo.toml` (drop `spike`; add `pickmap`, `quickphf = "0.1"`, and build-dep `quickphf_codegen = "0.1"`), `bench/build.rs`, `bench/src/lib.rs`, `bench/src/bin/report.rs`

**Interfaces:**
- Consumes: `pickmap::static_map!` (Task 7) and every `keysets::all()` set (Task 1).
- Produces:
  - For each set module in `competitors.rs`:
    - `pickmap::static_map! { pub PICK: u32; ... }` with `pub fn pick_get(k: &str) -> Option<u32> { PICK.get(k).copied() }`;
    - `pub fn quickphf_get(k: &str) -> Option<u32>`, generated with `quickphf_codegen`. Check docs.rs for the 0.1 API; the map is `quickphf::PhfMap<&'static str, u32>`.
  - The competitor name list becomes `["pickmap", "lemire-cpp", "quickphf", "match", "phf", "frozen-collections", "hashmap-foldhash"]`.
  - `set_cases!` covers all 9 sets.
  - `report.rs`:
    - adds the `SPIKE_BASELINE` table from Global Constraints and prints `pickmap / spike` for hit and miss on the 4 spike sets, flagging ratios over 1.10;
    - prints the verdict columns `pickmap`, `min(phf, frozen, quickphf)`, `speedup`, `pickmap / lemire` for hit, miss and mixed.
  - If a Lemire set fails to compile, it is skipped with `cargo:warning`, as in the spike plan.

- [ ] **Step 1: Copy the bench and adapt** `tests/agree.rs`, so that `all_competitors_agree_per_query` expects at least 7 competitors per set across all 9 sets.
- [ ] **Step 2: Run** `cargo test -p bench --release --test agree`. Expected: FAIL (bench still names `spike`).
- [ ] **Step 3: Implement** the changes above.
- [ ] **Step 4: Run** `cargo test -p bench --release`. Expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "bench: carry over harness, measure the real macro, add quickphf and new sets"`

---

### Task 9: Branch-miss guard

**Files:**
- Create: `bench/tests/branch_misses.rs`
- Modify: `pickmap-core` code only where the guard proves a data-dependent branch.

**Interfaces:**
- Consumes: `bench::{batch, cases, Batch}`, `perf-event2`.

The test `pickmap_has_no_data_dependent_branches` is `#[cfg_attr(debug_assertions, ignore = "release only")]`. For every `keysets::all()` set and the `Hit` and `Miss` workloads, it:
1. takes the `"pickmap"` case;
2. warms it on one shuffled batch (`Batch::shuffled`, `SplitMix64(11)`);
3. counts `Hardware::BRANCH_MISSES` over one `tput` call on a second shuffled batch.

It asserts misses per lookup ≤ 0.03. If the counter can't be built, it prints `"perf counters unavailable; skipping"` and returns.

If it fails, the cause is LLVM re-creating a branch (spec, "Table layout and lookup"). Find it with `objdump -d -C --no-show-raw-insn target/release/deps/branch_misses-*`, under the symbol `bench::tput::<bench::competitors::<set>::pick_get>`. Then fix it in `pickmap-core` using the spec's remedies: `opaque` on the mask or value LLVM reasons about, an arithmetic blend instead of a select, or one combined compare. Re-run every test after each fix.

- [ ] **Step 1: Write the test.**
- [ ] **Step 2: Run** `cargo test -p bench --release --test branch_misses -- --nocapture`. Expected: PASS, or FAIL naming the set and workload. The spike's `http_headers` miss path (0.10) is the likely failure.
- [ ] **Step 3: Fix** each failure as described, recording each fix as a ledger ruling.
- [ ] **Step 4: Run** `cargo test --workspace --release`. Expected: all PASS, including `branch_misses`.
- [ ] **Step 5: Commit** `git commit -m "core: keep lookups free of data-dependent branches; add branch-miss guard"`

---

### Task 10: Expansion time, Miri, report, docs

**Files:**
- Create: `pickmap-gen/tests/expansion_time.rs`, `README.md`, `BENCHMARKS.md`

**Interfaces:**
- Consumes: `pickmap_gen::expand` and `plan`, `keysets::random3000`.

- [ ] **Step 1: Write the test** `plan_3000_keys_time` in `expansion_time.rs`. It times `plan()` on `random3000` and on `hard_prefix`, and prints both. It asserts under 1 s only when `cfg!(not(debug_assertions))`. In debug, it prints the time with a note, because the spec target is "under 1 s unoptimized if reachable".
- [ ] **Step 2: Run** `cargo test -p pickmap-gen --test expansion_time -- --nocapture` (debug) and `--release`. Expected: release PASS. Record the debug time. If it's over 1 s, the README gets the spec's `[profile.dev.build-override] opt-level = 3` recommendation, with the measured numbers.
- [ ] **Step 3: Run Miri** with `cargo +nightly miri test -p pickmap-core -p pickmap`. Expected: PASS with no UB reports. `trybuild` UI tests may be skipped under Miri with `#[cfg_attr(miri, ignore)]`.
- [ ] **Step 4: Run the report pinned, twice:** `cargo build -p bench --release --bin report && taskset -c 4 target/release/report > BENCHMARKS.md`, then a second run to a scratch file. Compare the verdicts; differences over 10% get noted in `BENCHMARKS.md`. Then add a top section, "Summary", with the hardware, the commands, the spike-baseline comparison, and the gaps that remain, as in the spec's later list.
- [ ] **Step 5: Write `README.md`.** It covers:
  - the syntax example from the spec, and what `get`, `iter`, `len` and `StaticMap` do;
  - performance in one paragraph that links `BENCHMARKS.md`;
  - limits: string keys only, `get` only, and an expansion-time note if needed;
  - safety: no over-reads; the single empty `asm!` barrier and why it exists;
  - credit, as in the spec's Credit section.
- [ ] **Step 6: Commit** `git commit -m "docs: README, benchmarks, expansion-time test"`
