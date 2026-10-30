use keysets::SplitMix64;
use pickmap_core::hash::{Pos, h_long, pick_byte, pthash_bucket};

#[test]
fn pick_byte_clamps() {
    assert_eq!(pick_byte(b"abc", Pos::Start(5)), b'c');
    assert_eq!(pick_byte(b"abc", Pos::End(0)), b'c');
    assert_eq!(pick_byte(b"abc", Pos::End(9)), b'a');
    assert_eq!(pick_byte(b"", Pos::Start(0)), 0);
}

#[test]
fn h_long_sensitive() {
    let mut rng = SplitMix64(2);
    let iters = if cfg!(miri) { 1 } else { 5 };
    for len in 0..=80 {
        for _ in 0..iters {
            let key: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            let base = h_long(&key, 7);
            for bit in 0..len * 8 {
                let mut k = key.clone();
                k[bit / 8] ^= 1 << (bit % 8);
                assert_ne!(h_long(&k, 7), base, "len {len} bit {bit}");
            }
        }
    }
    assert_ne!(h_long(b"a", 7), h_long(b"a\0", 7));
}

#[test]
fn pthash_bucket_zero_bits() {
    assert_eq!(pthash_bucket(u64::MAX, 0), 0);
    assert_eq!(pthash_bucket(u64::MAX, 3), 7);
}
