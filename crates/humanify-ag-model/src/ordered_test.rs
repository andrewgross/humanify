use crate::ordered::OrderedMap;

#[test]
fn keys_are_written_in_insertion_order_and_a_reset_keeps_its_place() {
    let mut m: OrderedMap<u64> = OrderedMap::new();
    m.set("zeta", 1u64);
    m.set("10", 2u64);
    m.set("alpha", 3u64);
    m.set("zeta", 4u64);
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"zeta":4,"10":2,"alpha":3}"#
    );
}

#[test]
fn spread_overwrites_in_place_and_appends_new_keys() {
    let mut a: OrderedMap<u64> = vec![("a".to_string(), 1), ("b".to_string(), 2)].into();
    a.spread(&vec![("b".to_string(), 5), ("c".to_string(), 6)].into());
    assert_eq!(serde_json::to_string(&a).unwrap(), r#"{"a":1,"b":5,"c":6}"#);
}
