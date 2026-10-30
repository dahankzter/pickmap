//! Per-set choice of how keys become words.

use pickmap_core::load::{MID, SMALL, classes_for, key_value_with, padded_value, signature_with, split_value_with};

/// How a map turns keys (and inputs) into `words` u64s: 1, 2 or 4 (8, 16 or 32 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repr {
    /// `load::key_value` with only the length classes the set needs.
    Classes { words: usize, classes: u8 },
    /// `load::signature`: `c` words of `w` bytes.
    Sig { w: usize, c: usize, words: usize },
    /// `load::split_value`: `c` 8-byte words from 8 bytes on, the `short` classes below.
    Split { c: usize, words: usize, short: u8 },
    /// The key zero-padded to `8 * words` bytes, as `get_in`'s masked load reads it
    /// (`load::masked_in`). Only used for `get_in`'s tables, never chosen for `get`.
    Padded { words: usize },
}

impl Repr {
    pub fn words(self) -> usize {
        match self {
            Repr::Classes { words, .. }
            | Repr::Sig { words, .. }
            | Repr::Split { words, .. }
            | Repr::Padded { words } => words,
        }
    }

    /// The value generated code computes for `k`.
    pub fn value(self, k: &[u8]) -> Vec<u64> {
        macro_rules! at {
            ($words:literal) => {
                match self {
                    Repr::Classes { classes, .. } => key_value_with::<$words>(k, classes).to_vec(),
                    Repr::Sig { w, c, .. } => signature_with::<$words>(k, w, c).to_vec(),
                    Repr::Split { c, short, .. } => split_value_with::<$words>(k, c, short).to_vec(),
                    // Inputs longer than the load keep its first 8 * words bytes.
                    Repr::Padded { .. } => padded_value::<$words>(&k[..k.len().min(8 * $words)]).to_vec(),
                }
            };
        }
        match self.words() {
            1 => at!(1),
            2 => at!(2),
            _ => at!(4),
        }
    }
}

/// The fewest words (1, 2 or 4) that hold `bytes` bytes.
pub(crate) fn words_of(bytes: usize) -> usize {
    match bytes {
        0..=8 => 1,
        9..=16 => 2,
        _ => 4,
    }
}

/// The representation needing the fewest loads for the keys of 32 bytes or fewer.
/// Longer keys are compared byte by byte and don't affect the choice.
pub fn repr_for(keys: &[&[u8]]) -> Repr {
    repr_choice(keys, true)
}

/// [`repr_for`] without [`Repr::Split`], for maps that hash their keys: the split value
/// repeats words for shorter inputs (all four words are equal at 8 bytes), and
/// `h_value` folds 4-word values pairwise with XOR, which cancels them.
pub fn repr_for_hashing(keys: &[&[u8]]) -> Repr {
    repr_choice(keys, false)
}

fn repr_choice(keys: &[&[u8]], allow_split: bool) -> Repr {
    let lens: Vec<usize> = keys.iter().map(|k| k.len()).filter(|&l| (1..=32).contains(&l)).collect();
    let (Some(&min), Some(&max)) = (lens.iter().min(), lens.iter().max()) else {
        return Repr::Classes { words: 1, classes: 0 };
    };
    let (classes, class_loads) = classes_for(lens.iter().copied());
    let w = [8, 4, 2, 1].into_iter().find(|&w| w <= min).unwrap();
    let c = max.div_ceil(w);
    let sig = (c, Repr::Sig { w, c, words: words_of(c * w) });
    let mut best = sig;
    // Lengths straddling 8 bytes: 8-byte words above, the short classes below.
    if allow_split && min < 8 && max >= 8 {
        let short = classes & (SMALL | MID);
        // SMALL costs 3 loads, MID 2.
        let short_loads = 3 * (short & SMALL != 0) as usize + 2 * (short & MID != 0) as usize;
        let c8 = max.div_ceil(8);
        let split = (short_loads + c8, Repr::Split { c: c8, words: words_of(8 * c8), short });
        if split.0 < best.0 {
            best = split;
        }
    }
    if class_loads < best.0 {
        best = (class_loads, Repr::Classes { words: words_of(max), classes });
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn bytes(keys: &[String]) -> Vec<&[u8]> {
        keys.iter().map(|k| k.as_bytes()).collect()
    }

    #[test]
    fn repr_matches_spike() {
        assert_eq!(repr_for(&bytes(&keysets::http_methods().keys)), Repr::Sig { w: 2, c: 4, words: 1 });
        assert_eq!(repr_for(&bytes(&keysets::rust_keywords().keys)), Repr::Sig { w: 2, c: 4, words: 1 });
        assert_eq!(repr_for(&bytes(&keysets::random_words().keys)), Repr::Sig { w: 4, c: 3, words: 2 });
        assert_eq!(repr_for(&bytes(&keysets::http_headers().keys)), Repr::Split { c: 4, words: 4, short: 3 });
    }

    #[test]
    fn long_keys_ignored_for_repr() {
        let long = "z".repeat(40);
        assert_eq!(repr_for(&[b"ab", long.as_bytes()]), Repr::Sig { w: 2, c: 1, words: 1 });
    }

    #[test]
    fn empty_and_empty_string() {
        assert_eq!(repr_for(&[]), Repr::Classes { words: 1, classes: 0 });
        assert_eq!(repr_for(&[b""]), Repr::Classes { words: 1, classes: 0 });
    }

    #[test]
    fn values_are_exact_per_set() {
        for set in keysets::all() {
            let keys = bytes(&set.keys);
            let repr = repr_for(&keys);
            let mut by_len: HashMap<usize, HashSet<Vec<u64>>> = HashMap::new();
            // Keys over 32 bytes are compared byte by byte; their value is never used.
            for k in keys.iter().filter(|k| k.len() <= 32) {
                assert!(by_len.entry(k.len()).or_default().insert(repr.value(k)), "{}: {repr:?}", set.name);
            }
        }
    }
}
