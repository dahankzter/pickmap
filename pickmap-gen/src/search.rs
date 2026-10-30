//! Perfect-hash search: picked bytes first, PtHash as the fallback.

use std::collections::HashSet;

use pickmap_core::hash::{PILOT_MUL, Pos, h_long, h_value, pick_byte, picked_slot, pthash_bucket, pthash_slot};

use crate::repr::Repr;

#[derive(Clone, Debug)]
pub enum Strategy {
    /// `asso[i]` belongs to `positions[i]`.
    Picked { positions: Vec<Pos>, asso: Vec<[u16; 256]>, mask: u32 },
    /// `pilots` are stored pre-mixed: `(pilot * PILOT_MUL) as u32`.
    PtHash { seed: u64, bucket_bits: u32, pilots: Vec<u32>, mask: u32 },
}

const STEP_BUDGET: usize = 200_000;
/// Across all picked-byte attempts, before falling back to PtHash.
const TOTAL_STEP_BUDGET: usize = 1_000_000;
const MAX_POSITIONS: usize = 3;
const PILOTS: u64 = 1 << 20;
const SEEDS: u64 = 64;

/// The hash generated code computes: `h_long` for keys over 32 bytes, otherwise
/// `h_value` over the key's value.
pub fn key_hash(repr: Repr, k: &[u8], seed: u64) -> u64 {
    if k.len() > 32 {
        return h_long(k, seed);
    }
    let v = repr.value(k);
    match v.len() {
        1 => h_value(&[v[0]], k.len(), seed),
        2 => h_value(&[v[0], v[1]], k.len(), seed),
        _ => h_value(&[v[0], v[1], v[2], v[3]], k.len(), seed),
    }
}

pub fn search(keys: &[&[u8]], repr: Repr) -> Result<Strategy, String> {
    search_picked(keys)
        .or_else(|| search_pthash(keys, repr))
        .ok_or_else(|| format!("no perfect hash found for {} keys", keys.len()))
}

pub fn slot_of(s: &Strategy, repr: Repr, k: &[u8]) -> usize {
    match s {
        Strategy::Picked { positions, asso, mask } => picked_slot(k, positions, asso, *mask),
        Strategy::PtHash { seed, bucket_bits, pilots, mask } => {
            pthash_slot(key_hash(repr, k, *seed), *bucket_bits, pilots, *mask)
        }
    }
}

pub fn table_len(s: &Strategy) -> usize {
    match s {
        Strategy::Picked { mask, .. } | Strategy::PtHash { mask, .. } => *mask as usize + 1,
    }
}

/// All `k`-element combinations of `0..n`, in lexicographic order.
fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    fn go(start: usize, n: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for i in start..n {
            cur.push(i);
            go(i + 1, n, k, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    go(0, n, k, &mut Vec::new(), &mut out);
    out
}

/// Fewest positions first, then the smallest table. `k == 0` is length-only.
fn search_picked(keys: &[&[u8]]) -> Option<Strategy> {
    let base = keys.len().next_power_of_two();
    // Association values are u16, so tables are capped at 65,536 slots.
    if 8 * base > 65_536 {
        return None;
    }
    let cands: Vec<Pos> = (0..8).map(Pos::Start).chain((0..8).map(Pos::End)).collect();
    // The bytes each candidate picks from every key. On short keys many candidates clamp
    // onto the same byte, so their columns are identical.
    let columns: Vec<Vec<u8>> = cands.iter().map(|&p| keys.iter().map(|k| pick_byte(k, p)).collect()).collect();
    let mut seen: HashSet<Vec<&Vec<u8>>> = HashSet::new();
    let mut budget = TOTAL_STEP_BUDGET;
    for k in 0..=MAX_POSITIONS {
        let combos: Vec<Vec<usize>> = combinations(cands.len(), k)
            .into_iter()
            .filter(|c| {
                // Skip combinations equivalent to one already kept: the same set of
                // columns, or two positions that always pick the same byte.
                let mut cols: Vec<&Vec<u8>> = c.iter().map(|&i| &columns[i]).collect();
                cols.sort();
                let len = cols.len();
                cols.dedup();
                cols.len() == len && seen.insert(cols)
            })
            .filter(|c| feasible(keys, &c.iter().map(|&i| cands[i]).collect::<Vec<Pos>>()))
            .collect();
        for m in [base, base * 2, base * 4, base * 8] {
            for c in &combos {
                let positions: Vec<Pos> = c.iter().map(|&i| cands[i]).collect();
                if let Some(asso) = assign(keys, &positions, m, &mut budget) {
                    return Some(Strategy::Picked { positions, asso, mask: (m - 1) as u32 });
                }
                if budget == 0 {
                    return None;
                }
            }
        }
    }
    None
}

/// With one table per position, keys only need distinct `(len, picked bytes)` tuples.
fn feasible(keys: &[&[u8]], positions: &[Pos]) -> bool {
    let mut seen = HashSet::new();
    keys.iter().all(|k| seen.insert((k.len(), positions.iter().map(|&p| pick_byte(k, p)).collect::<Vec<u8>>())))
}

/// Depth-first assignment of association values with backtracking. A variable is one
/// `(position, byte)` table entry; the most-used variables are assigned first.
/// Charges every step against `budget` as well as the per-attempt limit.
fn assign(keys: &[&[u8]], positions: &[Pos], m: usize, budget: &mut usize) -> Option<Vec<[u16; 256]>> {
    let var = |pi: usize, b: u8| pi * 256 + b as usize;
    let picks: Vec<Vec<usize>> =
        keys.iter().map(|k| positions.iter().enumerate().map(|(pi, &p)| var(pi, pick_byte(k, p))).collect()).collect();
    let mut freq = vec![0usize; positions.len() * 256];
    for p in &picks {
        for &v in p {
            freq[v] += 1;
        }
    }
    let mut order: Vec<usize> = (0..freq.len()).filter(|&v| freq[v] > 0).collect();
    order.sort_by_key(|&v| std::cmp::Reverse(freq[v]));
    let mut rank = vec![usize::MAX; freq.len()];
    for (i, &v) in order.iter().enumerate() {
        rank[v] = i;
    }
    // Keys whose slot becomes known once `order[d]` is assigned.
    let mut completes: Vec<Vec<usize>> = vec![Vec::new(); order.len().max(1)];
    for (ki, p) in picks.iter().enumerate() {
        completes[p.iter().map(|&v| rank[v]).max().unwrap_or(0)].push(ki);
    }

    let mut values = vec![0usize; freq.len()];
    let mut taken = vec![false; m];
    let place = |d: usize, values: &[usize], taken: &mut [bool]| -> Option<Vec<usize>> {
        let mut placed = Vec::new();
        for &ki in &completes[d] {
            let s = picks[ki].iter().fold(keys[ki].len(), |h, &v| h + values[v]) & (m - 1);
            if taken[s] {
                for &p in &placed {
                    taken[p] = false;
                }
                return None;
            }
            taken[s] = true;
            placed.push(s);
        }
        Some(placed)
    };
    let finish = |values: &[usize]| -> Vec<[u16; 256]> {
        (0..positions.len()).map(|pi| std::array::from_fn(|b| values[pi * 256 + b] as u16)).collect()
    };

    if order.is_empty() {
        return place(0, &values, &mut taken).map(|_| finish(&values));
    }
    let mut steps = 0usize;
    // Explicit stack: (depth, next value to try, slots placed at this depth).
    let mut stack: Vec<(usize, usize, Vec<usize>)> = vec![(0, 0, Vec::new())];
    while let Some((d, v, placed)) = stack.pop() {
        for &p in &placed {
            taken[p] = false;
        }
        if v >= m {
            continue;
        }
        steps += 1;
        if steps > STEP_BUDGET || *budget == 0 {
            return None;
        }
        *budget -= 1;
        values[order[d]] = v;
        match place(d, &values, &mut taken) {
            Some(now) => {
                if d + 1 == order.len() {
                    return Some(finish(&values));
                }
                stack.push((d, v + 1, now));
                stack.push((d + 1, 0, Vec::new()));
            }
            None => stack.push((d, v + 1, Vec::new())),
        }
    }
    None
}

/// Tries `next_power_of_two(n)` slots (load factor up to 1), then twice that.
fn search_pthash(keys: &[&[u8]], repr: Repr) -> Option<Strategy> {
    let base = keys.len().max(1).next_power_of_two();
    search_pthash_m(keys, repr, base).or_else(|| search_pthash_m(keys, repr, base * 2))
}

fn search_pthash_m(keys: &[&[u8]], repr: Repr, m: usize) -> Option<Strategy> {
    let n = keys.len();
    let mask = (m - 1) as u32;
    // About two keys per bucket: small buckets make load factors near 1 work.
    let bucket_bits = (n / 2).max(1).next_power_of_two().trailing_zeros();
    let buckets = 1usize << bucket_bits;
    let mut taken = vec![false; m];
    let mut slots: Vec<usize> = Vec::new();
    'seed: for seed in 0..SEEDS {
        let mut hs: Vec<u64> = keys.iter().map(|k| key_hash(repr, k, seed)).collect();
        let mut sorted = hs.clone();
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            continue;
        }
        hs.sort_unstable_by_key(|&h| pthash_bucket(h, bucket_bits));
        let mut members: Vec<&[u64]> = Vec::with_capacity(buckets);
        let mut start = 0;
        for b in 0..buckets {
            let end = start + hs[start..].iter().take_while(|&&h| pthash_bucket(h, bucket_bits) == b).count();
            members.push(&hs[start..end]);
            start = end;
        }
        let mut order: Vec<usize> = (0..buckets).collect();
        order.sort_by_key(|&b| std::cmp::Reverse(members[b].len()));
        taken.fill(false);
        let mut pilots = vec![0u32; buckets];
        for b in order {
            if members[b].is_empty() {
                break;
            }
            // No allocation in this loop: `slots` is reused, duplicates found by scan.
            let found = (0..PILOTS).map(|p| p.wrapping_mul(PILOT_MUL) as u32).find(|&pm| {
                slots.clear();
                for &h in members[b] {
                    let s = ((h as u32 ^ pm) & mask) as usize;
                    if taken[s] || slots.contains(&s) {
                        return false;
                    }
                    slots.push(s);
                }
                true
            });
            let Some(pm) = found else { continue 'seed };
            pilots[b] = pm;
            for &h in members[b] {
                taken[((h as u32 ^ pm) & mask) as usize] = true;
            }
        }
        // Self-check against the run-time slot function.
        debug_assert!(hs.iter().all(|&h| taken[pthash_slot(h, bucket_bits, &pilots, mask)]));
        return Some(Strategy::PtHash { seed, bucket_bits, pilots, mask });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::repr_for;
    use std::collections::HashSet;

    fn check_perfect(keys: &[&[u8]]) -> Strategy {
        let repr = repr_for(keys);
        let s = search(keys, repr).expect("search");
        let slots: HashSet<usize> = keys.iter().map(|k| slot_of(&s, repr, k)).collect();
        assert_eq!(slots.len(), keys.len(), "slot collision");
        assert!(slots.iter().all(|&i| i < table_len(&s)));
        s
    }

    fn bytes(keys: &[String]) -> Vec<&[u8]> {
        keys.iter().map(|k| k.as_bytes()).collect()
    }

    #[test]
    fn perfect_for_every_keyset() {
        for set in keysets::all() {
            check_perfect(&bytes(&set.keys));
        }
    }

    #[test]
    fn small_sets_use_picked() {
        for set in [keysets::http_methods(), keysets::http_headers(), keysets::rust_keywords()] {
            assert!(matches!(check_perfect(&bytes(&set.keys)), Strategy::Picked { .. }), "{}", set.name);
        }
    }

    #[test]
    fn random1000_uses_pthash_1024() {
        let s = check_perfect(&bytes(&keysets::random_words().keys));
        assert!(matches!(s, Strategy::PtHash { .. }));
        assert_eq!(table_len(&s), 1024);
    }

    #[test]
    fn long_keys_share_ends() {
        let keys: Vec<String> = (0..300).map(|i| format!("{}{i:08}{}", "A".repeat(16), "Z".repeat(16))).collect();
        assert!(keys.iter().all(|k| k.len() == 40));
        check_perfect(&bytes(&keys));
    }
}
