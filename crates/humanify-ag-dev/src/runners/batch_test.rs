use super::*;

#[test]
fn thousands_is_python_s() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(thousands(1234567), "1,234,567");
}

#[test]
fn tree_tsv_skips_the_run_scaffold() {
    let dir = std::env::temp_dir().join(format!("exp202-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/a")).expect("mkdir");
    std::fs::create_dir_all(dir.join(".humanify")).expect("mkdir");
    std::fs::write(dir.join("src/a/x.js"), "12345").expect("write");
    std::fs::write(dir.join("runtime.js"), "1").expect("write");
    std::fs::write(dir.join(".humanify/m.json"), "{}").expect("write");
    assert_eq!(tree_tsv(&dir), "runtime.js\t1\nsrc/a/x.js\t5\n");
    let before = dir.join("before");
    let after = dir.join("after");
    for (root, n) in [(&before, "1"), (&after, "22")] {
        std::fs::create_dir_all(root.join("bun-dev-x")).expect("mkdir");
        std::fs::write(
            root.join("bun-dev-x/tree.tsv"),
            format!("src/a.js\t{n}000\nvendor/_bun-modules.json\t9\nvendor/v.js\t3\n"),
        )
        .expect("write");
    }
    let table = trees(&before, &after, &["x".to_string()]).expect("table");
    assert!(
        table.ends_with("| x | 1 / 1,000 | 1 / 22,000 | 1 / 3 | 1 / 3 | 0 / 0 | 0 / 0 |\n"),
        "{table}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
