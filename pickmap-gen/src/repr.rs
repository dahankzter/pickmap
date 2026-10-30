//! Per-set choice of how keys become words.

use pickmap_core::load::{classes_for, key_value_with, signature_with};

/// How a map turns keys (and inputs) into `words` u64s: 1, 2 or 4 (8, 16 or 32 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repr {
    /// `load::key_value` with only the length classes the set needs.
    Classes { words: usize, classes: u8 },
    /// `load::signature`: `c` words of `w` bytes.
    Sig { w: usize, c: usize, words: usize },
}

impl Repr {
    pub fn words(self) -> usize {
        match self {
            Repr::Classes { words, .. } | Repr::Sig { words, .. } => words,
        }
    }

    /// The value generated code computes for `k`.
    pub fn value(self, k: &[u8]) -> Vec<u64> {
        macro_rules! at {
            ($words:literal) => {
                match self {
                    Repr::Classes { classes, .. } => key_value_with::<$words>(k, classes).to_vec(),
                    Repr::Sig { w, c, .. } => signature_with::<$words>(k, w, c).to_vec(),
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
fn words_of(bytes: usize) -> usize {
    match bytes {
        0..=8 => 1,
        9..=16 => 2,
        _ => 4,
    }
}

/// The representation needing the fewest loads for the keys of 32 bytes or fewer.
/// Longer keys are compared byte by byte and don't affect the choice.
pub fn repr_for(keys: &[&[u8]]) -> Repr {
    let lens: Vec<usize> = keys.iter().map(|k| k.len()).filter(|&l| (1..=32).contains(&l)).collect();
    let (Some(&min), Some(&max)) = (lens.iter().min(), lens.iter().max()) else {
        return Repr::Classes { words: 1, classes: 0 };
    };
    let (classes, class_loads) = classes_for(lens.iter().copied());
    let w = [8, 4, 2, 1].into_iter().find(|&w| w <= min).unwrap();
    let c = max.div_ceil(w);
    if c <= class_loads {
        Repr::Sig { w, c, words: words_of(c * w) }
    } else {
        Repr::Classes { words: words_of(max), classes }
    }
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
        assert_eq!(repr_for(&bytes(&keysets::http_headers().keys)), Repr::Classes { words: 4, classes: 15 });
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
