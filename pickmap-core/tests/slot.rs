use pickmap_core::slot::Slot;

#[test]
fn find_matches_only_equal() {
    let s = Slot::<2> { key: [1, 2], len: 9, idx: 4 };
    assert_eq!(s.find(&[1, 2], 9), Some(4));
    assert_eq!(s.find(&[1, 2], 8), None);
    assert_eq!(s.find(&[1, 3], 9), None);
    assert_eq!(s.find(&[0, 2], 9), None);
}

#[test]
fn empty_never_matches() {
    for n in 0..=64 {
        assert_eq!(Slot::<1>::EMPTY.find(&[0], n), None);
    }
}

#[test]
fn find_long_compares_bytes() {
    // Literals, not leaked Strings: Miri reports leaks.
    let x = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let y = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
    assert_eq!((x.len(), y.len()), (40, 40));
    let keys = [x, y];
    let s = Slot::<4> { key: [0; 4], len: 40, idx: 1 };
    assert_eq!(s.find_long(&[0; 4], y.as_bytes(), &keys), Some(1));
    assert_eq!(s.find_long(&[0; 4], x.as_bytes(), &keys), None);
}

#[test]
fn empty_rejects_u32_max_length() {
    // An input of exactly u32::MAX bytes must not look like an empty slot.
    assert_eq!(Slot::<1>::EMPTY.find(&[0], u32::MAX as usize), None);
    assert_eq!(Slot::<2>::EMPTY.find(&[0, 0], u32::MAX as usize), None);
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
    assert_eq!(Slot::<4>::EMPTY.find(&[0; 4], 0), None);
    assert_eq!(Slot::<1>::EMPTY.find(&[u64::MAX], u32::MAX as usize), None);
}
