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
// Site B: the vendored bodies (the unpack writes them; the naming era's
// statements never cover a moved module)
// ---------------------------------------------------------------------------

use crate::naming::restore::restore_body_text;

/// A body's statement print hash, keyed the same way the capture keyed it —
/// the wrapped parse is the pass's own, so the test plants the key it will
/// be looked up by.
fn body_whole_hash(body: &str, statement: usize) -> String {
    let allocator = oxc_allocator::Allocator::default();
    let wrapped = format!("function restoreWrap() {{\n{body}\n}}");
    let ingest = Ingest::parse(&allocator, &wrapped, "vendor.js");
    assert!(ingest.errors.is_empty(), "body must parse: {body:?}");
    let tables = SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    let statements = json["body"][0]["body"]["body"]
        .as_array()
        .expect("statements");
    statement_named_prints(&statements[statement], &tables)
        .whole
        .print
        .hash
}

#[test]
fn a_vendored_body_s_slots_come_from_the_index_s_answer() {
    let body = "var foo = function (bar) {\n  return bar * 2;\n};\nuse(foo);\n";
    let hash = body_whole_hash(body, 0);
    let allocator = oxc_allocator::Allocator::default();
    let wrapped = format!("function restoreWrap() {{\n{body}\n}}");
    let ingest = Ingest::parse(&allocator, &wrapped, "vendor.js");
    let tables = SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    let statements = json["body"][0]["body"]["body"]
        .as_array()
        .expect("statements");
    let prints = statement_named_prints(&statements[0], &tables);
    let slot = prints
        .whole
        .mapping
        .iter()
        .position(|(_, _, name)| name == "foo")
        .expect("foo is a slot") as u32;
    let mut restorations = std::collections::BTreeMap::new();
    restorations.insert(
        hash,
        row(RestoreAnswer::Single(vec![named(slot, "pLimit")])),
    );
    let out = restore_body_text(body, &restorations, None).text;
    assert!(
        out.contains("var pLimit ="),
        "the restored name ships: {out}"
    );
    assert!(!out.contains("foo"), "no crawl name left: {out}");
    // Everything else is verbatim — the body is never beautified.
    assert!(out.contains("bar * 2"), "untouched code stays: {out}");
}

#[test]
fn a_vendored_body_without_answers_is_verbatim() {
    let body = "var foo = function (bar) {\n  return bar * 2;\n};\n";
    assert_eq!(
        restore_body_text(body, &std::collections::BTreeMap::new(), None).text,
        body
    );
}

#[test]
fn a_vendored_body_s_prior_release_carries_its_names() {
    // The prior release shipped the same file RESTORED (or renamed); the
    // fresh body's identical statement carries the prior's spellings for
    // the slots the index does not answer.
    let body = "var foo = function (bar) {\n  return bar * 2;\n};\n";
    let prior_body = "var pLimit = function (factor) {\n  return factor * 2;\n};\n";
    // The bodies hash EQUAL only when the normalized statements match; a
    // renamed identifier slots blind (name-blind walk), so the prior's
    // names join by slot. But `pLimit` vs `foo` differ in the CONTENT
    // tokens? Identifiers are content tokens? The content tokens are
    // strings/templates/property names/free identifiers — locals are
    // slotted, so the two bodies hash equal.
    let hash = body_whole_hash(body, 0);
    let prior_hash = body_whole_hash(prior_body, 0);
    assert_eq!(hash, prior_hash, "the walk is name-blind for locals");
    // Slot 0 is `foo`/`pLimit` (the declared var), slot 1 the param.
    let mut restorations = std::collections::BTreeMap::new();
    restorations.insert(
        hash.clone(),
        row(RestoreAnswer::Single(vec![named(1, "factor")])),
    );
    let out = restore_body_text(body, &restorations, Some(prior_body)).text;
    // The index answered the param slot; the prior carried the var's name.
    assert!(out.contains("var pLimit = function (factor)"), "{out}");
}

#[test]
fn the_index_s_answer_outranks_the_prior_s_spelling() {
    let body = "var foo = 1;\n";
    let prior_body = "var stale = 1;\n";
    let hash = body_whole_hash(body, 0);
    let mut restorations = std::collections::BTreeMap::new();
    restorations.insert(hash, row(RestoreAnswer::Single(vec![named(0, "fresh")])));
    let out = restore_body_text(body, &restorations, Some(prior_body)).text;
    assert!(out.contains("var fresh = 1;"), "the index wins: {out}");
}
