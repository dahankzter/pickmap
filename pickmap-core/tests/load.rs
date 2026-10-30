use keysets::SplitMix64;
use pickmap_core::load::{BIG, HUGE, MID, SMALL, classes_for, key_value, key_value_with, signature, signature_with};

const ITERS: usize = if cfg!(miri) { 2 } else { 200 };

fn random_bytes(rng: &mut SplitMix64, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.next() as u8).collect()
}

/// Changing any byte of a `len`-byte input changes `f`'s value.
fn exact_check<const N: usize>(lens: std::ops::RangeInclusive<usize>, f: impl Fn(&[u8]) -> [u64; N], what: &str) {
    let mut rng = SplitMix64(4);
    for len in lens {
        for _ in 0..ITERS {
            let a = random_bytes(&mut rng, len);
            let v = f(&a);
            for i in 0..len {
                let mut b = a.clone();
                b[i] ^= 0x5a;
                assert_ne!(f(&b), v, "{what} len={len} byte {i}");
            }
        }
    }
}

const ALL3: u8 = SMALL | MID | BIG;
const ALL4: u8 = SMALL | MID | BIG | HUGE;

#[test]
fn key_value_is_exact() {
    exact_check(0..=16, key_value::<2, ALL3>, "classes 2");
    exact_check(0..=32, key_value::<4, ALL4>, "classes 4");
    exact_check(0..=8, key_value::<1, ALL3>, "classes 1");
    exact_check(4..=7, key_value::<2, MID>, "MID only");
    exact_check(8..=16, key_value::<2, BIG>, "BIG only");
}

#[test]
fn signature_is_exact() {
    exact_check(2..=8, signature::<2, 4, 1>, "sig 2x4 in 1 word");
    exact_check(2..=8, signature::<2, 4, 2>, "sig 2x4 in 2 words");
    exact_check(4..=12, signature::<4, 3, 2>, "sig 4x3");
    exact_check(2..=28, signature::<2, 14, 4>, "sig 2x14");
    exact_check(8..=24, signature::<8, 3, 4>, "sig 8x3");
}

#[test]
fn split_value_is_exact() {
    use pickmap_core::load::split_value;
    exact_check(0..=32, split_value::<4, 4, { SMALL | MID }>, "split 4x8 + small/mid");
    exact_check(0..=16, split_value::<2, 2, { SMALL | MID }>, "split 2x8 + small/mid");
    exact_check(4..=24, split_value::<4, 3, MID>, "split 3x8 + mid");
}

#[test]
fn with_forms_match_const_forms() {
    let mut rng = SplitMix64(5);
    for len in 0..=40 {
        let k = random_bytes(&mut rng, len);
        assert_eq!(key_value_with::<4>(&k, ALL4), key_value::<4, ALL4>(&k), "len {len}");
        assert_eq!(key_value_with::<2>(&k, ALL3), key_value::<2, ALL3>(&k), "len {len}");
        assert_eq!(signature_with::<1>(&k, 2, 4), signature::<2, 4, 1>(&k), "len {len}");
        assert_eq!(signature_with::<2>(&k, 4, 3), signature::<4, 3, 2>(&k), "len {len}");
    }
}

/// Exact-length allocations, so Miri reports any read past the input.
#[test]
fn miri_all_lengths() {
    let mut rng = SplitMix64(6);
    for len in 0..=40 {
        let k = random_bytes(&mut rng, len).into_boxed_slice();
        std::hint::black_box(key_value::<1, ALL3>(&k));
        std::hint::black_box(key_value::<2, ALL3>(&k));
        std::hint::black_box(key_value::<4, ALL4>(&k));
        std::hint::black_box(signature::<2, 4, 1>(&k));
        std::hint::black_box(signature::<4, 3, 2>(&k));
        std::hint::black_box(signature::<8, 4, 4>(&k));
        std::hint::black_box(pickmap_core::load::split_value::<4, 4, { SMALL | MID }>(&k));
        for pos in [pickmap_core::hash::Pos::Start(7), pickmap_core::hash::Pos::End(7)] {
            std::hint::black_box(pickmap_core::hash::pick_byte(&k, pos));
        }
        std::hint::black_box(pickmap_core::hash::h_long(&k, 1));
    }
}

#[test]
fn classes_for_counts() {
    assert_eq!(classes_for([2, 5, 9, 20].into_iter()), (15, 11));
    assert_eq!(classes_for([4, 12].into_iter()), (MID | BIG, 4));
}
