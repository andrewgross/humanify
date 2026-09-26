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
    let parts =
        crate::prior::build_side_parts(&ingest, &json, "input.js", crate::graph::Eligibility::All);
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

/// Attempt `<name>Renamed` for every program-scope binding, twice (a
/// second pass renames the already-renamed names again, as a later naming
/// pass or a prior carry would), then render. Rejections are ignored.
fn rename_every_binding_and_render(code: &str) -> String {
    use crate::naming::waves::render::render_program;
    use crate::rename::validated::test_support::with_semantic;
    use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
    use crate::trail::Anchor;
    with_semantic(code, true, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let program = state.view().program_scope();
        for pass in ["Renamed", "Again"] {
            for (old, _) in state.bindings_in(program) {
                let new = format!("{old}{pass}");
                let _ = state.attempt_validated_rename(
                    RenameRequest {
                        scope: program,
                        old_name: &old,
                        new_name: &new,
                        expected: None,
                    },
                    TrailSpec::Untrailed { why: "test" },
                );
            }
        }
        render_program(semantic, &state)
    })
}

/// Finding #16, its exact sequence: rename one `export const` binding,
/// then the other, then the first again. The TS (and the port until #55)
/// split the declaration on the first rename and renamed in place after,
/// printing `export { first as a, b }` over locals named `again`/`second`
/// — dead names, a module that no longer loads. Export names are never
/// renamed now, so the module prints as it came.
#[test]
fn export_const_renames_never_leave_a_specifier_on_a_dead_name() {
    use crate::naming::waves::render::render_program;
    use crate::rename::validated::test_support::with_semantic;
    use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
    use crate::trail::Anchor;
    let code = "export const a = 1, b = 2;\nuse(a, b);\n";
    let out = with_semantic(code, true, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let program = state.view().program_scope();
        for (old, new) in [("a", "first"), ("b", "second"), ("first", "again")] {
            let _ = state.attempt_validated_rename(
                RenameRequest {
                    scope: program,
                    old_name: old,
                    new_name: new,
                    expected: None,
                },
                TrailSpec::Untrailed { why: "test" },
            );
        }
        render_program(semantic, &state)
    });
    assert_eq!(out, code);
}

/// Finding #55: renaming every binding of a module that uses every export
/// form leaves every export NAME as it was — declarations keep their ids,
/// specifiers keep their external names, re-exports bind nothing — and the
/// multi-declarator `export const` (#16's zustand shape) is never split,
/// so no specifier is left pointing at a dead name.
#[test]
fn renaming_every_binding_keeps_every_export_name() {
    let code = "import { sep as a } from \"node:path\";\n\
                export { basename } from \"node:path\";\n\
                export * as posix from \"node:path\";\n\
                export function createStore(e) {\n  return e;\n}\n\
                export class Counter {}\n\
                export const version = 1, limit = 3;\n\
                const o = e => e + a;\n\
                function i(e) {\n  return [e, limit];\n}\n\
                export { o as join, i };\n\
                export default function s(e) {\n  return createStore(e ?? version);\n}\n";
    assert_eq!(
        rename_every_binding_and_render(code),
        "import { sep as aRenamedAgain } from \"node:path\";\n\
         export { basename } from \"node:path\";\n\
         export * as posix from \"node:path\";\n\
         export function createStore(e) {\n  return e;\n}\n\
         export class Counter {}\n\
         export const version = 1, limit = 3;\n\
         const oRenamedAgain = e => e + aRenamedAgain;\n\
         function iRenamedAgain(e) {\n  return [e, limit];\n}\n\
         export { oRenamedAgain as join, iRenamedAgain as i };\n\
         export default function sRenamedAgain(e) {\n  return createStore(e ?? version);\n}\n"
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

/// The pipeline reports WHICH invariant failed (plugin.ts: a parse
/// failure, else the structural signature, else the free-name/binding
/// measure) — the CLI's `ERROR:` blocks and the exit code read it.
#[test]
fn the_verdict_names_the_failed_invariant() {
    use crate::naming::driver::validate::{Verdict, baseline_of, verdict};
    let fresh = "var a = 1;\nconsole.log(a, x);\n";
    let b = baseline_of(fresh).expect("fresh parses");
    assert!(matches!(
        verdict("var count = 1;\nconsole.log(count, x);\n", &b),
        Verdict::Valid
    ));
    assert!(matches!(
        verdict("var a = 1;\nconsole.log(a, x;\n", &b),
        Verdict::ParseFailed
    ));
    assert!(matches!(
        verdict("var a = 2;\nconsole.log(a, x);\n", &b),
        Verdict::Structural
    ));
    // A capture changes the free-name set; the structural signature is
    // preferred when both fire (plugin.ts `structuralFailure ?? ...`).
    let captured = verdict("var x = 1;\nconsole.log(x, x);\n", &b);
    assert!(
        matches!(captured, Verdict::Structural | Verdict::Semantic { .. }),
        "{captured:?}"
    );
}

/// A provider naming every requested identifier `<id>Named`.
struct SuffixProvider;

impl humanify_model::llm::NameProvider for SuffixProvider {
    fn run_wave(
        &self,
        calls: Vec<humanify_model::llm::LlmCall>,
    ) -> Vec<Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                Ok(humanify_model::llm::BatchRenameResponse {
                    renames: humanify_model::llm::Renames::from_entries(
                        c.request
                            .identifiers
                            .iter()
                            .map(|i| (i.clone(), Some(format!("{i}Named")))),
                    ),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

fn ledger_config() -> super::NamingConfig {
    super::NamingConfig {
        bundler: None,
        minifier: None,
        skip_libraries: true,
        reconcile_prior_diff: true,
        naming_floor: true,
        naming_floor_sweep: true,
        source_map: false,
        emit_rename_ledger: true,
        family_permute_disabled: false,
        params: humanify_model::llm::CacheKeyParams {
            model: "m".into(),
            temperature: Some(0.0),
            max_tokens: None,
            reasoning_effort: None,
        },
        capture_dump: false,
        tunables: Default::default(),
        shingle_probe: false,
        fast: crate::fast::FastTier::Off,
    }
}

/// `--rename-ledger` (plugin.ts `buildRenameLedgerBundle`): the base stage
/// derives from the naming-era renames over the FRESH text, each
/// post-generate pass that applied a rename (the prior-diff reconcile, the
/// deferred sweep) adds a stage over the text IT renamed, and the whole
/// ledger replays onto the fresh text to the SHIPPED code — the TS's own
/// self-check.
#[test]
fn the_rename_ledger_replays_the_fresh_text_to_the_shipped_code() {
    use crate::rename::validated::ledger::apply_rename_ledger;
    let fresh =
        "function a(b) {\n  var c = b + 1;\n  return c;\n}\nvar d = a(2);\nconsole.log(d);\n";
    let prior = "function addOne(value) {\n  var result = value + 1;\n  return result;\n}\nvar total = addOne(2);\nconsole.log(total);\n";
    for prior in [None, Some(prior)] {
        let out = super::run_naming(
            &super::NamingInput {
                fresh,
                prior,
                library: None,
            },
            &ledger_config(),
            &SuffixProvider,
        )
        .expect("the stage runs");
        let bundle = out.rename_ledger.as_ref().expect("a ledger in ledger mode");
        assert_eq!(bundle.source, fresh);
        assert!(!bundle.ledger.entries.is_empty(), "the waves renamed");
        assert_eq!(
            bundle.stage_sources.len(),
            bundle.ledger.post.as_ref().map_or(0, Vec::len)
        );
        assert_eq!(
            apply_rename_ledger(fresh, &bundle.ledger).as_deref(),
            Ok(out.code.as_deref().expect("shipped")),
            "prior {}",
            prior.is_some()
        );
    }
    // Finding #49: a renamed shorthand property prints `key: name` — the
    // ledger records the printed form and pins the shipped text's hash.
    let shorthand =
        "function a(b) {\n  var c = b + 1;\n  return { c };\n}\nvar d = a(2);\nconsole.log(d);\n";
    let out = super::run_naming(
        &super::NamingInput {
            fresh: shorthand,
            prior: None,
            library: None,
        },
        &ledger_config(),
        &SuffixProvider,
    )
    .expect("the stage runs");
    let shipped = out.code.as_deref().expect("shipped");
    assert!(shipped.contains("{ c: cNamed }"), "{shipped}");
    let bundle = out.rename_ledger.as_ref().expect("a ledger");
    assert_eq!(
        bundle.ledger.output_sha256.as_deref(),
        Some(crate::rename::validated::ledger::sha256_hex(shipped).as_str())
    );
    assert_eq!(
        apply_rename_ledger(shorthand, &bundle.ledger).as_deref(),
        Ok(shipped)
    );
    // Without the flag there is no ledger.
    let mut config = ledger_config();
    config.emit_rename_ledger = false;
    let out = super::run_naming(
        &super::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &config,
        &SuffixProvider,
    )
    .expect("the stage runs");
    assert!(out.rename_ledger.is_none());
}

/// Finding #35: an `llm` trail row's `refCount` counts each reference and
/// write of the binding ONCE. With a prior (the two scope epochs) a
/// function's own-scope binding referenced inside a nested block used to
/// count those references twice (the TS's fresh-era re-registration).
#[test]
fn the_llm_ref_count_counts_each_reference_once() {
    let fresh = "function a(b) {\n  if (b) {\n    let q = b;\n    use(q, b);\n  }\n  b = 2;\n  return b;\n}\nuse(a);\n";
    let prior = "var unrelated = 1;\nconsole.log(unrelated);\n";
    for prior in [None, Some(prior)] {
        let out = super::run_naming(
            &super::NamingInput {
                fresh,
                prior,
                library: None,
            },
            &ledger_config(),
            &SuffixProvider,
        )
        .expect("the stage runs");
        let counts: Vec<Option<u32>> = out
            .trail
            .entries()
            .iter()
            .filter(|e| e.old_name == "b")
            .flat_map(|e| &e.attempts)
            .filter(|a| a.tier == crate::trail::Tier::Llm)
            .map(|a| a.ref_count)
            .collect();
        // Reads: `if (b)`, `q = b`, `use(q, b)`, `return b`; write: `b = 2`.
        assert_eq!(counts, [Some(5)], "prior {}", prior.is_some());
    }
}

/// Finding #41: a structural failure NAMES its first diverging token
/// (`describeStructuralDivergence`) — indented lines, the TS's shape.
#[test]
fn a_structural_divergence_is_localised() {
    use crate::naming::driver::validate::describe_structural_divergence;
    let fresh = "var a = 1;\nconsole.log(a);\n";
    assert_eq!(
        describe_structural_divergence(fresh, "var count = 1;\nconsole.log(count);\n"),
        None
    );
    let detail = describe_structural_divergence(fresh, "var a = 2;\nconsole.log(a);\n")
        .expect("a divergence");
    let lines: Vec<&str> = detail.lines().collect();
    assert_eq!(lines.len(), 5, "{detail}");
    assert!(
        lines[0].starts_with("  first divergence at token "),
        "{detail}"
    );
    assert!(lines[0].ends_with(" tokens each"), "{detail}");
    // The Rust serializer's literal token (its bytes are its own, 02 §4a).
    assert_eq!(lines[1], "    original: \"N=1;\"");
    assert_eq!(lines[2], "    output:   \"N=2;\"");
    assert!(
        lines[3].starts_with("    original context: ")
            && lines[4].starts_with("    output context:   ")
    );
    let longer = describe_structural_divergence(fresh, "var a = 1;\nconsole.log(a);\nfoo();\n")
        .expect("a divergence");
    assert!(longer.contains(" tokens before vs "), "{longer}");
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

/// Finding #56: a first-version run (no prior) names EVERY function, and
/// each function context's used-identifiers Set used to COPY the module
/// scope's names — functions x module names strings, 62 GB peak on the
/// smallest bundle and a crash on 2.1.182. The Sets share the scope
/// tables' snapshots, so what they hold grows with the input, not with
/// its square.
#[test]
fn a_cold_run_holds_the_module_names_once_not_once_per_function() {
    use std::fmt::Write;
    const TOP: usize = 400;
    const FUNCTIONS: usize = 300;
    let mut fresh = String::new();
    for i in 0..TOP {
        writeln!(fresh, "var t{i} = {i};").unwrap();
    }
    for i in 0..FUNCTIONS {
        writeln!(fresh, "function f{i}(p) {{\n  return p + t{};\n}}", i % TOP).unwrap();
    }
    writeln!(fresh, "console.log(f0, t0);").unwrap();
    let mut config = ledger_config();
    config.emit_rename_ledger = false;
    let out = super::run_naming(
        &super::NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &config,
        &SuffixProvider,
    )
    .expect("the stage runs");
    assert!(
        out.processor.completed_calls >= FUNCTIONS,
        "every function was asked ({})",
        out.processor.completed_calls
    );
    let held = out.waves.context_set_names;
    assert!(
        held < 10 * (TOP + FUNCTIONS),
        "the context Sets hold {held} names for {FUNCTIONS} functions over {TOP} module names"
    );
}
