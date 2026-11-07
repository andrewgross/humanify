use super::*;

fn tally(prior: &str, fresh: &str) -> Tally {
    compose_file(prior, fresh, "f.js").unwrap()
}

#[test]
fn identical_files_charge_nothing() {
    assert_eq!(
        tally("var a = 1;\nf(a);\n", "var a = 1;\nf(a);\n"),
        Tally::default()
    );
}

#[test]
fn a_moved_statement_is_reorder() {
    let t = tally("var a = 1;\nvar b = 2;\n", "var b = 2;\nvar a = 1;\n");
    assert_eq!(t.reorder, 2);
    assert_eq!(t.real + t.naming, 0);
}

#[test]
fn a_rename_is_naming_and_a_require_rename_is_alias() {
    let t = tally(
        "const a = require(\"./x.js\");\nfunction f(p) {\n  return p;\n}\n",
        "const b = require(\"./x.js\");\nfunction g(q) {\n  return q;\n}\n",
    );
    assert_eq!(t.alias, 2);
    // `function f(p) {` and `return p;` both change: 2 lines each side.
    assert_eq!(t.naming, 4);
    assert_eq!(t.real, 0);
}

#[test]
fn an_edited_statement_is_charged_its_edited_lines() {
    let t = tally(
        "function compute(alpha, beta) {\n  const total = alpha + beta;\n  return total;\n}\n",
        "function compute(alpha, beta) {\n  const total = alpha * beta;\n  return total;\n}\n",
    );
    assert_eq!(t.real, 2);
}

#[test]
fn new_code_is_real_whole_mass_and_a_spelling_flip_is_reported_inside_real() {
    let t = tally("var a = 1;\n", "var a = 1;\nnewThing(1, 2);\n");
    assert_eq!(t.real, 1);
    let t = tally(
        "var m = (0, ns.def)(function (a) {\n  a.k = 1;\n});\n",
        "var m = (0, ns.def)((a) => {\n  a.k = 1;\n});\n",
    );
    // Different KPI hash, different masked head: real whole mass, both
    // sides; the same code modulo spelling, so reported as spelling.
    assert_eq!(t.real, 6);
    assert_eq!(t.spelling_identical_lines, 6);
}

#[test]
fn a_this_reading_arrow_is_not_a_spelling_flip() {
    let t = tally(
        "var m = (0, ns.def)(function (a) {\n  a.k = this;\n});\n",
        "var m = (0, ns.def)((a) => {\n  a.k = this;\n});\n",
    );
    assert_eq!(t.real, 6);
    assert_eq!(t.spelling_identical_lines, 0);
}
