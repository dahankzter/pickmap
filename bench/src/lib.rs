//! Benchmark harness: competitors, batches and timing loops.

use keysets::{KeySet, SplitMix64, Workload, queries};

pub mod competitors {
    include!(concat!(env!("OUT_DIR"), "/competitors.rs"));
}

pub const MISS: u32 = u32::MAX;

/// One workload's queries, kept both as `&str` and as raw parts for C++.
pub struct Batch {
    pub strs: Vec<&'static str>,
    pub ptrs: Vec<*const u8>,
    pub lens: Vec<usize>,
    /// The same queries back to back in one buffer, one space apart, as a lexer or
    /// parser would see them in its input; `ranges[i]` is `strs[i]` inside `buf`.
    pub buf: Vec<u8>,
    pub ranges: Vec<std::ops::Range<usize>>,
}

pub fn batch(set: &KeySet, w: Workload) -> Batch {
    Batch::from_strs(queries(set, w).into_iter().map(|q| &*q.leak()).collect())
}

impl Batch {
    pub fn from_strs(strs: Vec<&'static str>) -> Batch {
        let mut buf = Vec::new();
        let mut ranges = Vec::with_capacity(strs.len());
        for s in &strs {
            ranges.push(buf.len()..buf.len() + s.len());
            buf.extend_from_slice(s.as_bytes());
            buf.push(b' ');
        }
        Batch {
            ptrs: strs.iter().map(|s| s.as_ptr()).collect(),
            lens: strs.iter().map(|s| s.len()).collect(),
            strs,
            buf,
            ranges,
        }
    }

    /// A fresh random permutation of the same queries (Fisher-Yates).
    pub fn shuffled(&self, rng: &mut SplitMix64) -> Batch {
        let mut strs = self.strs.clone();
        for i in (1..strs.len()).rev() {
            strs.swap(i, rng.below(i + 1));
        }
        Batch::from_strs(strs)
    }
}

pub struct Case {
    pub set: &'static str,
    pub competitor: &'static str,
    pub tput: fn(&Batch) -> u64,
}

/// Independent lookups back to back. Generic over the lookup so it is inlined.
#[inline(never)]
pub fn tput<F: Fn(&str) -> Option<u32>>(b: &Batch, f: F) -> u64 {
    b.strs.iter().map(|q| f(q).unwrap_or(MISS) as u64).sum()
}

/// [`tput`] for lookups of the queries inside `b.buf` (`get_in`).
#[inline(never)]
pub fn tput_in<F: Fn(&[u8], std::ops::Range<usize>) -> Option<u32>>(b: &Batch, f: F) -> u64 {
    b.ranges.iter().map(|r| f(&b.buf, r.clone()).unwrap_or(MISS) as u64).sum()
}


macro_rules! case {
    ($out:ident, $set:ident, $name:literal, $get:path) => {
        $out.push(Case {
            set: stringify!($set),
            competitor: $name,
            tput: |b| tput(b, $get),
        });
    };
}

macro_rules! lemire {
    ($out:ident, $set:ident) => {{
        mod $set {
            unsafe extern "C" {
                #[link_name = concat!("lemire_", stringify!($set), "_tput")]
                pub fn tput(p: *const *const u8, l: *const usize, n: usize) -> u64;
            }
        }
        $out.push(Case {
            set: stringify!($set),
            competitor: "lemire-cpp",
            // SAFETY: `ptrs[i]`/`lens[i]` describe the live, leaked `&'static str` `strs[i]`,
            // and the C++ side reads exactly `n` entries of each array.
            tput: |b| unsafe { $set::tput(b.ptrs.as_ptr(), b.lens.as_ptr(), b.strs.len()) },
        });
    }};
}

macro_rules! set_cases {
    ($out:ident, $set:ident) => {
        case!($out, $set, "pickmap", competitors::$set::pick_get);
        case!($out, $set, "pickmap-keyword-scan", competitors::$set::pick_kw_get);
        $out.push(Case {
            set: stringify!($set),
            competitor: "pickmap-get-in",
            tput: |b| tput_in(b, competitors::$set::pick_in_get),
        });
        case!($out, $set, "quickphf", competitors::$set::quickphf_get);
        case!($out, $set, "match", competitors::$set::match_get);
        case!($out, $set, "phf", competitors::$set::phf_get);
        case!($out, $set, "frozen-collections", competitors::$set::fz_get);
        case!($out, $set, "hashmap-foldhash", competitors::$set::fold_get);
        std::sync::LazyLock::force(&competitors::$set::FOLD);
        lemire!($out, $set);
    };
}

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    set_cases!(out, http_methods);
    set_cases!(out, http_headers);
    set_cases!(out, rust_keywords);
    set_cases!(out, random1000);
    set_cases!(out, url_schemes);
    set_cases!(out, js_reserved);
    set_cases!(out, tickers100);
    set_cases!(out, random3000);
    set_cases!(out, hard_prefix);
    set_cases!(out, long_paths);
    out
}
