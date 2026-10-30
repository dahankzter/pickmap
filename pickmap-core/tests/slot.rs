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
