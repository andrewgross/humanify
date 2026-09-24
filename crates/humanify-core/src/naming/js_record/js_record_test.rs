use super::*;
use crate::naming::test_vectors;

#[test]
fn prototype_strings_equal_the_probed_ts_values() {
    let v = test_vectors();
    let probed = v["protoStrings"].as_object().unwrap();
    assert_eq!(probed.len(), OBJECT_PROTOTYPE_STRINGS.len());
    for (k, expected) in probed {
        assert_eq!(inherited_string(k), expected.as_str(), "{k}");
    }
}

#[test]
fn own_values_win_and_absent_keys_fall_through() {
    let rec = StrMap(vec![
        ("a".into(), "alpha".into()),
        ("toString".into(), "own".into()),
        ("e".into(), String::new()),
    ]);
    assert_eq!(get(&rec, "a").as_deref(), Some("alpha"));
    assert_eq!(get(&rec, "toString").as_deref(), Some("own"));
    assert_eq!(
        get(&rec, "valueOf").as_deref(),
        Some("function valueOf() { [native code] }")
    );
    assert_eq!(get(&rec, "missing"), None);
    assert_eq!(get(&rec, "e").as_deref(), Some(""));
    assert_eq!(get_truthy(&rec, "e"), None);
}
