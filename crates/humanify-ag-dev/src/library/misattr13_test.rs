use super::*;

#[test]
fn long_strings_are_the_string_regex_s() {
    let body = br#"a="0123456789012345678901"; b='short'; c="has\"escape-and-more-chars"; d='abcdefghijklmnopqrstu'"#;
    assert_eq!(
        long_strings(body),
        // Python's finditer: the escaped quote opens the next match
        vec![
            b"0123456789012345678901".to_vec(),
            b"escape-and-more-chars".to_vec(),
            b"abcdefghijklmnopqrstu".to_vec()
        ]
    );
}

#[test]
fn majority_breaks_ties_by_the_lower_source() {
    let segs = [(0, 5), (10, 3), (20, 5)];
    let offs: Vec<i64> = segs.iter().map(|s| s.0).collect();
    // [0,30): source 5 holds 20 bytes, source 3 holds 10
    assert_eq!(majority(&segs, &offs, 0, 30), Some(5));
    // [5,15): 5 bytes each: the lower index wins
    assert_eq!(majority(&segs, &offs, 5, 15), Some(3));
}

#[test]
fn package_root_takes_the_innermost_install() {
    assert_eq!(
        package_root("/a/node_modules/@s/p/node_modules/q/x.js"),
        Some(("q".into(), "/a/node_modules/@s/p/node_modules/q".into()))
    );
    assert_eq!(
        package_root("/a/node_modules/@s/p/x.js"),
        Some(("@s/p".into(), "/a/node_modules/@s/p".into()))
    );
}
