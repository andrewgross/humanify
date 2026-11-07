use super::*;

#[test]
fn lone_surrogates_become_replacement_escapes_and_pairs_stay() {
    let cases = [
        (r#"["a\ud800b"]"#, r#"["a\ufffdb"]"#),
        (r#"["\udc00"]"#, r#"["\ufffd"]"#),
        (r#"["\ud83d\ude00"]"#, r#"["\ud83d\ude00"]"#),
        (r#"["\\ud800"]"#, r#"["\\ud800"]"#),
        (r#"["\ud800\u0041"]"#, r#"["\ufffd\u0041"]"#),
    ];
    for (input, want) in cases {
        assert_eq!(lone_surrogates_replaced(input), want, "{input}");
        let _: Value = serde_json::from_str(&lone_surrogates_replaced(input)).unwrap();
    }
}

#[test]
fn every_corpus_has_a_data_root() {
    let repo = crate::paths::repo_root();
    let roots = library::data_roots(&repo).unwrap();
    for c in library::CORPORA {
        assert!(roots.value.roots[c.name].starts_with('/'), "{}", c.name);
    }
}
