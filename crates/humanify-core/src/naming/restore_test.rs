//! The restoration rule's unit tests (exp160, library-names plan step 7):
//! the index's answer names a claimed statement's slots, a slot the answer
//! does not cover falls back to the era's own naming, an ambiguous answer
//! abstains, conflicting prints abstain, and a binding the validated path
//! already settled keeps its name.

use oxc_semantic::Semantic;

use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::libindex::names::{NameKind, RestoredName};
use crate::libindex::table::{RestorationRow, RestoreAnswer};
use crate::libindex::units::statement_named_prints;
use crate::naming::module_names::{ClaimedRegion, ModuleSource};
use crate::naming::restore::{RestorationOutcome, apply_era_restoration};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Anchor, Tier};

/// One local slot's restored name.
fn named(slot: u32, name: &str) -> RestoredName {
    RestoredName {
        slot,
        name: name.into(),
        kind: NameKind::Local,
    }
}

/// A one-region claim over `statements`.
fn region(statements: &[usize]) -> Vec<ClaimedRegion> {
    vec![ClaimedRegion {
        statements: statements.to_vec(),
        claims: statements.iter().map(|_| None).collect(),
    }]
}

/// One captured answer keyed by the print hash the test computed locally —
/// in production the key comes from the index (exp159), here from the same
/// walk the application runs.
fn row(answer: RestoreAnswer) -> RestorationRow {
    RestorationRow {
        package: "dep".into(),
        answer,
    }
}

/// Parse a fixture, take its program body as the era's module source, and
/// hand the pieces to `f`: the semantic, the state, the module source and
/// the tables (the ingest outlives the closure).
fn with_fixture<R>(
    code: &str,
    f: impl FnOnce(&Semantic<'_>, &mut RenameState, &ModuleSource<'_>, &SymbolTables) -> R,
) -> R {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(
        ingest.errors.is_empty(),
        "fixture must parse: {:?}",
        ingest.errors
    );
    let tables = SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    let body = crate::cut::container_statement_values(&json, None).expect("program body");
    let source = ModuleSource {
        body,
        text: code,
        grammar: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
    };
    let mut state = RenameState::new(
        ingest.semantic(),
        Anchor::Fresh,
        crate::rename::name_profile::NameProfile::Bun,
    );
    f(ingest.semantic(), &mut state, &source, &tables)
}

/// The statement's whole-print hash, its functions' hashes, and the slot
/// of the mapping entry spelled `declares` (slot = the mapping's index —
/// the walk emits one entry per slot, in slot order).
fn facts(
    source: &ModuleSource<'_>,
    statement: usize,
    tables: &SymbolTables,
    declares: &str,
) -> (String, Vec<String>, u32) {
    let prints = statement_named_prints(&source.body[statement], tables);
    let slot = prints
        .whole
        .mapping
        .iter()
        .position(|(_, _, name)| name == declares)
        .unwrap_or_else(|| panic!("{declares} is not a slot of statement {statement}"))
        as u32;
    (
        prints.whole.print.hash,
        prints
            .functions
            .iter()
            .map(|f| f.print.hash.clone())
            .collect(),
        slot,
    )
}

#[test]
fn restoration_names_a_claimed_statement_from_the_index() {
    let code = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        let (hash, _, slot) = facts(source, 0, tables, "foo");
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(
            hash,
            row(RestoreAnswer::Single(vec![named(slot, "pLimit")])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome.bindings, 1, "the declared slot is restored");
        assert_eq!(outcome.statements, 1);
        assert_eq!(outcome.conflicts, 0);
        let (binding, name) = &outcome.applied[0];
        assert_eq!(name, "pLimit");
        assert_eq!(state.name_of(*binding), "pLimit");
        // The statement's span is protected for the post-generate passes.
        assert_eq!(outcome.spans.len(), 1);
    });
}

#[test]
fn a_missing_slot_falls_back_to_the_eras_own_naming() {
    let code = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        let (hash, _, foo_slot) = facts(source, 0, tables, "foo");
        // The answer names only `bar`'s slot — `foo`'s is missing.
        let bar_slot = {
            let prints = statement_named_prints(&source.body[0], tables);
            prints
                .whole
                .mapping
                .iter()
                .position(|(_, _, name)| name == "bar")
                .expect("bar is a slot") as u32
        };
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(
            hash,
            row(RestoreAnswer::Single(vec![named(bar_slot, "multiplier")])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome.bindings, 1);
        assert_eq!(outcome.applied[0].1, "multiplier");
        // foo keeps its crawl name — the era's own naming still owns it.
        let foo_prints = statement_named_prints(&source.body[0], tables);
        let (_, foo_symbol, _) = foo_prints.whole.mapping[foo_slot as usize];
        let foo_symbol = foo_symbol.expect("foo has a symbol");
        let foo = state
            .view()
            .binding_of_symbol(foo_symbol)
            .expect("foo binding");
        assert_eq!(state.name_of(foo), "foo");
        assert!(!outcome.applied.iter().any(|(b, _)| *b == foo));
    });
}

#[test]
fn an_ambiguous_answer_abstains() {
    let code = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        let (hash, _, slot) = facts(source, 0, tables, "foo");
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(
            hash,
            row(RestoreAnswer::Ambiguous(vec![
                vec![named(slot, "one")],
                vec![named(slot, "two")],
            ])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome.bindings, 0, "ambiguous: silence, never a guess");
        assert!(outcome.applied.is_empty());
        assert_eq!(outcome.spans.len(), 0);
        let prints = statement_named_prints(&source.body[0], tables);
        let (_, symbol, _) = prints.whole.mapping[slot as usize];
        let symbol = symbol.expect("foo has a symbol");
        let foo = state.view().binding_of_symbol(symbol).expect("foo binding");
        assert_eq!(state.name_of(foo), "foo");
    });
}

#[test]
fn conflicting_prints_of_one_statement_abstain() {
    // The whole print and the function print inside it both name `foo`'s
    // slot, differently — the index's own evidence disagrees.
    let code = "var foo = function () {\n  return foo;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        let (hash, functions, slot) = facts(source, 0, tables, "foo");
        let fn_hash = functions[0].clone();
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(hash, row(RestoreAnswer::Single(vec![named(slot, "alpha")])));
        restorations.insert(
            fn_hash,
            row(RestoreAnswer::Single(vec![named(slot, "beta")])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome.bindings, 0, "conflict: abstain");
        assert_eq!(outcome.conflicts, 1);
        assert!(outcome.applied.is_empty());
    });
}

#[test]
fn a_binding_the_validated_path_already_settled_keeps_its_name() {
    let code = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        // The prior-version transfer settled `foo` already.
        let prints = statement_named_prints(&source.body[0], tables);
        let (_, symbol, _) = prints.whole.mapping[0];
        let symbol = symbol.expect("foo has a symbol");
        let binding = state.view().binding_of_symbol(symbol).expect("foo binding");
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(binding),
                old_name: "foo",
                new_name: "carried",
                expected: Some(binding),
            },
            TrailSpec::settling(Tier::ExactMatch),
        );
        assert!(attempt.applied, "the transfer's rename applies");
        let (hash, _, slot) = facts(source, 0, tables, "foo");
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(
            hash,
            row(RestoreAnswer::Single(vec![named(slot, "pLimit")])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome.bindings, 0, "settled: the transfer's name stays");
        assert_eq!(state.name_of(binding), "carried");
    });
}

#[test]
fn an_unanswered_hash_is_silence() {
    let code = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    with_fixture(code, |semantic, state, source, tables| {
        let (_, _, slot) = facts(source, 0, tables, "foo");
        // A key nothing in this fixture hashes to.
        let mut restorations = std::collections::BTreeMap::new();
        restorations.insert(
            "0123456789abcdef0123456789abcdef".into(),
            row(RestoreAnswer::Single(vec![named(slot, "pLimit")])),
        );
        let outcome = apply_era_restoration(state, semantic, source, &region(&[0]), &restorations);
        assert_eq!(outcome, RestorationOutcome::default());
    });
}

#[test]
fn a_mappings_slots_are_only_locals() {
    // The walk slots LOCALS; free identifiers stay verbatim in the hash
    // and never enter the mapping — so the pass's None-symbol skip is a
    // defensive branch, and the index can never rename a global through
    // the mapping (its answers address slots, and slots are locals).
    let code = "var foo = 1;\nuse(foo);\nconsole.log(window, setTimeout);\n";
    with_fixture(code, |_semantic, _state, source, tables| {
        for (i, stmt) in source.body.iter().enumerate() {
            let prints = statement_named_prints(stmt, tables);
            for named in std::iter::once(&prints.whole).chain(&prints.functions) {
                assert!(
                    named.mapping.iter().all(|(_, symbol, _)| symbol.is_some()),
                    "statement {i} has a free slot: {:?}",
                    named.mapping
                );
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Site B: the vendored bodies' restoration, applied on the bundle
// (`naming::restore::apply_body_restoration`) — the capture's coordinates,
// carried into the bodies' slices by the edit list.
// ---------------------------------------------------------------------------

use crate::libdetect::discovery::RestorationApplication;
use crate::naming::restore::{RestorationEdits, apply_body_restoration};

/// A bundle-shaped fixture: the parsed semantic plus the span of the
/// binding declared as `declares` (the slot's symbol, as the capture
/// carried it).
fn with_bundle<R>(code: &str, f: impl FnOnce(&Semantic<'_>, &[(usize, usize)]) -> R) -> R {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(
        ingest.errors.is_empty(),
        "fixture must parse: {:?}",
        ingest.errors
    );
    f(ingest.semantic(), &[(0, code.len())])
}

/// The captured answer's application row for `declares`' binding: slot 0
/// (the test plants the ordinal the capture would carry), span = the
/// binding's own span (inside the module body).
fn application(
    semantic: &Semantic<'_>,
    declares: &str,
    answer: RestoreAnswer,
    modules: &[(usize, usize)],
) -> RestorationApplication {
    let scoping = semantic.scoping();
    let symbol = scoping
        .symbol_ids()
        .find(|&s| scoping.symbol_name(s) == declares)
        .unwrap_or_else(|| panic!("{declares} has no symbol"));
    let span = scoping.symbol_span(symbol);
    assert!(
        modules
            .iter()
            .any(|&(s, e)| s <= span.start as usize && span.end as usize <= e),
        "the fixture's binding must sit inside a module body"
    );
    RestorationApplication {
        hash: "captured".into(),
        package: "dep".into(),
        answer,
        start: span.start,
        end: span.end,
        slots: vec![Some(symbol.index() as u32)],
    }
}

/// The edits' effect on the bundle's text (the bodies' slices apply them).
fn applied_text(code: &str, edits: &RestorationEdits) -> String {
    let text_edits: Vec<crate::unpack::bun::TextEdit> = edits
        .edits
        .iter()
        .map(|e| crate::unpack::bun::TextEdit {
            start: e.start,
            end: e.end,
            replacement: e.replacement.clone(),
        })
        .collect();
    crate::unpack::bun::slice_with_edits(code, &text_edits, 0, code.len())
}

#[test]
fn a_vendored_body_s_slot_is_restored_on_the_bundle() {
    let code = "var m = helper(function () {\n  var foo = 1;\n  use(foo);\n});\n";
    with_bundle(code, |semantic, modules| {
        let app = application(
            semantic,
            "foo",
            RestoreAnswer::Single(vec![named(0, "pLimit")]),
            modules,
        );
        let edits = apply_body_restoration(semantic, &[app], modules);
        assert_eq!(edits.outcome.bindings, 1, "the slot is restored");
        assert_eq!(edits.files, 1, "one module's bytes changed");
        assert_eq!(edits.outcome.applied[0].1, "pLimit");
        let text = applied_text(code, &edits);
        assert!(text.contains("var pLimit = 1;"), "restored: {text}");
        assert!(!text.contains("foo"), "no crawl name left: {text}");
    });
}

#[test]
fn an_application_outside_the_extracted_bodies_is_not_applied() {
    // A container statement's answer is the era's (Site A): the unpack
    // applies only rows inside the bodies it slices out.
    let code = "var m = helper(function () {\n  var foo = 1;\n  use(foo);\n});\n";
    with_bundle(code, |semantic, _| {
        let mut app = application(
            semantic,
            "foo",
            RestoreAnswer::Single(vec![named(0, "pLimit")]),
            &[(0, code.len())],
        );
        // The row's span outside every extracted body — nothing applies.
        app.start = 0;
        app.end = 0;
        let edits = apply_body_restoration(semantic, &[app], &[(1, 2)]);
        assert_eq!(edits.outcome, RestorationOutcome::default());
        assert!(edits.edits.is_empty());
    });
}

#[test]
fn an_ambiguous_application_abstains() {
    let code = "var m = helper(function () {\n  var foo = 1;\n  use(foo);\n});\n";
    with_bundle(code, |semantic, modules| {
        let app = application(
            semantic,
            "foo",
            RestoreAnswer::Ambiguous(vec![vec![named(0, "one")], vec![named(0, "two")]]),
            modules,
        );
        let edits = apply_body_restoration(semantic, &[app], modules);
        assert_eq!(edits.outcome, RestorationOutcome::default());
        assert!(edits.edits.is_empty());
    });
}

#[test]
fn a_missing_slot_keeps_current_behaviour_on_the_bundle() {
    // The answer names a slot the row does not carry — nothing applies,
    // and nothing else in the body is touched.
    let code = "var m = helper(function () {\n  var foo = 1;\n  use(foo);\n});\n";
    with_bundle(code, |semantic, modules| {
        let app = application(
            semantic,
            "foo",
            RestoreAnswer::Single(vec![named(7, "pLimit")]),
            modules,
        );
        let edits = apply_body_restoration(semantic, &[app], modules);
        // The row was consulted (counted answered, like Site A's) but its
        // only name targets a slot this statement's mapping does not
        // carry: nothing applies.
        assert_eq!(
            edits.outcome,
            RestorationOutcome {
                statements: 1,
                ..RestorationOutcome::default()
            }
        );
        assert!(edits.edits.is_empty());
    });
}

#[test]
fn two_rows_naming_one_binding_differently_settle_on_the_first() {
    // The index's own two answers disagree for one binding (two units
    // sharing it): the first applied name wins — the validated path never
    // re-rolls a settled binding.
    let code = "var m = helper(function () {\n  var foo = 1;\n  use(foo);\n});\n";
    with_bundle(code, |semantic, modules| {
        let first = application(
            semantic,
            "foo",
            RestoreAnswer::Single(vec![named(0, "alpha")]),
            modules,
        );
        let mut second = first.clone();
        second.answer = RestoreAnswer::Single(vec![named(0, "beta")]);
        let edits = apply_body_restoration(semantic, &[first, second], modules);
        assert_eq!(edits.outcome.bindings, 1, "one binding, one name");
        assert_eq!(edits.outcome.applied[0].1, "alpha");
        let text = applied_text(code, &edits);
        assert!(text.contains("var alpha = 1;"), "first wins: {text}");
    });
}
