//! Table slots and the single combined compare.

/// One table entry: the key's value (see [`crate::load`]), its length, and its index in
/// declaration order. Empty slots have `len == u32::MAX`; input lengths are clamped below
/// that, so no input matches an empty slot. Keys are shorter than [`MAX_KEY_LEN`].
/// Longest supported key, in bytes.
pub const MAX_KEY_LEN: usize = u32::MAX as usize - 2;

#[repr(C)]
pub struct Slot<const WORDS: usize> {
    pub key: [u64; WORDS],
    pub len: u32,
    pub idx: u32,
}

impl<const WORDS: usize> Slot<WORDS> {
    pub const EMPTY: Self = Slot { key: [0; WORDS], len: u32::MAX, idx: 0 };

    /// The key index if `v` and `n` are this slot's key. Length and every word are
    /// compared in one combined expression, so neither becomes a separate branch.
    #[inline(always)]
    pub fn find(&self, v: &[u64; WORDS], n: usize) -> Option<usize> {
        // Clamped (branch-free) so even a 4 GiB input can't carry the empty-slot length.
        let n = n.min(MAX_KEY_LEN + 1);
        let mut diff = self.len as u64 ^ n as u64;
        for i in 0..WORDS {
            diff |= self.key[i] ^ v[i];
        }
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
            if self.len as usize == k.len() && keys[i].as_bytes() == k { Some(i) } else { None }
        } else {
            self.find(v, k.len())
        }
    }
}
