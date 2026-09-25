//! The driver's red tests: each pins a regime the four oracle pairs never
//! fire (lesson 17) and a fixture regime of the WP4.6 gate exposed.

use oxc_allocator::Allocator;

use crate::naming::waves::generate::TextView;
use crate::naming::waves::graph_ext::build_naming_graph;

/// The naming graph's call sites per function session id, over `text`
/// parsed as the driver parses it (unambiguous).
fn call_sites(text: &str) -> Vec<(String, Vec<String>)> {
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, text, "input.js").expect("parses");
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = crate::prior::build_side_parts(
        &ingest,
        &json,
        "input.js",
        crate::graph::Eligibility::All,
        false,
    );
    let view = TextView::build(ingest.semantic());
    let ng = build_naming_graph(ingest.semantic(), &parts.graph, &view);
    parts
        .graph
        .functions
        .iter()
        .zip(ng.fn_call_sites)
        .map(|(f, s)| (f.session_id.clone(), s))
        .collect()
}

/// zustand (an ESM fixture): a call inside an `export const` arrow is a
/// call site of its callee — babel's statement parent is the export
/// declaration in the program body.
#[test]
fn a_call_inside_an_exported_arrow_records_its_call_site() {
    let text = "const impl = e => {\n  return e;\n};\nexport const make = e => e ? impl(e) : impl;\nconsole.log(import.meta.url);\n";
    let sites = call_sites(text);
    let impl_sites = &sites
        .iter()
        .find(|(id, _)| id == "input.js:1:13")
        .expect("impl row")
        .1;
    assert_eq!(
        impl_sites,
        &vec!["export const make=e=>e?impl(e):impl;".to_string()]
    );
}

/// Rename each (old, new) in its program-scope binding, then render.
fn rename_and_render(code: &str, renames: &[(&str, &str)]) -> String {
    use crate::naming::waves::render::render_program;
    use crate::rename::validated::test_support::with_semantic;
    use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
    use crate::trail::Anchor;
    with_semantic(code, true, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let program = state.view().program_scope();
        for (old, new) in renames {
            let attempt = state.attempt_validated_rename(
                RenameRequest {
                    scope: program,
                    old_name: old,
                    new_name: new,
                    expected: None,
                },
                TrailSpec::Untrailed { why: "test" },
            );
            assert!(attempt.applied, "{old} -> {new}: {:?}", attempt.reason);
        }
        render_program(semantic, &state)
    })
}

/// nanoid: babel's ImportSpecifier printer drops ` as local` once the
/// local's name equals the imported one (and an ExportSpecifier its
/// ` as exported`).
#[test]
fn an_aliased_specifier_renamed_to_its_other_side_prints_shorthand() {
    let out = rename_and_render(
        "import { a as b } from \"m\";\nconst c = 1;\nexport { c as d };\nuse(b);\n",
        &[("b", "a"), ("c", "d")],
    );
    assert_eq!(
        out,
        "import { a } from \"m\";\nconst d = 1;\nexport { d };\nuse(a);\n"
    );
}

/// zustand: babel's renamer SPLITS an `export const` the first time one of
/// its bindings is renamed — the declaration loses `export` and a
/// specifier list (the names at that moment, the renamed local updated)
/// follows it.
#[test]
fn renaming_an_export_const_binding_splits_the_declaration() {
    let out = rename_and_render(
        "export const a = 1, b = 2;\nuse(a, b);\n",
        &[("a", "first")],
    );
    assert_eq!(
        out,
        "const first = 1, b = 2;\nexport { first as a, b };\nuse(first, b);\n"
    );
}

/// `sanitizeLibraryName` (library-prefix-resolver.test.ts cases).
#[test]
fn library_names_sanitize_as_the_ts_does() {
    use super::library::sanitize_library_name;
    assert_eq!(sanitize_library_name("react-dom"), "react_dom");
    assert_eq!(sanitize_library_name("@babel/runtime"), "babel_runtime");
    assert_eq!(sanitize_library_name("lodash.debounce"), "lodash_debounce");
    assert_eq!(sanitize_library_name("3d-lib"), "_3d_lib");
    assert_eq!(sanitize_library_name("React"), "react");
}

/// Finding #34, fixed TS-first: an `export { x } from "m"` local names a
/// binding of ANOTHER module, so a correct rename that gives a local binding
/// the same name is still a pure rename (the nanoid fixture's first version).
#[test]
fn a_rename_onto_a_reexport_local_name_is_valid() {
    use crate::naming::driver::validate::{baseline_of, output_valid};
    let fresh = "import { urlAlphabet as a } from \"./url.js\";\nexport { urlAlphabet } from \"./url.js\";\nexport const f = () => a;\n";
    let renamed = "import { urlAlphabet } from \"./url.js\";\nexport { urlAlphabet } from \"./url.js\";\nexport const f = () => urlAlphabet;\n";
    let baseline = baseline_of(fresh).expect("fresh parses");
    assert!(output_valid(renamed, &baseline));
    // A real change to the re-export itself still fails.
    let changed = "import { urlAlphabet as a } from \"./url.js\";\nexport { otherName } from \"./url.js\";\nexport const f = () => a;\n";
    assert!(!output_valid(changed, &baseline));
}
