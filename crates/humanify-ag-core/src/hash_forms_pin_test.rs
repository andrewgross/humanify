//! Behaviour pin for the hash FORMS (2026-10-13, the matching-unification
//! refactor, step 2: every "is this the same code?" canonical form becomes a
//! named, versioned setting of one serializer — docs/matching-architecture.md
//! §3.2, §7).
//!
//! Step 2 moves no byte. These digests were recorded on main @ 587e211f,
//! BEFORE the forms became settings, and each hashes one form's output over
//! every committed e2e fixture bundle (both versions), through the form's
//! real callers wherever one is reachable: the function rows' match key and
//! placeholder slots and the module bindings' identity key (`graph`), the
//! factory records' structural hash and the vendor content keys
//! (`modules`), the library index's unit prints (`libindex::units`), the
//! naming validator's divergence report, the whole-file signature
//! (`finish::vendor_inherit`), the split's statement hash, and the
//! statement-align walk's unit hashes, placeholder tables and content
//! shingles. The direct serializer calls cover every unit-shaped node under
//! every canonical-walk form, so a form whose settings drift changes a
//! digest even where no caller's sample happens to reach the difference.
//!
//! A digest change is never "update the constant": it means a form stopped
//! answering the way it did — a deliberate change needs a new form VERSION
//! (and an eval), not a new pin.

use std::collections::HashSet;

use std::fmt::Write as _;

use oxc_allocator::Allocator;
use serde_json::Value;

use crate::hash::forms::{IDENTITY_KEY, MATCH_KEY, MATCH_KEY_PRIVATES_BLINDED, RENAME_SIGNATURE};
use crate::hash::serialize::{SymbolTables, canonical_serialize};
use crate::ingest::Ingest;
use crate::matching_helpers_pin_test::{FUNCTION_TYPES, digest, fixture_pairs, with_side};

/// Every statement, declaration and unit-shaped expression of a program, in
/// pre-order: what the statement hash, the twin gates, the statement
/// context and the align walk see as pieces.
fn pieces(root: &Value) -> Vec<&Value> {
    fn walk<'v>(v: &'v Value, out: &mut Vec<&'v Value>) {
        match v {
            Value::Object(map) => {
                if let Some(t) = map.get("type").and_then(Value::as_str)
                    && (t.ends_with("Statement")
                        || t.ends_with("Declaration")
                        || FUNCTION_TYPES.contains(&t)
                        || matches!(
                            t,
                            "ObjectExpression"
                                | "ClassExpression"
                                | "TemplateLiteral"
                                | "VariableDeclarator"
                                | "SwitchCase"
                        ))
                {
                    out.push(v);
                }
                map.values().for_each(|c| walk(c, out));
            }
            Value::Array(items) => items.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn mapping_digest(mapping: &[(String, Option<oxc_semantic::SymbolId>, String)]) -> String {
    let rows: Vec<String> = mapping
        .iter()
        .map(|(slot, sym, name)| format!("{slot}={sym:?}={name}"))
        .collect();
    digest(&rows.join("\u{1}"))
}

/// Run one section over every fixture side and assert its digest; a red
/// pin also writes the section's lines to `<temp dir>/humanify-pin-<section>.txt`
/// (to diff against a base run's). The core never reads the environment
/// (`env_guard`), so the test has no switches: the constants below were
/// recorded from the failure message of an empty pin.
fn pin(section: &str, expected: &str, mut lines: impl FnMut(&str, &mut String)) {
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(out, "# {name}");
        lines(&a, &mut out);
        lines(&b, &mut out);
    }
    let got = digest(&out);
    if got != expected {
        // The lines behind the digest, to diff against a base run's.
        let _ = std::fs::write(
            std::env::temp_dir().join(format!("humanify-pin-{section}.txt")),
            &out,
        );
    }
    assert_eq!(got, expected, "{section}");
}

// ---------------------------------------------------------------------------
// The canonical walk's forms, called directly on every piece
// ---------------------------------------------------------------------------

fn canonical_lines(text: &str, out: &mut String) {
    with_side(text, |_, tables, json| {
        for piece in pieces(json) {
            let mk = canonical_serialize(piece, tables, &MATCH_KEY);
            let vb = canonical_serialize(piece, tables, &IDENTITY_KEY);
            let mkb = canonical_serialize(piece, tables, &MATCH_KEY_PRIVATES_BLINDED);
            let vbb = canonical_serialize(piece, tables, &RENAME_SIGNATURE);
            let _ = writeln!(
                out,
                "{} {} {} {} {} {} {}",
                mk.hash,
                vb.hash,
                mkb.hash,
                vbb.hash,
                mapping_digest(&mk.mapping),
                mapping_digest(&vb.mapping),
                digest(&vbb.parts)
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The split's statement hash (its own masked walk)
// ---------------------------------------------------------------------------

fn statement_lines(text: &str, out: &mut String) {
    with_side(text, |_, _, json| {
        for piece in pieces(json) {
            let _ = writeln!(
                out,
                "{}",
                crate::hash::statement_hash::statement_hash(piece)
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The statement-align walk (babel field order, its own token stream)
// ---------------------------------------------------------------------------

fn align_lines(text: &str, out: &mut String) {
    with_side(text, |_, tables, json| {
        for piece in pieces(json) {
            let (blurred, masked, kept, mapping) =
                crate::matching::statement_align::unit_forms_for_pin(piece, tables);
            let shingles = crate::matching::statement_align::content_shingles(tables, piece);
            let joined: Vec<&str> = shingles.iter().map(String::as_str).collect();
            let _ = writeln!(
                out,
                "{blurred} {masked} {kept} {} {}",
                mapping_digest(&mapping),
                digest(&joined.join("\u{1}"))
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The forms through their pipeline callers
// ---------------------------------------------------------------------------

fn graph_lines(text: &str, out: &mut String) {
    with_side(text, |ingest, _, _| {
        let graph = crate::graph::build_unified_graph(
            ingest.semantic(),
            ingest.program,
            "input.js",
            crate::rename::eligibility::NeverRename::UNIVERSAL,
            crate::toolchain::BundleLayout::SingleWrapperFunction,
        );
        for f in &graph.functions {
            let _ = writeln!(
                out,
                "F {} {:?} {:?}",
                f.structural_hash, f.placeholder_bindings, f.placeholder_id_spans
            );
        }
        for b in &graph.module_bindings {
            let _ = writeln!(out, "B {} {:?}", b.name, b.fingerprint_hash);
        }
    });
}

/// The run's toolchain for one bundle (detection, no overrides).
fn toolchain_of(code: &str) -> crate::toolchain::Toolchain {
    crate::toolchain::resolve_toolchain(&crate::detect::detect_bundle(code), None, None)
}

fn modules_lines(text: &str, out: &mut String) {
    let tc = toolchain_of(text);
    // The factory records' structural hash (the vendor manifest's
    // `structuralHash`, stamped with the MATCH_KEY form's version).
    let alloc = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&alloc, text);
    let semantic = ingest.semantic();
    let tables = SymbolTables::build(semantic);
    let wrapper = tc.layout.piece.find_wrapper(ingest.program, semantic);
    let body = wrapper.as_ref().map(|w| w.body_span);
    if let Some(c) =
        tc.module_wrappers
            .piece
            .classify_factories(text, ingest.program, semantic, body, &tables)
    {
        for f in &c.factories {
            let _ = writeln!(out, "R {} {}", f.factory_var, f.structural_hash);
        }
    }
    // The vendor content key of every module (the library index's
    // whole-module print).
    for k in crate::modules::vendor_content::bundle_content_keys(
        text,
        tc.module_wrappers.piece,
        tc.layout.piece,
    ) {
        let _ = writeln!(out, "K {:?} {:?}", k.body_span, k.key);
    }
    // The content key over EVERY function's text, against no declared
    // names and against a fixed one (the form, past the factories).
    let json = crate::ingest::program_estree_json(ingest.program);
    let mut fns = Vec::new();
    crate::matching_helpers_pin_test::nodes_of(&json, &FUNCTION_TYPES, &mut fns);
    let declared: HashSet<String> = ["e", "t", "require"].map(String::from).into();
    for f in fns {
        let at = |k: &str| f.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
        let Some(src) = text.get(at("start")..at("end")) else {
            continue;
        };
        let _ = writeln!(
            out,
            "V {:?} {:?}",
            crate::modules::vendor_content::vendor_content_key(src, &HashSet::new()),
            crate::modules::vendor_content::vendor_content_key(src, &declared)
        );
    }
}

fn units_lines(text: &str, out: &mut String) {
    let tc = toolchain_of(text);
    let alloc = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&alloc, text);
    for (unit, prints) in crate::libindex::units::bundle_unit_prints(
        text,
        &ingest,
        tc.layout.piece,
        tc.module_wrappers.piece,
        tc.interop.piece,
    ) {
        let _ = writeln!(out, "{:?} {prints:?}", unit.span);
    }
}

#[test]
fn whole_file_identity_forms_are_pinned() {
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(
            out,
            "# {name} {:?} {:?} {:?} {:?}",
            crate::finish::vendor_inherit::file_signature(&a),
            crate::finish::vendor_inherit::file_signature(&b),
            crate::naming::driver::validate::describe_structural_divergence(&a, &b),
            crate::naming::driver::validate::describe_structural_divergence(&a, &a),
        );
    }
    assert_eq!(digest(&out), PIN_FILES, "files");
}

#[test]
fn canonical_walk_forms_are_pinned() {
    pin("canonical", PIN_CANONICAL, canonical_lines);
}

#[test]
fn statement_hash_form_is_pinned() {
    pin("statement", PIN_STATEMENT, statement_lines);
}

#[test]
fn align_walk_forms_are_pinned() {
    pin("align", PIN_ALIGN, align_lines);
}

#[test]
fn graph_function_and_binding_forms_are_pinned() {
    pin("graph", PIN_GRAPH, graph_lines);
}

#[test]
fn module_forms_are_pinned() {
    pin("modules", PIN_MODULES, modules_lines);
}

#[test]
fn library_unit_forms_are_pinned() {
    pin("units", PIN_UNITS, units_lines);
}

type Section = fn(&str, &mut String);

/// The one-off FULL-corpus run (docs/matching-architecture.md §7, step 2):
/// every section over every `.js` file under each directory (or file) named
/// one per line in `<temp dir>/humanify-pin-corpus.txt` (e.g. a humanified release
/// tree — the RAW release bundle carries lone-surrogate escapes its JSON
/// round trip refuses), one digest line per file and section, then one
/// total per section. Run on the base commit and on the refactor and diff
/// the two outputs.
#[test]
#[ignore = "one-off: needs <temp dir>/humanify-pin-corpus.txt (real release trees)"]
fn full_corpus_digests() {
    let list = std::env::temp_dir().join("humanify-pin-corpus.txt");
    let corpus = std::fs::read_to_string(&list).expect("the corpus list file");
    let sections: [(&str, Section); 6] = [
        ("canonical", canonical_lines as Section),
        ("statement", statement_lines as Section),
        ("align", align_lines as Section),
        ("graph", graph_lines as Section),
        ("modules", modules_lines as Section),
        ("units", units_lines as Section),
    ];
    let mut files = Vec::new();
    for root in corpus.lines().filter(|l| !l.trim().is_empty()) {
        js_files(std::path::Path::new(root), &mut files);
    }
    files.sort();
    let mut totals: Vec<String> = vec![String::new(); sections.len()];
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (i, (section, lines)) in sections.iter().enumerate() {
            let mut out = String::new();
            lines(&text, &mut out);
            let d = digest(&out);
            println!("CORPUS {} {section} {d} {}", path.display(), out.len());
            totals[i].push_str(&d);
        }
    }
    for ((section, _), all) in sections.iter().zip(&totals) {
        println!("TOTAL {section} {} over {} files", digest(all), files.len());
    }
}

fn js_files(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            js_files(&entry.path(), out);
        }
    } else if path.extension().is_some_and(|e| e == "js") {
        out.push(path.to_path_buf());
    }
}

/// The persisted version stamps: an index manifest, a vendor manifest and
/// a split ledger are keyed by them, so step 2 may move where they live
/// but never their values. A deliberate era change moves one here (units/3:
/// the unit model's esbuild lazy modules, exp097).
#[test]
fn version_stamps_are_pinned() {
    let v = crate::libindex::format::fingerprint_versions();
    let got = format!(
        "statement={} factory={} module={} literals={} units={}",
        crate::hash::forms::STATEMENT.version,
        crate::hash::forms::MATCH_KEY.version,
        v.module,
        v.literals,
        v.units
    );
    assert_eq!(
        got,
        "statement=3 factory=4 module=vendor-content-key/4 literals=literals/1 units=units/3"
    );
}

const PIN_CANONICAL: &str = "566ce95126e5a96647cdf27d";
const PIN_STATEMENT: &str = "9878f404f494a0ce3f235686";
const PIN_ALIGN: &str = "35a3c368cd4e0592e48f47b6";
const PIN_GRAPH: &str = "34ec479a0bce66b4d9d66e8a";
const PIN_MODULES: &str = "9bfa3cd3c7787d97a3f76f25";
const PIN_UNITS: &str = "6d469263bd7c2f510f37c083";
const PIN_FILES: &str = "911b9df2a0edf0f9b5fbb73a";
