use pickmap_core::slot::{MAX_KEY_LEN, Slot, meta};

#[test]
fn find_matches_only_equal() {
    let s = Slot::<2> { key: [1, 2], meta: 9, idx: 4 };
    assert_eq!(s.find(&[1, 2], 9), Some(4));
    assert_eq!(s.find(&[1, 2], 8), None);
    assert_eq!(s.find(&[1, 3], 9), None);
    assert_eq!(s.find(&[0, 2], 9), None);
    assert_eq!(s.find(&[1, 2], 9 + (1 << 32)), None);
}

#[test]
fn find_early_matches_only_equal() {
    let s = Slot::<2> { key: [1, 2], meta: 9, idx: 4 };
    assert_eq!(s.find_early(&[1, 2], 9), Some(4));
    assert_eq!(s.find_early(&[1, 2], 8), None);
    assert_eq!(s.find_early(&[1, 3], 9), None);
    assert_eq!(s.find_early(&[0, 2], 9), None);
    assert_eq!(s.find_early(&[1, 2], 9 + (1 << 32)), None);
    for n in 0..=64 {
        assert_eq!(Slot::<1>::EMPTY.find_early(&[0], n), None);
    }
}

#[test]
fn find_tagged_matches_only_equal() {
    let s = Slot::<2> { key: [1, 2], meta: meta(9, 7), idx: 4 };
    assert_eq!(s.find_tagged(&[1, 2], meta(9, 7)), Some(4));
    assert_eq!(s.find_tagged(&[1, 2], meta(8, 7)), None);
    assert_eq!(s.find_tagged(&[1, 2], meta(9, 6)), None);
    assert_eq!(s.find_tagged(&[1, 3], meta(9, 7)), None);
    assert_eq!(s.find_tagged(&[0, 2], meta(9, 7)), None);
}

#[test]
fn empty_never_matches() {
    for n in 0..=64 {
        assert_eq!(Slot::<1>::EMPTY.find(&[0], n), None);
        for tag in [0, 1, 255] {
            assert_eq!(Slot::<1>::EMPTY.find_tagged(&[0], meta(n, tag)), None);
        }
    }
}

#[test]
fn find_long_compares_bytes() {
    // Literals, not leaked Strings: Miri reports leaks.
    let x = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let y = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
    let keys = [x, y];
    let s = Slot::<4> { key: [0; 4], meta: 40, idx: 1 };
    assert_eq!(s.find_long(&[0; 4], y.as_bytes(), &keys), Some(1));
    assert_eq!(s.find_long(&[0; 4], x.as_bytes(), &keys), None);
    let t = Slot::<4> { key: [0; 4], meta: meta(40, 3), idx: 1 };
    assert_eq!(t.find_tagged_long(&[0; 4], meta(40, 3), y.as_bytes(), &keys), Some(1));
    assert_eq!(t.find_tagged_long(&[0; 4], meta(40, 3), x.as_bytes(), &keys), None);
}

#[test]
fn huge_lengths_never_wrap() {
    let s = Slot::<1> { key: [5], meta: 5, idx: 0 };
    let t = Slot::<1> { key: [5], meta: meta(5, 9), idx: 0 };
    for n in [MAX_KEY_LEN + 1, MAX_KEY_LEN + 6, (1 << 24) + 5, u32::MAX as usize + 6, usize::MAX] {
        assert_eq!(s.find(&[5], n), None, "{n}");
        assert_eq!(t.find_tagged(&[5], meta(n, 9)), None, "{n}");
    }
}

#[test]
fn empty_rejects_empty_input_values() {
    use pickmap_core::load::{BIG, HUGE, MID, SMALL, key_value, signature};
    // Every representation maps a zero-length input to all-zero words, which an
    // empty slot's key never equals.
    assert_eq!(key_value::<4, { SMALL | MID | BIG | HUGE }>(b""), [0; 4]);
    assert_eq!(key_value::<1, 0>(b""), [0]);
    assert_eq!(signature::<8, 4, 4>(b""), [0; 4]);
    assert_eq!(signature::<1, 3, 1>(b""), [0]);
    for tag in 0..=255 {
        assert_eq!(Slot::<4>::EMPTY.find_tagged(&[0; 4], meta(0, tag)), None);
    }
    assert_eq!(Slot::<4>::EMPTY.find(&[0; 4], 0), None);
}
