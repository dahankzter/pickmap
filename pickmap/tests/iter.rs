use pickmap::__private::Iter;

#[test]
fn iter_in_order_and_exact_size() {
    static KEYS: [&str; 2] = ["a", "b"];
    static VALUES: [u8; 2] = [1, 2];
    let it = Iter::new(&KEYS, &VALUES);
    assert_eq!(it.len(), 2);
    assert_eq!(it.clone().collect::<Vec<_>>(), [("a", &1), ("b", &2)]);
    assert_eq!(it.rev().collect::<Vec<_>>(), [("b", &2), ("a", &1)]);
}
