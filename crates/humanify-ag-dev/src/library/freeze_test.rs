use super::*;

/// truth.py `package_root` and corpus.py `pnpm_version` (recorded from Python).
#[test]
fn package_root_is_python_s() {
    let t = package_root;
    assert_eq!(
        t("../node_modules/a/b.js"),
        Some(("a".into(), "../node_modules/a".into(), "b.js".into()))
    );
    assert_eq!(
        t("node_modules/@s/p/x/y.js"),
        Some(("@s/p".into(), "node_modules/@s/p".into(), "x/y.js".into()))
    );
    assert_eq!(
        t("x/node_modules/@s"),
        Some(("@s".into(), "x/node_modules/@s".into(), String::new()))
    );
    assert_eq!(t("src/app.js"), None);
    assert_eq!(
        t("node_modules/a/node_modules/b/c.js"),
        Some((
            "b".into(),
            "node_modules/a/node_modules/b".into(),
            "c.js".into()
        ))
    );
}

#[test]
fn a_corpus_without_inputs_has_no_recipe() {
    assert!(plan("dev").is_err());
    assert!(plan("test").is_err());
}
