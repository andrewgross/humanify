use super::*;

/// A bundle: one IIFE whose scope declares 50+ bindings.
fn bundle(body: &str) -> String {
    let filler: String = (0..50).map(|i| format!("var v{i} = {i};\n")).collect();
    format!("(function () {{\n{filler}{body}\n}})();\n")
}

#[test]
fn a_bundle_reads_its_wrapper_body_and_refuses_a_wrapperless_text() {
    let code = bundle("function greet(name, other) {\n  return name;\n}");
    let stmts = bundle_statements(&code, "t", true).unwrap();
    assert_eq!(stmts.len(), 51);
    let last = stmts.last().unwrap();
    assert_eq!(
        last.text,
        "function greet(name, other) {\n  return name;\n}"
    );
    assert_eq!(last.lines, 3);
    assert_eq!(last.declared, ["greet", "name", "other"]);
    let err = bundle_statements("var a = 1;\nvar b = 2;\n", "split.js", false).unwrap_err();
    assert!(err.contains("no wrapper IIFE"), "{err}");
}

#[test]
fn the_hash_masks_names_but_not_content() {
    let a = bundle("function f(a) { return a.x + 1; }");
    let b = bundle("function renamed(q) { return q.y + 1; }");
    let c = bundle("function f(a) { return a.x + 2; }");
    let h = |code: &str| {
        bundle_statements(code, "t", false)
            .unwrap()
            .pop()
            .unwrap()
            .hash
    };
    assert_eq!(h(&a), h(&b));
    assert_ne!(h(&a), h(&c));
}

#[test]
fn a_split_file_reads_its_program_body_with_both_hashes() {
    let arrow = file_statements(
        "const x = require(\"./a.js\");\nvar m = (0, ns.def)((a) => { a.k = 1; });\n",
        "a.js",
    )
    .unwrap();
    let func =
        file_statements("var m = (0, ns.def)(function (a) { a.k = 1; });\n", "b.js").unwrap();
    assert_eq!(arrow.len(), 2);
    assert!(arrow[0].is_require);
    assert!(!arrow[1].is_require);
    // The wrapper spelling: one statement modulo arrow <-> function.
    assert_ne!(arrow[1].hash, func[0].hash);
    assert_eq!(arrow[1].spelling_hash, func[0].spelling_hash);
}

#[test]
fn a_parse_error_is_fatal() {
    let err = file_statements("var = ;", "broken.js").unwrap_err();
    assert!(err.contains("broken.js: parse failed"), "{err}");
}
