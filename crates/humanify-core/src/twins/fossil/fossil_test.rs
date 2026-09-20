//! Tests for the Bun fossil-module extraction ([`super`]) — the grammar is
//! OWNER-documented in `experiments/068-module-fossils/SPEC.md`; the
//! fixtures mirror TS `src/prior-version/statement-twin.test.ts`'s module
//! fixtures plus the two observed helper forms (bun's direct function,
//! esbuild's single-key object).
//!
//! The statement JSON + hashes come from the REAL inventory
//! ([`crate::twins::statement_inventory_with_values`]) — the same subtrees
//! whose hashes the gate tiers read.

use super::{extract_fossil_modules, module_signature};
use crate::twins::statement_inventory_with_values;

/// Inventory + hashes for a program-body fixture (no wrapper at these
/// sizes).
fn inventory_of(code: &str) -> (Vec<String>, Vec<serde_json::Value>) {
    let (inv, values) = statement_inventory_with_values(code, "fresh", None).expect("inventory");
    let hashes: Vec<String> = inv.statements.iter().map(|s| s.hash.clone()).collect();
    (hashes, values)
}

/// Bun's `__esm` helper shape: `(fn, res) => () => (fn, res)`.
const ESM_HELPER: &str = "var __esm = (fn, res) => () => (fn, res);";

#[test]
fn extracts_two_segments_with_their_declared_names() {
    let code = format!(
        "\
{helper}
var alphaValue;
var alphaInit = __esm(() => {{ alphaValue = readConfig(1); }});
var betaValue;
var betaSpare;
var betaInit = __esm(() => {{ betaValue = readConfig(2); betaSpare = 7; }});
",
        helper = ESM_HELPER
    );
    let (hashes, values) = inventory_of(&code);
    assert_eq!(values.len(), 6);
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert_eq!(ex.modules.len(), 2, "one module per init def");
    assert!(ex.eager_zone.is_empty());

    // Segment A: (prev+1)..=init — the FIRST segment swallows the helper.
    let a = &ex.modules[0];
    assert_eq!(a.statements, vec![0, 1, 2]);
    assert_eq!(a.init_index, 2);
    assert_eq!(a.declared, vec!["__esm", "alphaValue", "alphaInit"]);
    assert!(a.imports.is_empty(), "no leading init calls");
    assert_eq!(a.hashes.len(), 3);

    // Segment B.
    let b = &ex.modules[1];
    assert_eq!(b.statements, vec![3, 4, 5]);
    assert_eq!(b.init_index, 5);
    assert_eq!(b.declared, vec!["betaValue", "betaSpare", "betaInit"]);

    // The signatures are the SORTED hashes joined — and the two modules
    // differ (that uniqueness is what licenses the module tier's pairing).
    assert_ne!(module_signature(a), module_signature(b));
}

#[test]
fn a_later_inits_leading_zero_arg_calls_are_its_import_edges() {
    let code = format!(
        "\
{helper}
var aVal;
var aInit = __esm(() => {{ aVal = 1; }});
var bVal;
var bInit = __esm(() => {{ aInit(); bVal = 2; }});
var eagerThing = 9;
",
        helper = ESM_HELPER
    );
    let (hashes, values) = inventory_of(&code);
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert_eq!(ex.modules.len(), 2);
    // Module B imports module A by INDEX (the import edge).
    assert_eq!(ex.modules[1].imports, vec![0]);
    assert_eq!(ex.modules[0].imports, Vec::<usize>::new());
    // The trailing eager statement belongs to no segment.
    assert_eq!(ex.eager_zone, vec![5]);
    assert_eq!(ex.modules[1].statements, vec![3, 4]);
}

#[test]
fn the_esbuild_object_form_of_the_init_argument_extracts() {
    // esbuild passes an object with a single keyed method whose KEY IS THE
    // ORIGINAL SOURCE PATH; oxc's ESTree merges the method into an
    // ObjectProperty with a function value (the module doc's note).
    let code = format!(
        "\
{helper}
var objVal;
var objInit = __esm({{ \"src/a.ts\": () => {{ objVal = 3; }} }});
",
        helper = ESM_HELPER
    );
    let (hashes, values) = inventory_of(&code);
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert_eq!(ex.modules.len(), 1, "the object form is still an init def");
    assert_eq!(ex.modules[0].statements, vec![0, 1, 2]);
    assert_eq!(ex.modules[0].init_index, 2);
}

#[test]
fn the_helper_is_recognized_by_shape_not_name() {
    // The helper is whatever DECLARATOR matches the shape — a renamed
    // helper (post-minification) still detects; look-alikes do not.
    let code = "\
var zz = (fn, res) => () => (fn, res);
var notHelperOne = (fn) => fn(1);
var notHelperTwo = (fn, res) => (fn, res);
var notHelperThree = function (fn, res) { return () => (fn, res); };
var v0;
var v1 = zz(() => { v0 = 1; });
";
    let (hashes, values) = inventory_of(code);
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert_eq!(ex.modules.len(), 1, "only the zz init def counts");
    // Segments are CONTIGUOUS (prev+1..=init): the look-alike declarators
    // ride along in the segment — extraction doesn't judge them, the
    // signature match does (they'd break a same-version signature).
    assert_eq!(ex.modules[0].statements, vec![0, 1, 2, 3, 4, 5]);
    // The helper declaration itself is segment A's first statement (it was
    // swallowed by the v1 segment).
    assert_eq!(ex.eager_zone, Vec::<usize>::new());
}

#[test]
fn no_inits_means_an_all_eager_zone() {
    let code = "var a = 1;\nvar b = a + 1;\n";
    let (hashes, values) = inventory_of(code);
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert!(ex.modules.is_empty());
    assert_eq!(ex.eager_zone, vec![0, 1]);
}

#[test]
fn a_hash_count_mismatch_fails_loud() {
    let (hashes, values) = inventory_of("var a = 1;\nvar b = 2;\n");
    let err = extract_fossil_modules(&values[..1], &hashes).expect_err("the lengths must agree");
    assert!(
        err.contains("hashes for"),
        "the error names the mismatch: {err}"
    );
}

#[test]
fn twin_modules_share_a_signature_and_are_left_unpaired_upstream() {
    // Two byte-identical segments (after the seed module absorbs the
    // helper) — the signatures collide on each side, which is what the
    // gate's module_scoped_ambiguous counter reads. The EXTRACTION only
    // reports; the uniqueness decision lives in the gate.
    let twin = |a: &str, b: &str| {
        format!(
            "\
{helper}
var seedValue;
var seedInit = __esm(() => {{ seedValue = boot(9); }});
var {a};
var {a}Init = __esm(() => {{ {a} = readConfig(1); }});
var {b};
var {b}Init = __esm(() => {{ {b} = readConfig(1); }});
",
            helper = ESM_HELPER
        )
    };
    let (hashes, values) = inventory_of(&twin("z0", "z2"));
    let ex = extract_fossil_modules(&values, &hashes).expect("extract");
    assert_eq!(ex.modules.len(), 3, "seed + the two twins");
    let (seed, t1, t2) = (
        module_signature(&ex.modules[0]),
        module_signature(&ex.modules[1]),
        module_signature(&ex.modules[2]),
    );
    assert_eq!(t1, t2, "the twins are byte-identical in structure");
    assert_ne!(seed, t1, "the seed carries the helper + boot(9)");
}
