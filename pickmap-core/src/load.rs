//! Branch-free key values: a key (or any input) becomes 1, 2 or 4 `u64` words, such
//! that two inputs of the same length are equal exactly when their words are.
//!
//! No function here reads outside its input. An input too short for some load reads a
//! static zero buffer instead, chosen with a branch-free select.

use core::hint::select_unpredictable as sel;

static ZEROS: [u8; 16] = [0; 16];

/// Length classes for [`key_value`]. A set only needs the classes its key lengths fall in.
pub const SMALL: u8 = 1; // 1..=3 bytes, 3 loads
pub const MID: u8 = 2; // 4..=7 bytes, 2 loads
pub const BIG: u8 = 4; // 8..=16 bytes, 2 loads
pub const HUGE: u8 = 8; // 17..=32 bytes, 4 loads (needs WORDS == 4)

/// Optimization barrier: returns `x` unchanged, but LLVM can't reason about the result,
/// so it can't turn the surrounding branch-free code back into branches. An empty asm
/// block, as constant-time code uses; it emits no instructions. Not `pure`, so LLVM
/// can't sink it into a branch either.
#[cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri)))]
#[inline(always)]
pub fn opaque(mut x: u64) -> u64 {
    // SAFETY: the asm is empty: it reads and writes only the register holding `x`, and
    // touches no memory.
    unsafe {
        core::arch::asm!("/* {0} */", inout(reg) x, options(nomem, nostack, preserves_flags));
    }
    x
}

/// Other targets and Miri: a plain identity. Correct everywhere, but LLVM may then turn
/// the branch-free code back into branches.
#[cfg(not(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri))))]
#[inline(always)]
pub fn opaque(x: u64) -> u64 {
    x
}

#[inline(always)]
pub(crate) fn le_u64(k: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(k[at..at + 8].try_into().unwrap())
}

#[inline(always)]
fn le_u32(k: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(k[at..at + 4].try_into().unwrap())
}

/// Branch-free exact value for keys of up to `8 * WORDS` bytes whose lengths fall in
/// `CLASSES`. Each class reads the input when the input is long enough for it and the
/// zero buffer otherwise; then the input's class is selected without branching. Inputs
/// of other lengths still get some value, and callers reject them by length.
#[inline(always)]
pub fn key_value<const WORDS: usize, const CLASSES: u8>(k: &[u8]) -> [u64; WORDS] {
    key_value_with(k, CLASSES)
}

/// [`key_value`] with the classes as a run-time argument, for build-time use.
#[inline(always)]
pub fn key_value_with<const WORDS: usize>(k: &[u8], classes: u8) -> [u64; WORDS] {
    let n = k.len();
    let small = if classes & SMALL != 0 {
        let s = sel(n >= 1, k, &ZEROS[..1]);
        let l = s.len();
        // First, middle and last byte cover 1..=3 bytes.
        s[0] as u64 | (s[l / 2] as u64) << 8 | (s[l - 1] as u64) << 16
    } else {
        0
    };
    let mid = if classes & MID != 0 {
        let s = sel(n >= 4, k, &ZEROS[..4]);
        // Two overlapping 4-byte words.
        le_u32(s, 0) as u64 | (le_u32(s, s.len() - 4) as u64) << 32
    } else {
        0
    };
    let big = if classes & BIG != 0 {
        let s = sel(n >= 8, k, &ZEROS[..8]);
        // Two overlapping 8-byte words.
        [le_u64(s, 0), le_u64(s, s.len() - 8)]
    } else {
        [0, 0]
    };
    let mut v = [0u64; WORDS];
    v[0] = sel(n >= 8, big[0], sel(n >= 4, mid, small));
    if WORDS >= 2 {
        v[1] = sel(n >= 8, big[1], 0);
    }
    if WORDS == 4 && classes & HUGE != 0 {
        let s = sel(n > 16, k, &ZEROS[..16]);
        let l = s.len();
        // Four words covering the first and the last 16 bytes.
        let huge = [le_u64(s, 0), le_u64(s, 8), le_u64(s, l - 16), le_u64(s, l - 8)];
        // Blend with an opaque mask: with a select, or a mask LLVM can see through, it
        // branches around the short classes for long inputs.
        let long = opaque(((n > 16) as u64).wrapping_neg());
        for i in 0..4 {
            v[i] = (huge[i] & long) | (v[i] & !long);
        }
    }
    v
}

/// The classes a set of key lengths needs, and how many loads they cost.
pub fn classes_for(lens: impl Iterator<Item = usize>) -> (u8, usize) {
    let classes = lens.fold(0, |c, l| {
        c | match l {
            0 => 0,
            1..=3 => SMALL,
            4..=7 => MID,
            8..=16 => BIG,
            _ => HUGE,
        }
    });
    let loads = [(SMALL, 3), (MID, 2), (BIG, 2), (HUGE, 4)]
        .iter()
        .filter(|(c, _)| classes & c != 0)
        .map(|(_, l)| l)
        .sum();
    (classes, loads)
}

/// Branch-free exact signature: `C` words of `W` bytes at offsets `min(i * W, len - W)`,
/// packed little-endian into `WORDS` u64s. Exact for inputs of `W..=C * W` bytes. Inputs
/// shorter than `W` read the zero buffer instead.
#[inline(always)]
pub fn signature<const W: usize, const C: usize, const WORDS: usize>(k: &[u8]) -> [u64; WORDS] {
    signature_with(k, W, C)
}

/// [`signature`] with `w` and `c` as run-time arguments, for build-time use.
#[inline(always)]
pub fn signature_with<const WORDS: usize>(k: &[u8], w: usize, c: usize) -> [u64; WORDS] {
    let k = sel(k.len() >= w, k, &ZEROS[..w]);
    let last = k.len() - w;
    let mut v = [0u64; WORDS];
    for i in 0..c {
        let at = (i * w).min(last);
        let word = match w {
            1 => k[at] as u64,
            2 => u16::from_le_bytes(k[at..at + 2].try_into().unwrap()) as u64,
            4 => le_u32(k, at) as u64,
            _ => le_u64(k, at),
        };
        let bit = i * w * 8;
        v[bit / 64] |= word << (bit % 64);
    }
    v
}

/// Branch-free exact value for sets whose key lengths straddle 8 bytes: inputs of 8 bytes
/// or more use `C` overlapping 8-byte words at offsets `min(8 * i, len - 8)`, shorter ones
/// the `SHORT` classes ([`SMALL`] and/or [`MID`]). Exact for keys up to `8 * C` bytes.
#[inline(always)]
pub fn split_value<const WORDS: usize, const C: usize, const SHORT: u8>(k: &[u8]) -> [u64; WORDS] {
    split_value_with(k, C, SHORT)
}

/// [`split_value`] with `c` and `short` as run-time arguments, for build-time use.
#[inline(always)]
pub fn split_value_with<const WORDS: usize>(k: &[u8], c: usize, short: u8) -> [u64; WORDS] {
    // Each half is zero outside its range: the signature reads the zero buffer for inputs
    // under 8 bytes, and without BIG the short classes give 0 from 8 bytes on. So the
    // halves combine with a plain OR, no select.
    let mut v = signature_with::<WORDS>(k, 8, c);
    v[0] |= key_value_with::<1>(k, short & (SMALL | MID))[0];
    v
}

/// `MASKS[n][i]` keeps the bytes of word `i` that lie inside an `n`-byte key.
static MASKS: [[u64; 4]; 33] = {
    let mut m = [[0u64; 4]; 33];
    let mut n = 0;
    while n <= 32 {
        let mut i = 0;
        while i < 4 {
            let bytes = if n >= (i + 1) * 8 {
                8
            } else if n > i * 8 {
                n - i * 8
            } else {
                0
            };
            m[n][i] = if bytes == 8 { u64::MAX } else { (1u64 << (bytes * 8)) - 1 };
            i += 1;
        }
        n += 1;
    }
    m
};

/// The `n`-byte key at `buf[start..]`, zero-padded to `8 * WORDS` bytes: one wide load
/// from `buf`, masked to the key. This is Lemire's over-read, done safely: the caller
/// guarantees `start + 8 * WORDS <= buf.len()`, so every byte read is inside `buf`.
/// For `n > 8 * WORDS` the value is the first `8 * WORDS` bytes; callers reject such
/// inputs by length.
#[inline(always)]
pub fn masked_in<const WORDS: usize>(buf: &[u8], start: usize, n: usize) -> [u64; WORDS] {
    let s = &buf[start..start + 8 * WORDS];
    let m = &MASKS[n.min(8 * WORDS)];
    // With AVX2 or newer, LLVM vectorizes the loads and masks, then moves the lanes back
    // out on the hash's critical path; the barrier keeps each word scalar. Without AVX it
    // doesn't, and the barrier would only cost.
    #[cfg(target_feature = "avx2")]
    return core::array::from_fn(|i| opaque(le_u64(s, 8 * i)) & m[i]);
    #[cfg(not(target_feature = "avx2"))]
    return core::array::from_fn(|i| le_u64(s, 8 * i) & m[i]);
}

/// `k` zero-padded to `8 * WORDS` bytes, as little-endian words. Build-time twin of
/// [`masked_in`]; only for `k.len() <= 8 * WORDS`.
pub fn padded_value<const WORDS: usize>(k: &[u8]) -> [u64; WORDS] {
    let mut buf = [0u8; 32];
    buf[..k.len()].copy_from_slice(k);
    core::array::from_fn(|i| le_u64(&buf, 8 * i))
}

/// The key zero-padded to `8 * WORDS` bytes (its first `8 * WORDS` bytes if longer), in
/// one AVX-512 masked load straight from the key. Equal to [`padded_value`] of
/// `&k[..k.len().min(8 * WORDS)]` and to [`masked_in`], so maps use the same tables.
///
/// Only in builds compiled with AVX-512BW and AVX-512VL (e.g. `-C target-cpu=native` on
/// Zen 4/5 or recent Intel).
#[cfg(all(target_arch = "x86_64", target_feature = "avx512bw", target_feature = "avx512vl", not(miri)))]
#[inline(always)]
pub fn masked_key<const WORDS: usize>(k: &[u8]) -> [u64; WORDS] {
    use core::arch::x86_64::{__m128i, __m256i, _mm_maskz_loadu_epi8, _mm256_maskz_loadu_epi8};
    let n = k.len().min(8 * WORDS);
    let p = k.as_ptr().cast::<i8>();
    let mut out = [0u64; WORDS];
    // SAFETY: a masked load reads only the bytes whose mask bit is set, and faults on
    // none of the others. The mask covers exactly the first `n <= k.len()` bytes of `k`,
    // which are in bounds; with `n == 0` nothing is read, so even an empty slice's
    // dangling pointer is fine. The target features are guaranteed by the `cfg` above.
    // The vectors are plain 16 or 32-byte values, read back as u64 lanes.
    unsafe {
        if WORDS <= 2 {
            let v: __m128i = _mm_maskz_loadu_epi8(((1u32 << n) - 1) as u16, p);
            let lanes: [u64; 2] = core::mem::transmute(v);
            out.copy_from_slice(&lanes[..WORDS]);
        } else {
            let mask = if n == 32 { u32::MAX } else { (1u32 << n) - 1 };
            let v: __m256i = _mm256_maskz_loadu_epi8(mask, p);
            let lanes: [u64; 4] = core::mem::transmute(v);
            out.copy_from_slice(&lanes[..WORDS]);
        }
    }
    out
}
