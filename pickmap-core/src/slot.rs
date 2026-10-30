//! Table slots and the two key compares.
//!
//! Both compares keep every branch before the final hit/miss answer predictable on any
//! input stream, including near-misses that share most bytes with a key:
//! - [`Slot::find`] (picked-byte maps) compares length and every key word in one
//!   combined expression behind an optimization barrier, then branches once.
//! - [`Slot::find_tagged`] (PtHash maps) first compares an 8-bit tag of the key's hash,
//!   which they compute anyway. A near-miss has a different hash, so this branch is in
//!   effect "hit or miss?". Only likely hits go on to the full compare.

/// Longest supported key, in bytes: a tagged slot keeps the length in 24 bits.
pub const MAX_KEY_LEN: usize = (1 << 24) - 2;

/// A tagged slot's `meta`: the key length in the low 24 bits and the tag on top. The
/// length is clamped to `MAX_KEY_LEN + 1` (branch-free), so no input length wraps onto a
/// key's.
#[inline(always)]
pub fn meta(len: usize, tag: u8) -> u32 {
    len.min(MAX_KEY_LEN + 1) as u32 | (tag as u32) << 24
}

/// One table entry: the key's value (see [`crate::load`]), its `meta`, and its index in
/// declaration order. `meta` is the key length for [`Slot::find`], and [`meta`] of length
/// and tag for [`Slot::find_tagged`].
///
/// Empty slots have `meta` 0 (length 0) and all-ones key words. Only a zero-length input
/// has length 0, and every representation maps it to all-zero words, so no input
/// matches an empty slot.
#[repr(C)]
pub struct Slot<const WORDS: usize> {
    pub key: [u64; WORDS],
    pub meta: u32,
    pub idx: u32,
}

impl<const WORDS: usize> Slot<WORDS> {
    pub const EMPTY: Self = Slot { key: [u64::MAX; WORDS], meta: 0, idx: 0 };

    #[inline(always)]
    fn diff(&self, v: &[u64; WORDS]) -> u64 {
        let mut diff = 0;
        for i in 0..WORDS {
            diff |= self.key[i] ^ v[i];
        }
        diff
    }

    /// The key index if `v` and `n` are this slot's key (`meta` is the key length).
    /// Length and every word are compared in one combined expression, so neither becomes
    /// a separate branch.
    #[inline(always)]
    pub fn find(&self, v: &[u64; WORDS], n: usize) -> Option<usize> {
        let diff = (self.meta as u64 ^ n as u64) | self.diff(v);
        // Through the barrier: otherwise LLVM splits `diff == 0` back into one
        // compare-and-branch per word, and near-misses mispredict them.
        if crate::load::opaque(diff) == 0 { Some(self.idx as usize) } else { None }
    }

    /// [`Slot::find`] for maps that hold keys longer than 32 bytes: those inputs are
    /// compared byte by byte against `keys[idx]`.
    #[inline(always)]
    pub fn find_long(&self, v: &[u64; WORDS], k: &[u8], keys: &[&str]) -> Option<usize> {
        if k.len() > 32 {
            let i = self.idx as usize;
            if self.meta as usize == k.len() && keys[i].as_bytes() == k { Some(i) } else { None }
        } else {
            self.find(v, k.len())
        }
    }

    /// The key index if `v` and `meta` (length and tag) are this slot's key. The first
    /// branch is the tag-and-length check; the word compare only runs for likely hits,
    /// where it is predictable.
    #[inline(always)]
    pub fn find_tagged(&self, v: &[u64; WORDS], meta: u32) -> Option<usize> {
        if self.meta != meta {
            return None;
        }
        // Word by word: `==` on the arrays becomes a `bcmp` call for 4 words.
        if self.diff(v) != 0 {
            return None;
        }
        Some(self.idx as usize)
    }

    /// [`Slot::find_tagged`] for maps that hold keys longer than 32 bytes.
    #[inline(always)]
    pub fn find_tagged_long(&self, v: &[u64; WORDS], meta: u32, k: &[u8], keys: &[&str]) -> Option<usize> {
        if self.meta != meta {
            return None;
        }
        let i = self.idx as usize;
        let eq = if k.len() > 32 { keys[i].as_bytes() == k } else { self.diff(v) == 0 };
        if eq { Some(i) } else { None }
    }
}
