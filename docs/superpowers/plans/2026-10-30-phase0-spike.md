# pickmap Phase 0 Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Find out with honest measurements whether a picked-byte perfect hash with a safe padded compare is clearly faster than `phf` and `frozen-collections` for static string maps, and produce a go/no-go report.

**Architecture:** A Cargo workspace with three crates. `keysets` (kept) holds the benchmark key sets and query generation. `spike` (throwaway) uses a `build.rs` to search for a perfect hash per key set and emit specialized lookup code. `bench` (kept) generates the competitor maps (`match`, `phf`, `frozen-collections`, `foldhash` `HashMap`, and Lemire's C++ through `cc`), checks that they all agree, and runs a custom timing harness that prints a markdown report with a verdict.

**Tech Stack:** Rust 1.99.1 stable. `phf` + `phf_codegen`, `frozen-collections`, `foldhash`, and `cc` (latest on crates.io). clang++ 23 with C++23. ConstexprCore/perfect_hash, vendored at commit `81947016ab00330b94e16806e5f9acdaacaf96d4`.

**Spec:** `docs/superpowers/specs/2026-10-30-static-map-design.md` (Phase 0 section, plus Benchmarks)

## Global Constraints

- Stable Rust only, no nightly features.
- The spike uses no out-of-bounds reads, ever. Reading past the end of the input key is forbidden even if it is "harmless".
- The spike's lookup code is safe Rust (slice `get`, `from_le_bytes` on in-bounds subslices). `unsafe` is added only if a measurement shows that bounds checks cost something, and then with a `// SAFETY:` comment.
- A missing key always returns `None`.
- Key sets: HTTP methods, HTTP header names, Rust keywords, 1,000 random words.
- Competitors: `phf`, `frozen-collections`, plain `match`, `std::collections::HashMap` with `foldhash`, Lemire's `ConstexprCore/perfect_hash`.
- Workloads: all hits, all misses, 50/50 mix, in shuffled order.
- Throughput is the headline number. Latency is secondary.
- Benchmark machine: AMD Ryzen Threadripper PRO 9975WX, pinned to one core, with governor and boost recorded.
- Values are `u32`: the key's index in declaration order. The miss sentinel used in the timing loops is `u32::MAX`.
- The C++ side is compiled with clang++ `-std=c++23 -O3` and no `-march`. The Rust side uses the default target with no `target-cpu`. Both sides get the same baseline ISA (SSE2).

## Review Focus

1. **Benchmark loops optimized away or hoisted:** the reported ns/lookup becomes impossibly low. Each case's checksum must equal the `HashMap` reference, and any result under 0.3 ns/lookup is flagged in the report (Task 6).
2. **Empty and 1-byte inputs** reaching picked-byte positions and 8-byte loads must return `None` without reading out of bounds. Pinned by the Miri test over lengths 0..=40 (Task 2) and by `""` and 1-byte strings in the miss queries (Task 1).
3. **A miss with the same length and same picked bytes as a key** reaches the stored-key compare and must still return `None`. Pinned by the single-byte mutation test (Task 4).
4. **Keys longer than 16 bytes that share their first 16 bytes** (`access-control-allow-origin` / `-headers` / `-methods`) go through the long-key path and must not match each other. Pinned by including all three in the header set (Task 1) and by the mutation test (Task 4).
5. **Non-ASCII UTF-8 queries** must return `None` and must not panic on char boundaries. Lookups work on bytes, never on `char` indices. Pinned by the non-ASCII test (Task 4).

---

### Task 1: Workspace and `keysets` crate

**Files:**
- Create: `Cargo.toml` (workspace with members `keysets`, `spike`, `bench`; `[profile.release]` with `codegen-units = 1`, `lto = "fat"`, `debug = 1`)
- Create: `.gitignore` (`/target`)
- Create: `keysets/Cargo.toml`, `keysets/src/lib.rs`
- Create: `keysets/data/http_methods.txt`, `keysets/data/http_headers.txt`, `keysets/data/rust_keywords.txt` (one key per line, embedded with `include_str!`)
- Test: `keysets/tests/keysets.rs`

**Interfaces:**
- Produces:
  - `pub struct KeySet { pub name: &'static str, pub keys: Vec<String> }`. The value of `keys[i]` is `i as u32`.
  - `pub fn http_methods() -> KeySet` (name `"http_methods"`), `pub fn http_headers() -> KeySet` (`"http_headers"`), `pub fn rust_keywords() -> KeySet` (`"rust_keywords"`), `pub fn random_words() -> KeySet` (`"random1000"`).
  - `pub fn all() -> Vec<KeySet>` in the order above.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Workload { Hit, Miss, Mixed }`, plus `pub const WORKLOADS: [Workload; 3]`.
  - `pub const QUERY_COUNT: usize = 8192;` (a power of two; the latency loop relies on this).
  - `pub fn queries(set: &KeySet, w: Workload) -> Vec<String>`, which is deterministic and returns `QUERY_COUNT` entries.
  - `pub struct SplitMix64(u64)` with `fn next(&mut self) -> u64`. This is the only RNG, so no dependency is needed.

Data:
- `http_methods.txt`: `GET HEAD POST PUT DELETE CONNECT OPTIONS TRACE PATCH`
- `http_headers.txt`: `accept accept-charset accept-encoding accept-language accept-ranges access-control-allow-origin access-control-allow-headers access-control-allow-methods age allow authorization cache-control connection content-disposition content-encoding content-language content-length content-location content-range content-type cookie date etag expect expires from host if-match if-modified-since if-none-match if-range if-unmodified-since last-modified link location max-forwards origin pragma proxy-authenticate proxy-authorization range referer refresh retry-after server set-cookie strict-transport-security te trailer transfer-encoding upgrade user-agent vary via warning www-authenticate`
- `rust_keywords.txt`: `as break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while async await dyn abstract become box do final macro override priv typeof unsized virtual yield try gen union`
- `random_words()`: SplitMix64 with seed `42`. Lengths are uniform in `4..=12`, letters uniform in `a..=z`. Skip duplicates until there are 1000 keys.

Query generation (`queries`): SplitMix64 seeded from `0x5EED ^ fnv1a64(set.name) ^ (workload as u64)`, where `fnv1a64` is standard 64-bit FNV-1a.
- `Hit`: uniform random keys.
- `Miss`: produced by a miss generator. 75% are near-misses: take a random key and apply one of four edits, chosen uniformly: replace one random byte with a different byte from `a..=z` / `A..=Z` / `-`; append one byte; drop the last byte; swap two adjacent bytes. 25% are fully random: length uniform `0..=20`, bytes from `a..=z`. In addition, positions 0, 1 and 2 of every miss list are forced to `""`, `"x"` and `"é"`. Reject any candidate that is in the set.
- `Mixed`: each entry independently has a 50% chance of being a hit or a miss, from the same generators.

- [ ] **Step 1: Write the failing tests** in `keysets/tests/keysets.rs`
  - `sizes`: lengths are 9, 56, 53 and 1000. All keys within a set are distinct.
  - `queries_deterministic`: two calls give equal vectors, and the length is `QUERY_COUNT`.
  - `hit_queries_all_in_set`, `miss_queries_none_in_set`: checked for every set.
  - `miss_queries_include_edge_inputs`: `""`, `"x"` and `"é"` are in every `Miss` list.
  - `mixed_is_roughly_half`: the hit count is in `3600..=4600` for every set.
- [ ] **Step 2: Run** `cargo test -p keysets`. Expected: FAIL (the crate is empty).
- [ ] **Step 3: Implement** the interfaces and data above.
- [ ] **Step 4: Run** `cargo test -p keysets`. Expected: PASS. If `sizes` fails because of a data typo, fix the data, not the expected numbers.
- [ ] **Step 5: Commit** `git add -A && git commit -m "spike: workspace and benchmark key sets"`

---

### Task 2: Safe hash primitives in `spike`

**Files:**
- Create: `spike/Cargo.toml` (deps: none; build-deps: `keysets`; dev-deps: `keysets`), `spike/src/lib.rs`
- Create: `spike/src/hash.rs`. Plain functions with no crate-internal dependencies, so `build.rs` can include it with `#[path]`.
- Test: `spike/tests/hash.rs`

**Interfaces:**
- Produces (all `#[inline(always)]`, all safe code):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub enum Pos { Start(u8), End(u8) }`
  - `pub fn pick_byte(key: &[u8], pos: Pos) -> u8`. The index is clamped into the key: `Start(p)` maps to `min(p, len-1)` and `End(p)` maps to `len-1-min(p, len-1)`. An empty key gives `0`.
  - `pub fn picked_slot(key: &[u8], positions: &[Pos], asso: &[u16; 256], mask: u32) -> usize`, which computes `(len + Σ asso[pick_byte]) & mask` using wrapping `u32` arithmetic.
  - `pub fn load_padded16(key: &[u8]) -> u128`, valid for `len <= 16`. Equal to `u128::from_le_bytes` of the key copied into a zeroed `[u8; 16]`.
  - `pub fn h64(key: &[u8], seed: u64) -> u64`, used by the fallback hash. It must depend on every byte and on the length.

`load_padded16` approach: split by length class and use only in-bounds loads.
- 16: one 16-byte load.
- 8..=15: `lo` is the first 8 bytes; `hi` is bytes `len-8..len` as a `u64`, shifted right by `(16-len)*8`.
- 4..=7: the same trick with `u32` halves.
- 1..=3: `key[0] | key[len/2] << 8*(len/2) | key[len-1] << 8*(len-1)`.
- 0: `0`.

`h64` approach:
- Keys of 16 bytes or fewer: fold `lo`, `hi` from `load_padded16` and `len`.
- Longer keys: fold every 8-byte chunk, with the tail done as an overlapping load of the last 8 bytes.
- Mix with a 128-bit multiply fold: `((a ^ s1) as u128 * (b ^ s2) as u128)`, then XOR the high and low halves. The constants and seed are mixed in.

- [ ] **Step 1: Write the failing tests** in `spike/tests/hash.rs`
  - `load_padded16_matches_naive`: for every length 0..=16, and 1000 random byte strings per length (SplitMix64), the result equals the naive copy-into-zeroed-array version.
  - `pick_byte_clamps`: `pick_byte(b"abc", Start(5)) == b'c'`, `pick_byte(b"abc", End(0)) == b'c'`, `pick_byte(b"abc", End(9)) == b'a'`, `pick_byte(b"", Start(0)) == 0`.
  - `h64_sensitive`: for random keys of lengths 0..=40, flipping any single bit changes `h64`. Also, `"a"` and `"a\0"` hash differently.
  - `miri_all_lengths`: calls every function on keys of lengths 0..=40, built as exact-length `Vec<u8>` so Miri catches overreads. Mark it so it runs under `cargo miri test`; keep iteration counts small under `cfg(miri)`.
- [ ] **Step 2: Run** `cargo test -p spike --test hash`. Expected: FAIL (functions missing).
- [ ] **Step 3: Implement** `spike/src/hash.rs` and add `pub mod hash;` in `lib.rs`.
- [ ] **Step 4: Run** `cargo test -p spike --test hash`. Expected: PASS. Then run `cargo +nightly miri test -p spike --test hash` (nightly is used only as a tool here). Expected: PASS with no UB reports. If nightly or Miri is not installed, run `rustup toolchain install nightly --component miri` first.
- [ ] **Step 5: Commit** `git add -A && git commit -m "spike: safe hash primitives"`

---

### Task 3: Perfect hash search

**Files:**
- Create: `spike/gen/search.rs`. It is used by `spike/build.rs` through `#[path = "gen/search.rs"]` and by tests through `#[path]` in `spike/src/lib.rs` under `#[cfg(test)]` or a `pub mod search` (implementer's choice). It includes `hash.rs` via `#[path = "../src/hash.rs"]` or `super::hash`, so build time and run time share one implementation.
- Test: unit tests inside `search.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Pos`, `picked_slot`, `h64` from Task 2.
- Produces:
  - `pub enum Strategy { Picked { positions: Vec<Pos>, asso: [u16; 256], mask: u32 }, PtHash { seed: u64, buckets: u32, pilots: Vec<u16>, mask: u32 } }`
  - `pub fn search(keys: &[&[u8]]) -> Strategy`. It tries `Picked` first and falls back to `PtHash`. It panics only if both fail (acceptable for the spike).
  - `pub fn slot_of(s: &Strategy, key: &[u8]) -> usize`. This is the single definition of slot computation, and generated code must reproduce it exactly.
  - `pub fn table_len(s: &Strategy) -> usize` (= `mask + 1`).
  - PtHash slot definition: `b = ((h >> 32) * buckets as u64) >> 32`, then `slot = (h ^ (pilots[b] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u32 & mask`, where `h = h64(key, seed)`.

`Picked` search (the tests don't determine this algorithm):
1. Candidate positions are `Start(0..=7)` and `End(0..=7)`.
2. For `k` in 1..=3, take combinations of `k` distinct positions in a fixed order. Skip a combination unless the tuples `(len, picked bytes...)` are distinct across all keys, since this is necessary for success.
3. For each feasible combination, try table sizes `m = next_pow2(n)`, then ×2, up to `8 * next_pow2(n)`. Assign `asso` by depth-first search over the bytes that occur, ordered by how many keys use each byte (most first), trying values `0..m`. After each assignment, check only the keys whose bytes are now all assigned, and reject on any slot collision. Backtrack on failure. Each `(combination, m)` attempt has a budget of 200,000 assignment steps.
4. Return the first success, which has the fewest positions and the smallest table.

`PtHash` search: `mask = next_pow2(n * 5 / 4) - 1` and `buckets = max(1, n / 4)`. For seed `0, 1, 2, …` (up to 64): fail this seed if any two keys share an `h64`. Otherwise, fill buckets from largest to smallest, finding the first `pilot` in `0..=u16::MAX` that puts every key in the bucket into a free slot not used by earlier keys in the same bucket.

- [ ] **Step 1: Write the failing tests** in `search.rs`
  - `finds_perfect_for_every_keyset`: for each `keysets::all()` set, `search` succeeds, and `slot_of` is distinct across all keys.
  - `http_methods_is_picked`: the HTTP methods set returns `Strategy::Picked` with 2 or fewer positions.
  - `fallback_works`: a synthetic set of 500 keys `"key-prefix-common-{i:05}"`, which is hard for picked bytes from the start, succeeds through some strategy with distinct slots.
  - `pthash_direct`: calling the PtHash builder directly on `random1000` gives distinct slots and `table_len <= 2048`.
- [ ] **Step 2: Run** `cargo test -p spike search`. Expected: FAIL.
- [ ] **Step 3: Implement** the search as described.
- [ ] **Step 4: Run** `cargo test -p spike search --release`. Expected: PASS, with every test finishing in under 10 s total. If `Picked` fails on `random1000`, that is fine as long as the fallback succeeds.
- [ ] **Step 5: Commit** `git add -A && git commit -m "spike: perfect hash search (picked bytes + PtHash fallback)"`

---

### Task 4: Generated lookups in `spike`

**Files:**
- Create: `spike/build.rs`. For each `keysets::all()` set, it runs `search` and writes `$OUT_DIR/sets.rs`.
- Modify: `spike/src/lib.rs` (`pub mod sets { include!(concat!(env!("OUT_DIR"), "/sets.rs")); }`)
- Test: `spike/tests/lookup.rs`

**Interfaces:**
- Consumes: Task 2 `hash` functions (the generated code calls `crate::hash::*`), Task 3 `search` and `slot_of` (build time only).
- Produces: for each set, a module `spike::sets::<name>` (`http_methods`, `http_headers`, `rust_keywords`, `random1000`) with:
  - `pub fn get(key: &str) -> Option<u32>`: the padded-compare variant.
  - `pub fn get_memcmp(key: &str) -> Option<u32>`: the same slot, then a length check and `stored.as_bytes() == key.as_bytes()`.
  - `pub const STRATEGY: &str` (`"picked:k=<n>"` or `"pthash"`) and `pub const TABLE_LEN: usize`.

Generated layout (decided): a `static SLOTS: [Slot; TABLE_LEN]` with `#[repr(C, align(32))] struct Slot { key: u128, len: u32, val: u32, long: u32 }`.
- Empty slots have `len == u32::MAX`.
- For keys of 16 bytes or fewer, `key = load_padded16(key)` and `long = u32::MAX`.
- For longer keys, `key = 0` and `long` is an index into `static LONG: [&str; _]`.
- Positions, mask, seed and bucket count are emitted as `const` items so they are compile-time constants inside `get`. `asso` is a `static [u16; 256]` and `pilots` is a `static [u16; _]`.

`get` flow:
1. Compute the slot. Picked: `picked_slot`. PtHash: `h64` plus the pilot formula from Task 3.
2. Load `s = &SLOTS[slot]`.
3. If `s.len as usize != key.len()`, return `None`.
4. If `len <= 16`, compare `load_padded16(key) == s.key`. Otherwise compare the bytes with `LONG[s.long]`.
5. On a match, return `Some(s.val)`.

- [ ] **Step 1: Write the failing tests** in `spike/tests/lookup.rs`. Use one helper, `check(set, get_fn)`, run for every set and for both `get` and `get_memcmp`.
  - `every_key_found`: `get(keys[i]) == Some(i as u32)`.
  - `queries_match_hashmap`: for all 3 workloads, results equal those of a `HashMap<String, u32>` built from the set.
  - `single_byte_mutations_miss`: for every key and every byte index, replace the byte with each of `b'a'`, `b'Z'`, `b'-'`, `0x00`. Expect `None` unless the result equals some key, in which case expect that key's index.
  - `non_ascii_misses`: `"é"`, `"GÉT"`, `"content-typé"`, `"日本"` and `"accept\u{0}"` return `None`.
  - `long_shared_prefix`: in `http_headers`, `access-control-allow-origin`, `-headers` and `-methods` map to three distinct correct values, and `access-control-allow-` returns `None`.
- [ ] **Step 2: Run** `cargo test -p spike --test lookup`. Expected: FAIL (`sets` missing).
- [ ] **Step 3: Implement** `build.rs` and the generated code as above. Generated code must use only safe Rust.
- [ ] **Step 4: Run** `cargo test -p spike --test lookup`. Expected: PASS. Print each set's `STRATEGY` and `TABLE_LEN` with `cargo test -p spike --test lookup -- --nocapture` and note them for the report.
- [ ] **Step 5: Commit** `git add -A && git commit -m "spike: generated per-set lookups"`

---

### Task 5: Rust competitors and the agreement check

**Files:**
- Create: `bench/Cargo.toml` (deps: `keysets`, `spike`, `phf`, `frozen-collections`, `foldhash`; build-deps: `keysets`, `phf_codegen`)
- Create: `bench/build.rs`, which writes `$OUT_DIR/competitors.rs`
- Create: `bench/src/lib.rs`
- Test: `bench/tests/agree.rs`

**Interfaces:**
- Consumes: `keysets::*`, `spike::sets::<name>::{get, get_memcmp}`.
- Produces:
  - `pub const MISS: u32 = u32::MAX;`
  - `pub struct Batch { pub strs: Vec<&'static str>, pub ptrs: Vec<*const u8>, pub lens: Vec<usize> }` and `pub fn batch(set: &KeySet, w: Workload) -> Batch`. It leaks the query strings once; that's fine for a benchmark.
  - `pub struct Case { pub set: &'static str, pub competitor: &'static str, pub tput: fn(&Batch) -> u64, pub lat: fn(&Batch) -> u64 }`
  - `pub fn cases() -> Vec<Case>`. Competitor names are exactly `"pickmap-spike"`, `"pickmap-spike-memcmp"`, `"match"`, `"phf"`, `"frozen-collections"`, `"hashmap-foldhash"`, and in Task 6 `"lemire-cpp"`.
  - Loop bodies, written once as a `macro_rules!` so each lookup is inlined into its own loop (never through a `fn` pointer per lookup):
    - throughput: `b.strs.iter().map(|q| f(q).unwrap_or(MISS) as u64).sum()`
    - latency: `i` starts at 0, then `QUERY_COUNT` times: `r = f(b.strs[i]).unwrap_or(MISS)`, `sum += r as u64`, `i = (i + 1 + (r >> 31) as usize) & (QUERY_COUNT - 1)`
  - Competitor generation in `build.rs`, per set:
    - `match`: `fn(&str) -> Option<u32>` matching every key literal.
    - `phf`: through `phf_codegen::Map<&str>` with `u32` values.
    - `frozen-collections`: the `fz_string_map!` long form declaring a static, with the macro syntax taken from the current docs.rs for the version resolved.
    - `foldhash`: a `std::sync::LazyLock<HashMap<&'static str, u32, foldhash::fast::FixedState>>` built from the set and forced before timing.

- [ ] **Step 1: Write the failing test** `all_competitors_agree` in `bench/tests/agree.rs`. For each set and workload, every `Case` gives the same `tput` and the same `lat` checksum as the `"hashmap-foldhash"` case for that set.
- [ ] **Step 2: Run** `cargo test -p bench --release --test agree`. Expected: FAIL (`cases` missing).
- [ ] **Step 3: Implement** `build.rs` and `lib.rs`.
- [ ] **Step 4: Run** `cargo test -p bench --release --test agree`. Expected: PASS.
- [ ] **Step 5: Commit** `git add -A && git commit -m "bench: Rust competitors and agreement check"`

---

### Task 6: Lemire C++ reference

**Files:**
- Create: `bench/cpp/vendor/perfect_hash/`. Copy the headers and `LICENSE` from `https://github.com/ConstexprCore/perfect_hash` at commit `81947016ab00330b94e16806e5f9acdaacaf96d4`. Note the commit in `bench/cpp/vendor/perfect_hash/VENDORED.md`.
- Modify: `bench/build.rs` (generate `$OUT_DIR/lemire.cpp` and compile it with `cc::Build::new().cpp(true).compiler("clang++").std("c++23").opt_level(3)`; add the `cc` build-dep)
- Modify: `bench/src/lib.rs` (extern decls and `"lemire-cpp"` cases)
- Test: `bench/tests/agree.rs` (extended automatically through `cases()`)

**Interfaces:**
- Consumes: `Batch`, `MISS`, `QUERY_COUNT` from Task 5.
- Produces: for each set, `extern "C" uint64_t lemire_<set>_tput(const char* const* ptrs, const size_t* lens, size_t n)` and `lemire_<set>_lat(...)`. The loops are the same as Task 5's, written in C++, with `std::optional::value_or(UINT32_MAX)`. The whole loop runs in C++, so the FFI call is paid once per batch, not per lookup.
- Map construction follows the vendored README. Sets of 255 keys or fewer use `make_perfect_map<kv<"KEY", Nu>...>()`. `random1000` uses the wide factory (`make_wide_perfect_map<array, values>()`). Values are the declaration index.
- If the vendored library rejects a set at compile time, skip that set's `lemire-cpp` case, print a `cargo:warning`, and record "n/a (reason)" in the report.

- [ ] **Step 1: Run** `cargo test -p bench --release --test agree`. It must still pass before the change; this is the baseline.
- [ ] **Step 2: Implement** the vendoring, the generated C++ and the cases.
- [ ] **Step 3: Run** `cargo test -p bench --release --test agree`. Expected: PASS, with `lemire-cpp` included for every set it compiles for.
- [ ] **Step 4: Commit** `git add -A && git commit -m "bench: Lemire perfect_hash C++ reference"`

---

### Task 7: Timing harness, run, and go/no-go report

**Files:**
- Create: `bench/src/bin/report.rs`
- Create: `docs/spike-results.md` (the output of the run, committed)

**Interfaces:**
- Consumes: `cases()`, `batch()`, `WORKLOADS`, `keysets::all()`, and `spike::sets::<name>::{STRATEGY, TABLE_LEN}`.

Harness (decided; no framework):
- For each case, workload and mode (tput or lat): 5 warm-up calls, then 41 samples. Each sample times 16 back-to-back calls with `Instant`. The reported value is median sample time / (16 × `QUERY_COUNT`), in ns/lookup, to 2 decimals.
- Wrap inputs and outputs in `std::hint::black_box`.
- Assert that every checksum equals the reference before timing; abort on mismatch.

Report contents (markdown to stdout):
1. A header: CPU model (from `/proc/cpuinfo`), `scaling_governor` and `cpufreq/boost` for the pinned CPU, `rustc -V`, `clang++ --version` first line, and the date.
2. The strategy and table length for each set.
3. A throughput table: for each set, one table with competitors as rows and Hit / Miss / Mixed as columns.
4. A latency table in the same shape.
5. Flags: any value under 0.3 ns/lookup gets the note "suspiciously fast, check for hoisting".
6. A verdict. `best` is the faster of `pickmap-spike` and `pickmap-spike-memcmp`. `speedup = min(phf, frozen-collections) / best` on Mixed throughput, per set.
   - **GO** if speedup ≥ 1.25 on at least 3 of the 4 sets and ≥ 1.0 on all of them.
   - Otherwise **NO-GO**.
   - Also print `best / lemire-cpp` and `best / match` per set, for information.

- [ ] **Step 1: Implement** `report.rs`.
- [ ] **Step 2: Smoke run** `cargo run -p bench --release --bin report > /dev/null`. Expected: exits 0 within a few minutes.
- [ ] **Step 3: Real run, pinned:** `taskset -c 4 cargo run -p bench --release --bin report | tee docs/spike-results.md`. Expected: a complete report with a verdict line. Run it twice, and if Mixed throughput numbers differ by more than 10% between runs, note that in the report.
- [ ] **Step 4: Add a "Findings" section by hand** at the end of `docs/spike-results.md`. Cover which compare variant won, whether bounds checks showed up as a cost (look at `perf stat` / `perf annotate` on the winner if it is close to Lemire), which strategy each set used, and the size of the gap to Lemire. Also state that a SIMD compare was not tried in the spike: safe in-bounds loads put the input in general registers, so a scalar `u128` compare comes first, and SIMD is worth trying only if the profile shows the compare dominates.
- [ ] **Step 5: Commit** `git add -A && git commit -m "spike: benchmark report and go/no-go"`
- [ ] **Step 6: Stop.** Bring the verdict and findings to the user. Phase 1 gets its own plan after their decision.
