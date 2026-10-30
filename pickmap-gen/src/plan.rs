//! A complete lookup plan for one key set, plus a simulation of the emitted lookup.

use std::collections::HashSet;

use pickmap_core::hash::{h_long, h_value, picked_slot, pthash_slot};
use pickmap_core::slot::{MAX_KEY_LEN, Slot};

use crate::repr::{Repr, repr_for};
use crate::search::{Strategy, search, slot_of, table_len};

pub struct Plan {
    pub repr: Repr,
    pub strategy: Strategy,
    /// `slots[i]` is the index of the key in slot `i`.
    pub slots: Vec<Option<u32>>,
    /// Whether any key is longer than 32 bytes (those are compared byte by byte).
    pub has_long: bool,
}

pub fn plan(keys: &[&str]) -> Result<Plan, String> {
    let mut seen = HashSet::new();
    if let Some(dup) = keys.iter().find(|k| !seen.insert(**k)) {
        return Err(format!("duplicate key {dup:?}"));
    }
    if let Some(k) = keys.iter().find(|k| k.len() > MAX_KEY_LEN) {
        return Err(format!("key of {} bytes is longer than the supported {MAX_KEY_LEN}", k.len()));
    }
    let bytes: Vec<&[u8]> = keys.iter().map(|k| k.as_bytes()).collect();
    let repr = repr_for(&bytes);
    let strategy = search(&bytes, repr)?;
    let mut slots = vec![None; table_len(&strategy)];
    for (i, k) in bytes.iter().enumerate() {
        let s = slot_of(&strategy, repr, k);
        assert!(slots[s].is_none(), "search returned a colliding strategy");
        slots[s] = Some(i as u32);
    }
    Ok(Plan { repr, strategy, slots, has_long: bytes.iter().any(|k| k.len() > 32) })
}

impl Plan {
    /// What the emitted `get` returns for `key`, computed the same way.
    pub fn lookup(&self, keys: &[&str], key: &str) -> Option<usize> {
        let k = key.as_bytes();
        let n = k.len();
        let v = self.repr.value(k);
        let slot = match &self.strategy {
            Strategy::Picked { positions, asso, mask } => picked_slot(k, positions, asso, *mask),
            Strategy::PtHash { seed, bucket_bits, pilots, mask } => {
                let h = if self.has_long && n > 32 {
                    h_long(k, *seed)
                } else {
                    match v.len() {
                        1 => h_value(&[v[0]], n, *seed),
                        2 => h_value(&[v[0], v[1]], n, *seed),
                        _ => h_value(&[v[0], v[1], v[2], v[3]], n, *seed),
                    }
                };
                pthash_slot(h, *bucket_bits, pilots, *mask)
            }
        };
        // Build the slot exactly as the emitter does and ask core, like generated code.
        macro_rules! find {
            ($words:literal) => {{
                let slot = match self.slots[slot] {
                    None => Slot::<$words>::EMPTY,
                    Some(i) => {
                        let stored = keys[i as usize].as_bytes();
                        Slot { key: self.repr.value(stored).try_into().unwrap(), len: stored.len() as u32, idx: i }
                    }
                };
                let v: [u64; $words] = v.clone().try_into().unwrap();
                if self.has_long { slot.find_long(&v, k, keys) } else { slot.find(&v, n) }
            }};
        }
        match self.repr.words() {
            1 => find!(1),
            2 => find!(2),
            _ => find!(4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keysets::SplitMix64;
    use std::collections::HashMap;

    fn reference<'a>(keys: &[&'a str]) -> HashMap<&'a str, usize> {
        keys.iter().enumerate().map(|(i, k)| (*k, i)).collect()
    }

    #[test]
    fn plan_lookup_matches_hashmap() {
        for set in keysets::all() {
            let keys: Vec<&str> = set.keys.iter().map(String::as_str).collect();
            let p = plan(&keys).expect("plan");
            let r = reference(&keys);
            for (i, k) in keys.iter().enumerate() {
                assert_eq!(p.lookup(&keys, k), Some(i), "{} {k:?}", set.name);
            }
            for w in keysets::WORKLOADS {
                for q in keysets::queries(&set, w) {
                    assert_eq!(p.lookup(&keys, &q), r.get(q.as_str()).copied(), "{} {q:?}", set.name);
                }
            }
        }
    }

    fn random_key(rng: &mut SplitMix64, shape: usize) -> String {
        let lower = |rng: &mut SplitMix64, n: usize| -> String { (0..n).map(|_| (b'a' + rng.below(26) as u8) as char).collect() };
        match shape {
            0 => {
                let n = 1 + rng.below(12);
                lower(rng, n)
            }
            1 => format!("shared/pre{}", lower(rng, 3)),
            2 => lower(rng, 6),
            3 => {
                let n = rng.below(61);
                lower(rng, n)
            }
            _ => {
                let n = 1 + rng.below(8);
                (0..n).map(|_| char::from_u32('à' as u32 + rng.below(('ž' as u32 - 'à' as u32) as usize) as u32).unwrap()).collect()
            }
        }
    }

    #[test]
    fn property_random_sets() {
        const SIZES: [usize; 9] = [0, 1, 2, 3, 5, 17, 64, 300, 2000];
        for seed in 1..=40u64 {
            let mut rng = SplitMix64(seed);
            let size = SIZES[rng.below(SIZES.len())];
            let shape = rng.below(5);
            let mut owned: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut tries = 0;
            // Small shapes may not have `size` distinct keys; stop after enough tries.
            while owned.len() < size && tries < size * 50 {
                tries += 1;
                let k = random_key(&mut rng, shape);
                if seen.insert(k.clone()) {
                    owned.push(k);
                }
            }
            let keys: Vec<&str> = owned.iter().map(String::as_str).collect();
            let p = plan(&keys).unwrap_or_else(|e| panic!("seed {seed} shape {shape} size {}: {e}", keys.len()));
            let r = reference(&keys);
            for (i, k) in keys.iter().enumerate() {
                assert_eq!(p.lookup(&keys, k), Some(i), "seed {seed} {k:?}");
            }
            for _ in 0..2000 {
                let n = rng.below(71);
                let q: String = (0..n).map(|_| (b' ' + rng.below(95) as u8) as char).collect();
                assert_eq!(p.lookup(&keys, &q), r.get(q.as_str()).copied(), "seed {seed} {q:?}");
            }
            for k in &keys {
                for i in 0..k.len() {
                    let mut b = k.as_bytes().to_vec();
                    b[i] ^= 0x01;
                    if let Ok(q) = String::from_utf8(b) {
                        assert_eq!(p.lookup(&keys, &q), r.get(q.as_str()).copied(), "seed {seed} {q:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn empty_set_plan() {
        let p = plan(&[]).expect("plan");
        assert_eq!(p.lookup(&[], ""), None);
        assert_eq!(p.lookup(&[], "a"), None);
    }

    #[test]
    fn duplicate_keys_rejected() {
        assert!(plan(&["a", "b", "a"]).is_err());
    }
}
