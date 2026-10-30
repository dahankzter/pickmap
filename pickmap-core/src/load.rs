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
#[inline(always)]
pub fn opaque(mut x: u64) -> u64 {
    #[cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri)))]
    // SAFETY: the asm is empty: it reads and writes only the register holding `x`, and
    // touches no memory.
    unsafe {
        core::arch::asm!("/* {0} */", inout(reg) x, options(nomem, nostack, preserves_flags));
    }
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
