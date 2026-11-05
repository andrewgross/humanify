//! JS `Map` semantics — the TS insertion order the transfer stage reads.

use super::MatchMap;

fn keys(map: &MatchMap) -> Vec<&str> {
    map.iter_ordered().map(|(k, _)| k.as_str()).collect()
}

#[test]
fn set_appends_new_keys_in_insertion_order() {
    let mut map = MatchMap::new();
    map.insert("c".into(), "1".into());
    map.insert("a".into(), "2".into());
    map.insert("b".into(), "3".into());
    assert_eq!(keys(&map), ["c", "a", "b"]);
}

#[test]
fn set_on_an_existing_key_keeps_its_position() {
    let mut map = MatchMap::new();
    map.insert("x".into(), "1".into());
    map.insert("y".into(), "2".into());
    map.insert("x".into(), "3".into());
    assert_eq!(keys(&map), ["x", "y"]);
    assert_eq!(map.get("x").map(String::as_str), Some("3"));
}

#[test]
fn delete_then_set_moves_the_key_to_the_end() {
    let mut map = MatchMap::new();
    map.insert("x".into(), "1".into());
    map.insert("y".into(), "2".into());
    assert_eq!(map.remove("x").as_deref(), Some("1"));
    map.insert("x".into(), "4".into());
    assert_eq!(keys(&map), ["y", "x"]);
    assert_eq!(map.len(), 2);
}

#[test]
fn reads_go_through_the_lookup_table() {
    let map: MatchMap = [("p".to_string(), "f".to_string())].into_iter().collect();
    assert!(map.contains_key("p"));
    assert_eq!(map.values().count(), 1);
}
