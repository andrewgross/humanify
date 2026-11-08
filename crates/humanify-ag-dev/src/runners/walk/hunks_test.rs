use super::*;

#[test]
fn masking_keeps_properties_keys_and_keywords() {
    assert_eq!(
        mask_identifiers("  const a = b.c + d;"),
        "const X = X.c + X;"
    );
    assert_eq!(
        mask_identifiers("return { key: val, other : x }"),
        "return { key: X, other : X }"
    );
    assert_eq!(mask_identifiers("a. b(0x1f)"), "X. b(0X)");
}

fn hunk(lines: &[&str]) -> Hunk {
    Hunk {
        file: "f.js".into(),
        header: "@@ -1 +1 @@".into(),
        lines: lines.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn hunks_classify_as_the_walk_report_labels_them() {
    assert_eq!(classify(&hunk(&["+a();"])), Kind::Added);
    assert_eq!(classify(&hunk(&["-a();"])), Kind::Removed);
    assert_eq!(
        classify(&hunk(&["-a();", "-b();", "+b();", "+a();"])),
        Kind::Moved
    );
    assert_eq!(classify(&hunk(&["-foo(x);", "+bar(y);"])), Kind::NameOnly);
    assert_eq!(classify(&hunk(&["-foo(1);", "+foo(2);"])), Kind::Real);
}

#[test]
fn a_unified_diff_splits_into_hunks_by_file() {
    let d = "diff --git a/src/x.js b/src/x.js\nindex 1..2\n--- a/src/x.js\n+++ b/src/x.js\n@@ -1,2 +1,2 @@\n ctx\n-a();\n+b();\n\\ No newline at end of file\ndiff --git a/src/y.js b/src/y.js\n@@ -3 +3 @@\n-c\n+d\n";
    let h = parse_hunks(d);
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].file, "src/x.js");
    assert_eq!(h[0].lines, vec![" ctx", "-a();", "+b();"]);
    assert_eq!(h[1].file, "src/y.js");
}
