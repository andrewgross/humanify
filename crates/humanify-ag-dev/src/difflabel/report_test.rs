use super::{hunks, surface};

#[test]
fn a_normal_diff_gives_each_hunk_its_lines() {
    let diff =
        "2c2\n< b\n---\n> B\n4a5,6\n> x\n> y\n8,9d9\n< p\n< q\n12,13c13\n< m\n< n\n---\n> o\n";
    let got = hunks(diff).unwrap();
    let want = [
        (vec![2], vec![2]),
        (vec![], vec![5, 6]),
        (vec![8, 9], vec![]),
        (vec![12, 13], vec![13]),
    ];
    assert_eq!(got, want);
}

#[test]
fn a_diff_header_that_is_not_one_is_an_error() {
    assert!(hunks("2x2\n").is_err());
}

#[test]
fn a_path_reads_as_its_surface() {
    assert_eq!(surface("src/a.js"), "src");
    assert_eq!(surface("vendor/x/index.js"), "vendor");
    assert_eq!(surface(".humanify/humanified.js"), "metadata");
    assert_eq!(surface("package.json"), "other");
}

fn write(root: &std::path::Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let at = root.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
}

#[test]
fn a_run_labels_every_changed_line_and_adds_up_to_the_diff() {
    let root = humanify_ag_io::scratch::temp_dir("difflabel-test").unwrap();
    let rows = |files: &str| crate::difflabel::layout_of_rows(&["index.js"], files);
    write(
        &root.join("in-a"),
        &[(
            "index.js",
            "function a(x){return x+1}function b(y){return y*2}",
        )],
    );
    write(
        &root.join("in-b"),
        &[(
            "index.js",
            "function q(x){return x+1}function r(y){return y*3}",
        )],
    );
    write(
        &root.join("a"),
        &[
            (
                "src/m.js",
                "function alpha(x) {\n  return x + 1;\n}\nfunction beta(y) {\n  return y * 2;\n}\n",
            ),
            (
                ".humanify/layout.json",
                &rows(r#"{"src/m.js":[[1,0,0],[4,0,25]]}"#),
            ),
        ],
    );
    write(
        &root.join("b"),
        &[
            (
                "src/m.js",
                "function alpha(value) {\n  return value + 1;\n}\nfunction beta(y) {\n  return y * 3;\n}\n",
            ),
            ("src/new.txt", "one\ntwo\n"),
            (
                ".humanify/layout.json",
                &format!("{}\n", rows(r#"{"src/m.js":[[1,0,0],[4,0,25]]}"#)),
            ),
        ],
    );
    let out = root.join("labels.json");
    let report = super::run(
        (&root.join("a"), &root.join("b")),
        (&root.join("in-a"), &root.join("in-b")),
        Some(&out),
    )
    .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let lines = std::fs::read_to_string(root.join("labels.lines.tsv")).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(lines.lines().count(), 10, "{lines}");
    assert!(lines.contains("-\tsrc/m.js\t1\trenamed\n"), "{lines}");
    assert!(lines.contains("+\tsrc/m.js\t5\tinput-changed\n"), "{lines}");
    assert!(
        report.contains("changed lines: 10 (diff -rN counts 10)"),
        "{report}"
    );
    let lines = |label: &str| json["labels"][label]["lines"].as_u64();
    assert_eq!(lines("renamed"), Some(4));
    assert_eq!(lines("input-changed"), Some(2));
    // The layout record's own two lines count like any file's.
    assert_eq!(lines("no-origin"), Some(4));
    assert_eq!(
        json["pairedInputFiles"],
        serde_json::json!([["index.js", "index.js"]])
    );
}
