use super::*;

fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dev-linechurn-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (rel, text) in files {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    d
}

#[test]
fn mask_keeps_properties_keys_and_keywords() {
    assert_eq!(
        mask_identifiers("  const userName = obj.field + other({ key: v });"),
        "const X = X.field + X({ key: X });"
    );
}

#[test]
fn const_field_keys() {
    assert_eq!(const_field_key("    VERSION: \"2.1.85\","), Some("VERSION"));
    assert_eq!(const_field_key("VERSION:"), None);
    assert_eq!(const_field_key("9X: 1"), None);
}

#[test]
fn build_constants_count_only_modified_stamps_in_two_or_more_files() {
    let p = tree(
        "bp",
        &[
            ("a.js", "x = {\n  VERSION: \"1.0.0\",\n  OTHER: 1,\n};\n"),
            ("b.js", "y = {\n  VERSION: \"1.0.0\",\n};\n"),
            ("c.js", "z = {\n  BUILD_TIME: \"t1\",\n};\n"),
        ],
    );
    let f = tree(
        "bf",
        &[
            ("a.js", "x = {\n  VERSION: \"1.0.1\",\n  OTHER: 2,\n};\n"),
            ("b.js", "y = {\n  VERSION: \"1.0.1\",\n};\n"),
            ("c.js", "z = {\n  BUILD_TIME: \"t2\",\n};\n"),
        ],
    );
    let c = build_constant_churn(&p, &f).unwrap();
    assert_eq!(c.lines, 4);
    assert_eq!(c.files, 2);
    assert_eq!(c.by_key, [("VERSION".to_string(), 4)]);
}

#[test]
fn name_only_lines_pair_masked_twins() {
    let p = tree("np", &[("a.js", "var alpha = f(1);\nobj.k = 2;\n")]);
    let f = tree("nf", &[("a.js", "var beta = f(1);\nobj.j = 2;\n")]);
    let c = name_only_churn(&p, &f).unwrap();
    assert_eq!(
        c,
        NameOnlyChurn {
            lines: 2,
            files: 1,
            changed_lines: 4
        }
    );
}
