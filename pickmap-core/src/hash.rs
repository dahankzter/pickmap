//! Slot computation: picked bytes (Lemire/gperf style) and the PtHash fallback.

use crate::load::le_u64;

static ZEROS: [u8; 1] = [0];

/// A byte position, counted from the start or from the end of the key. Positions past
/// the key are clamped onto it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pos {
    Start(u8),
    End(u8),
}

/// The byte at `pos`, branch-free; an empty key reads a zero byte.
#[inline(always)]
pub fn pick_byte(k: &[u8], pos: Pos) -> u8 {
    let k = core::hint::select_unpredictable(k.is_empty(), &ZEROS[..], k);
    let last = k.len() - 1;
    let i = match pos {
        Pos::Start(p) => (p as usize).min(last),
        Pos::End(p) => last - (p as usize).min(last),
    };
    k[i]
}

/// `(len + asso[0][byte at positions[0]] + ...) & mask`. `asso[i]` belongs to
/// `positions[i]`.
#[inline(always)]
pub fn picked_slot(k: &[u8], positions: &[Pos], asso: &[[u16; 256]], mask: u32) -> usize {
    let mut h = k.len() as u32;
    for (&pos, table) in positions.iter().zip(asso) {
        h = h.wrapping_add(table[pick_byte(k, pos) as usize] as u32);
    }
    (h & mask) as usize
}

const K0: u64 = 0xa076_1d64_78bd_642f;
const K1: u64 = 0xe703_7ed1_a0b4_28db;
const K2: u64 = 0x8ebc_6af0_9c88_c6e3;
const K3: u64 = 0x5899_65cc_7537_4cc3;

#[inline(always)]
fn fold_mul(a: u64, b: u64) -> u64 {
    let r = a as u128 * b as u128;
    r as u64 ^ (r >> 64) as u64
}

/// One folded multiply over a key value and its length.
#[inline(always)]
pub fn h_value<const WORDS: usize>(v: &[u64; WORDS], len: usize, seed: u64) -> u64 {
    let (a, b) = match WORDS {
        1 => (v[0], 0),
        2 => (v[0], v[1]),
        _ => (v[0] ^ v[2], v[1] ^ v[3]),
    };
    fold_mul(a ^ seed ^ K0, b ^ len as u64 ^ K1)
}

/// Seeded hash over every byte, for keys longer than 32 bytes.
#[inline(always)]
pub fn h_long(k: &[u8], seed: u64) -> u64 {
    let len = k.len();
    let acc = if len < 16 {
        let mut buf = [0u8; 16];
        buf[..len].copy_from_slice(k);
        fold_mul(le_u64(&buf, 0) ^ seed ^ K0, le_u64(&buf, 8) ^ K1)
    } else {
        let mut acc = seed;
        let mut at = 0;
        while at + 16 < len {
            acc = fold_mul(le_u64(k, at) ^ acc ^ K0, le_u64(k, at + 8) ^ K1);
            at += 16;
        }
        // Tail: the last 16 bytes, overlapping what was already folded.
        fold_mul(le_u64(k, len - 16) ^ acc ^ K0, le_u64(k, len - 8) ^ K1)
    };
    fold_mul(acc ^ K2, len as u64 ^ K3)
}

/// Pilots are stored pre-mixed as `(pilot * PILOT_MUL) as u32`.
pub const PILOT_MUL: u64 = 0x9E37_79B9_7F4A_7C15;

/// The bucket: the top `bucket_bits` bits of `h`.
#[inline(always)]
pub fn pthash_bucket(h: u64, bucket_bits: u32) -> usize {
    // `h >> 1 >> (63 - bits)` is `h >> (64 - bits)` without overflowing at bits == 0.
    (h >> 1 >> (63 - bucket_bits)) as usize
}

#[inline(always)]
pub fn pthash_slot(h: u64, bucket_bits: u32, pilots: &[u32], mask: u32) -> usize {
    ((h as u32 ^ pilots[pthash_bucket(h, bucket_bits)]) & mask) as usize
}

/// 8-bit tag of a full key hash, for PtHash maps: bits 40..48, which the slot (low 32
/// bits) doesn't use and the bucket (top bits) uses only for maps of 2^17+ keys.
#[inline(always)]
pub fn hash_tag(h: u64) -> u8 {
    (h >> 40) as u8
}
