use super::*;

#[test]
fn sha1_is_hashlib_s() {
    assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    let long = vec![b'a'; 1000];
    assert_eq!(sha1_hex(&long), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
}

#[test]
fn clean_keeps_searchable_tokens_only() {
    let common: HashSet<&str> = COMMON.split_whitespace().collect();
    let t = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let got = clean(
        vec![
            t("abcd"),
            t("ab1"),
            t("x\"yz"),
            t("length"),
            t("a1b2c3"),
            t("ok-name"),
            t("tab\there"),
        ],
        &common,
    );
    let mut got: Vec<String> = got
        .into_iter()
        .map(|b| String::from_utf8(b).expect("utf8"))
        .collect();
    got.sort();
    assert_eq!(got, vec!["abcd".to_string(), "ok-name".to_string()]);
}

#[test]
fn commonpath_is_os_path_s() {
    assert_eq!(commonpath(&["/a/b/c".into(), "/a/b/d".into()]), "/a/b");
    assert_eq!(commonpath(&["/a".into(), "/b".into()]), "/");
    assert_eq!(commonpath(&["/a/b".into()]), "/a/b");
}

#[test]
fn package_of_refuses_a_dot_entry() {
    assert_eq!(package_of("/x/node_modules/.bin/tool.js"), None);
    assert_eq!(
        package_of("/x/node_modules/@s/p/i.js"),
        Some(("@s/p".into(), "/x/node_modules/@s/p".into()))
    );
}
