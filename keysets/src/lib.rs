//! Benchmark key sets and deterministic query generation.

use std::collections::HashSet;

/// A named key set. The value of `keys[i]` is `i as u32`.
pub struct KeySet {
    pub name: &'static str,
    pub keys: Vec<String>,
}

pub const QUERY_COUNT: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Workload {
    Hit,
    Miss,
    Mixed,
}

pub const WORKLOADS: [Workload; 3] = [Workload::Hit, Workload::Miss, Workload::Mixed];

pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next() as u128 * n as u128) >> 64) as usize
    }
}

fn from_lines(name: &'static str, data: &str) -> KeySet {
    KeySet { name, keys: data.lines().map(str::to_owned).collect() }
}

pub fn http_methods() -> KeySet {
    from_lines("http_methods", include_str!("../data/http_methods.txt"))
}

pub fn http_headers() -> KeySet {
    from_lines("http_headers", include_str!("../data/http_headers.txt"))
}

pub fn rust_keywords() -> KeySet {
    from_lines("rust_keywords", include_str!("../data/rust_keywords.txt"))
}

pub fn url_schemes() -> KeySet {
    from_lines("url_schemes", include_str!("../data/url_schemes.txt"))
}

pub fn js_reserved() -> KeySet {
    from_lines("js_reserved", include_str!("../data/js_reserved.txt"))
}

pub fn tickers100() -> KeySet {
    from_lines("tickers100", include_str!("../data/tickers100.txt"))
}

/// `n` distinct lowercase words of 4..=12 letters.
pub fn random_words_n(n: usize, seed: u64, name: &'static str) -> KeySet {
    let mut rng = SplitMix64(seed);
    let mut seen = HashSet::new();
    let mut keys = Vec::new();
    while keys.len() < n {
        let len = 4 + rng.below(9);
        let word: String = (0..len).map(|_| (b'a' + rng.below(26) as u8) as char).collect();
        if seen.insert(word.clone()) {
            keys.push(word);
        }
    }
    KeySet { name, keys }
}

pub fn random_words() -> KeySet {
    random_words_n(1000, 42, "random1000")
}

pub fn random3000() -> KeySet {
    random_words_n(3000, 43, "random3000")
}

/// 500 keys of 26 bytes sharing a 14-byte prefix.
pub fn hard_prefix() -> KeySet {
    KeySet { name: "hard_prefix", keys: (0..500).map(|i| format!("/api/v1/items/{i:05}/detail")).collect() }
}

/// 200 URL paths: 150 over 32 bytes that share long prefixes and suffixes, and 50 short.
pub fn long_paths() -> KeySet {
    let keys = (0..200)
        .map(|i| {
            if i % 4 == 0 {
                format!("/r/{i}")
            } else {
                format!("/v2/organizations/acme-{i:04}/repositories/main/settings")
            }
        })
        .collect();
    KeySet { name: "long_paths", keys }
}

pub fn all() -> Vec<KeySet> {
    vec![
        http_methods(),
        http_headers(),
        rust_keywords(),
        random_words(),
        url_schemes(),
        js_reserved(),
        tickers100(),
        random3000(),
        hard_prefix(),
        long_paths(),
    ]
}

fn fnv1a64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

const EDIT_BYTES: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-";

fn miss(rng: &mut SplitMix64, set: &KeySet, keys: &HashSet<&str>) -> String {
    loop {
        let candidate = if rng.below(4) < 3 {
            let mut b = set.keys[rng.below(set.keys.len())].clone().into_bytes();
            match rng.below(4) {
                0 => {
                    let i = rng.below(b.len());
                    let old = b[i];
                    while b[i] == old {
                        b[i] = EDIT_BYTES[rng.below(EDIT_BYTES.len())];
                    }
                }
                1 => b.push(EDIT_BYTES[rng.below(EDIT_BYTES.len())]),
                2 => {
                    b.pop();
                }
                _ if b.len() >= 2 => {
                    let i = rng.below(b.len() - 1);
                    b.swap(i, i + 1);
                }
                _ => {}
            }
            String::from_utf8(b).expect("key sets are ASCII")
        } else {
            let len = rng.below(21);
            (0..len).map(|_| (b'a' + rng.below(26) as u8) as char).collect()
        };
        if !keys.contains(candidate.as_str()) {
            return candidate;
        }
    }
}

/// `QUERY_COUNT` deterministic queries for `set` under workload `w`.
pub fn queries(set: &KeySet, w: Workload) -> Vec<String> {
    let mut rng = SplitMix64(0x5EED ^ fnv1a64(set.name) ^ w as u64);
    let keys: HashSet<&str> = set.keys.iter().map(String::as_str).collect();
    let hit = |rng: &mut SplitMix64| set.keys[rng.below(set.keys.len())].clone();
    let mut out: Vec<String> = (0..QUERY_COUNT)
        .map(|_| match w {
            Workload::Hit => hit(&mut rng),
            Workload::Miss => miss(&mut rng, set, &keys),
            Workload::Mixed if rng.below(2) == 0 => hit(&mut rng),
            Workload::Mixed => miss(&mut rng, set, &keys),
        })
        .collect();
    if w == Workload::Miss {
        for (i, edge) in ["", "x", "é"].into_iter().enumerate() {
            out[i] = edge.to_owned();
        }
    }
    out
}
