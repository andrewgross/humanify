use super::*;

/// 16-findings-queue #12: only OWN entries are read — an absent key named
/// after an Object.prototype member is absent, not the built-in function.
#[test]
fn own_values_win_and_absent_keys_are_absent() {
    let rec = StrMap(vec![
        ("a".into(), "alpha".into()),
        ("toString".into(), "own".into()),
        ("e".into(), String::new()),
    ]);
    assert_eq!(get(&rec, "a").as_deref(), Some("alpha"));
    assert_eq!(get(&rec, "toString").as_deref(), Some("own"));
    assert_eq!(get(&rec, "valueOf"), None);
    assert_eq!(get(&rec, "constructor"), None);
    assert_eq!(get(&rec, "missing"), None);
    assert_eq!(get(&rec, "e").as_deref(), Some(""));
    assert_eq!(get_truthy(&rec, "e"), None);
}
