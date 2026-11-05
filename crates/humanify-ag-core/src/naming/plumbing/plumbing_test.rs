//! The plumbing pass's precision edges: it names exactly one recognised
//! lazy-init helper, and declines (recorded) whatever it cannot name
//! safely. The driver tests hold the end-to-end behaviour
//! (`naming::driver::driver_test`, the lazy-init helper tests).

use oxc_allocator::Allocator;

use super::{LAZY_INIT_HELPER_NAME, PlumbingNames, lazy_init_helpers_of, name_lazy_init_helper};
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::RenameState;
use crate::toolchain::{BundleLayout, ModuleWrapperGrammar};

const HELPER: &str = "(e, t) => () => (e && (t = e(e = 0)), t)";

/// Run the pass over `text` as the fresh era sees it; the names it
/// produced and every module binding's final name.
fn run(text: &str) -> (PlumbingNames, Vec<String>) {
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, text, "input.js").expect("parses");
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = crate::prior::build_side_parts(
        &ingest,
        &json,
        "input.js",
        crate::graph::Eligibility::All,
        BundleLayout::SingleWrapperFunction,
    );
    let semantic = ingest.semantic();
    let wrapper = BundleLayout::SingleWrapperFunction.find_wrapper(ingest.program, semantic);
    let helpers = lazy_init_helpers_of(
        ModuleWrapperGrammar::BunAndEsbuild,
        &json,
        wrapper.map(|w| w.body_span),
    );
    let mut state = RenameState::new(
        semantic,
        crate::trail::Anchor::Fresh,
        crate::rename::name_profile::NameProfile::Bun,
    );
    let mut lifecycle = vec![Lifecycle::Pending; parts.graph.module_bindings.len()];
    let out = name_lazy_init_helper(&parts.graph, &helpers, &mut state, &mut lifecycle);
    let names = parts
        .graph
        .module_bindings
        .iter()
        .map(|b| {
            state
                .name_of_symbol(b.symbol)
                .unwrap_or(&b.name)
                .to_string()
        })
        .collect();
    (out, names)
}

#[test]
fn the_one_helper_is_named_and_settled() {
    let (out, names) = run(&format!("var b = {HELPER};\nvar o = b(() => 1);\no();\n"));
    assert_eq!(out.named, vec![("b".into(), LAZY_INIT_HELPER_NAME.into())]);
    assert!(names.contains(&LAZY_INIT_HELPER_NAME.to_string()));
}

/// Two bindings of the helper's shape: which one is THE helper is not
/// knowable, so neither is named (never seen in a real bundle).
#[test]
fn two_helpers_are_left_to_the_model() {
    let (out, names) = run(&format!(
        "var b = {HELPER};\nvar c = {HELPER};\nvar o = b(() => 1);\nvar p = c(() => 2);\no();\np();\n"
    ));
    assert!(out.named.is_empty(), "{out:?}");
    assert_eq!(out.declined.len(), 2);
    assert!(!names.contains(&LAZY_INIT_HELPER_NAME.to_string()));
}

/// The plumbing name is already held in the helper's scope: the validated
/// applier refuses, the helper is left to the model, and why is recorded.
#[test]
fn a_taken_plumbing_name_declines_with_the_reason() {
    let (out, _) = run(&format!(
        "var __esm = 1;\nvar b = {HELPER};\nvar o = b(() => __esm);\no();\n"
    ));
    assert!(out.named.is_empty(), "{out:?}");
    assert_eq!(
        out.declined,
        vec![("b".to_string(), "target-in-scope".to_string())]
    );
}

/// No helper in the text: nothing to do, nothing recorded.
#[test]
fn a_text_without_the_helper_is_untouched() {
    let (out, _) = run("var b = (e) => e;\nvar o = b(() => 1);\no();\n");
    assert_eq!(out, PlumbingNames::default());
}
