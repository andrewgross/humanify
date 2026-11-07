use super::*;

fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dev-vendor-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    for (rel, text) in files {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    d
}

/// A library body big enough to pair (>= 20 content shingles).
fn lib(word: &str, extra: &str) -> String {
    format!(
        "var e = require(\"./dep.js\");\nfunction t(a, b) {{ return a.{word} + b.size * 3 + \"one two three four five six\"; }}\nfunction u(c) {{ return c.alpha(c.beta, c.gamma, 1, 2, 3); }}\nmodule.exports = {{ t: t, u: u, k: \"{extra}\" }};\n"
    )
}

#[test]
fn buckets_by_content_not_path() {
    let p = tree(
        "p",
        &[
            ("same.js", "var a = 1; module.exports = a;\n"),
            ("renamed.js", "function f(x) { return x + 1; }\n"),
            ("moved.js", "module.exports = \"moved content here\";\n"),
            ("gone.js", "module.exports = 41;\n"),
        ],
    );
    let f = tree(
        "f",
        &[
            ("same.js", "var a = 1; module.exports = a;\n"),
            ("renamed.js", "function g(y) {\n  return y + 1;\n}\n"),
            ("elsewhere.js", "module.exports = \"moved content here\";\n"),
            ("new.js", "module.exports = 42;\n"),
        ],
    );
    let v = decompose(&p, &f, &Differ).unwrap();
    assert_eq!(v.identical.files, 1);
    assert_eq!(v.name_only.files, 1);
    assert_eq!(v.moved_path.files, 1);
    assert_eq!(v.truly_added.files, 1);
    assert_eq!(v.truly_removed.files, 1);
    assert_eq!(v.real_lines, v.truly_added.lines + v.truly_removed.lines);
    assert_eq!(v.total_lines, v.noise_lines + v.real_lines);
    let rows: usize = v.file_rows.iter().map(|r| r.lines).sum();
    assert_eq!(rows + v.manifest_changed_lines, v.total_lines);
}

#[test]
fn intra_tree_require_paths_are_layout_not_content() {
    let p = tree(
        "rp",
        &[(
            "a.js",
            "var d = require(\"./x-1.js\"); module.exports = d;\n",
        )],
    );
    let f = tree(
        "rf",
        &[(
            "a.js",
            "var d = require(\"./x-2.js\"); module.exports = d;\n",
        )],
    );
    let v = decompose(&p, &f, &Differ).unwrap();
    assert_eq!(v.name_only.files, 1);
    let p = tree(
        "bp",
        &[("a.js", "var d = require(\"https\"); module.exports = d;\n")],
    );
    let f = tree(
        "bf",
        &[("a.js", "var d = require(\"http2\"); module.exports = d;\n")],
    );
    let v = decompose(&p, &f, &Differ).unwrap();
    assert_eq!(v.real_change.files, 1);
}

#[test]
fn a_moved_and_slightly_changed_library_is_paired() {
    let p = tree("mp", &[("lib-a/m.js", &lib("width", "v1"))]);
    let f = tree("mf", &[("lib-b/m.js", &lib("width", "v2"))]);
    let v = decompose(&p, &f, &Differ).unwrap();
    assert_eq!(v.relocated_pairs.len(), 1, "{:?}", v.relocated_pairs);
    let pair = &v.relocated_pairs[0];
    assert_eq!(
        (pair.prior.as_str(), pair.fresh.as_str()),
        ("lib-a/m.js", "lib-b/m.js")
    );
    assert_eq!(pair.real_lines, 2);
    assert_eq!(v.real_change.lines, 2);
    assert_eq!(v.truly_added.files + v.truly_removed.files, 0);
}

#[test]
fn shingles_collapse_slots_and_split_strings() {
    let toks: Vec<String> = [
        "Program{",
        "$0",
        "$12",
        "L0",
        "S=\"a b\"",
        ";",
        "I=x",
        "N=1",
        "\"+\"",
        "true",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut c = Vec::new();
    for t in &toks {
        content_of(t, &mut c);
    }
    assert_eq!(
        c,
        [
            "Program{", "$", "$", "L", "S=", "w=a", "w=b", "I=x", "N=1", "\"+\""
        ]
    );
}
