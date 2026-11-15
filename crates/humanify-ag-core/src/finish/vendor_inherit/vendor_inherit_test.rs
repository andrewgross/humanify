//! Ported from src/split/vendor-body-inherit.test.ts.
use super::{VendorBodyInheritor, same_program, skeleton};
use crate::prior_tree::PriorFiles;

const PRIOR_RENAMED: &str = "const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");
exports.f = __commonJS(function(iet, aQ) { var cX = iet.x + 1; return cX * aQ; });
";
const FRESH_RENAMED: &str = "const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");
exports.f = __commonJS(function(Jet, Kp) { var Lm = Jet.x + 1; return Lm * Kp; });
";

fn tree(files: &[(&str, &str)]) -> PriorFiles {
    PriorFiles::new(
        files
            .iter()
            .map(|(rel, body)| (rel.to_string(), body.as_bytes().to_vec()))
            .collect(),
    )
}

#[test]
fn reuses_the_prior_body_when_only_local_names_rerolled() {
    let prior = tree(&[("vendor/a.js", PRIOR_RENAMED)]);
    let mut inherit = VendorBodyInheritor::new(&prior);
    assert_eq!(
        inherit.bytes_for("vendor/a.js", FRESH_RENAMED.into()),
        PRIOR_RENAMED
    );
    assert_eq!(inherit.stats().inherited, 1);
    assert_eq!(inherit.stats().considered, 1);
}

#[test]
fn a_same_length_string_change_is_not_the_same_program() {
    // The case the blurred structuralHash cannot see.
    assert!(!same_program(
        "exports.f = function(q) { return fetch(\"https://a.example/v1\", q); };\n",
        "exports.f = function(z) { return fetch(\"https://b.example/v2\", z); };\n"
    ));
}

#[test]
fn a_number_change_inside_one_magnitude_bucket_is_not_the_same_program() {
    assert!(!same_program(
        "exports.f = function(q) { return setTimeout(q, 1000); };\n",
        "exports.f = function(z) { return setTimeout(z, 2000); };\n"
    ));
}

#[test]
fn a_changed_require_path_is_not_the_same_program() {
    assert!(!same_program(
        "const d = require(\"./lodash/lib_aaaa.js\");\nexports.f = function(q) { return d.f(q); };\n",
        "const y = require(\"./lodash/lib_aaaa-2.js\");\nexports.f = function(z) { return y.f(z); };\n"
    ));
}

#[test]
fn a_changed_property_name_is_not_the_same_program() {
    assert!(!same_program(
        "exports.f = function(q) { return q.readFileSync; };\n",
        "exports.f = function(z) { return z.writeFileSync; };\n"
    ));
}

#[test]
fn a_file_with_no_prior_counterpart_is_left_alone() {
    let prior = tree(&[]);
    let mut inherit = VendorBodyInheritor::new(&prior);
    assert_eq!(
        inherit.bytes_for("vendor/new.js", FRESH_RENAMED.into()),
        FRESH_RENAMED
    );
    assert_eq!(inherit.stats().considered, 0);
}

#[test]
fn a_byte_identical_prior_is_a_no_op() {
    let prior = tree(&[("vendor/a.js", PRIOR_RENAMED)]);
    let mut inherit = VendorBodyInheritor::new(&prior);
    assert_eq!(
        inherit.bytes_for("vendor/a.js", PRIOR_RENAMED.into()),
        PRIOR_RENAMED
    );
    assert_eq!(inherit.stats().inherited, 0);
    assert_eq!(inherit.stats().considered, 1);
}

#[test]
fn a_prior_that_fails_to_parse_is_never_inherited() {
    let prior = tree(&[("vendor/a.js", "this is ( not javascript\n")]);
    let mut inherit = VendorBodyInheritor::new(&prior);
    assert_eq!(
        inherit.bytes_for("vendor/a.js", FRESH_RENAMED.into()),
        FRESH_RENAMED
    );
}

#[test]
fn the_skeleton_drops_identifier_runs_only() {
    // The TS regex: /[A-Za-z_$][A-Za-z0-9_$]*/g — a digit run that does
    // not follow an identifier start survives.
    assert_eq!(skeleton("var a1 = 12 + b$_2.c;"), "  = 12 + .;");
    assert_eq!(skeleton("1e5 é x"), "1 é ");
}
