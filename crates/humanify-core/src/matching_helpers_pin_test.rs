//! Behaviour pin for the matching machinery's shared helpers (2026-10-13,
//! the matching-unification refactor, step 1).
//!
//! Step 1 moves the duplicated pieces — the fingerprint's known-globals
//! list, the k-shingle windows and the Jaccard ratios, the canonical
//! walk's identifier roles / block unwrap / literal classes / escaping
//! that `matching::statement_align` carried as KEEP-IN-SYNC copies — onto
//! one owner each, with ZERO output change. These digests were recorded
//! on the tree BEFORE the move (main @ 872dff10) and must not change: each
//! one hashes every caller's output over every committed e2e fixture
//! bundle (both versions), so an owner that answers differently from the
//! copy it replaced changes a digest.
//!
//! A digest change is never "update the constant": it means a helper
//! stopped answering the way its callers' copies did.

use std::fmt::Write as _;
use std::path::PathBuf;

use oxc_allocator::Allocator;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::hash::forms::{IDENTITY_KEY, MATCH_KEY};
use crate::hash::serialize::{SymbolTables, canonical_serialize};
use crate::ingest::Ingest;

/// Every committed e2e fixture bundle, as (fixture, v1.0.0 text, v1.1.0
/// text), in fixture-name order.
pub(crate) fn fixture_pairs() -> Vec<(String, String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/e2e/fixtures");
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .expect("e2e fixtures dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    // Fixtures committed AFTER the pins were recorded: the digests pin the
    // consumers' views over the fixtures they were recorded on, and a new
    // fixture adds lines without moving any old one (checked when each was
    // added: the pins hold with it left out). exp172's CommonJS app.
    const ADDED_AFTER_THE_PINS: [&str; 1] = ["bun-cjs-app"];
    names.retain(|n| !ADDED_AFTER_THE_PINS.contains(&n.as_str()));
    let mut out = Vec::new();
    for name in names {
        let at = |v: &str| {
            root.join(&name)
                .join("build")
                .join(v)
                .join("build/index.js")
        };
        let (Ok(a), Ok(b)) = (
            std::fs::read_to_string(at("v1.0.0")),
            std::fs::read_to_string(at("v1.1.0")),
        ) else {
            continue;
        };
        out.push((name, a, b));
    }
    assert!(out.len() >= 18, "expected the 18 committed fixture pairs");
    out
}

/// Assert one section's digest (the section name labels a failure).
fn assert_pinned(section: &str, out: &str, expected: &str) {
    assert_eq!(digest(out), expected, "{section}");
}

pub(crate) fn digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every JSON node of `root` whose type is in `types`, in pre-order.
pub(crate) fn nodes_of<'v>(root: &'v Value, types: &[&str], out: &mut Vec<&'v Value>) {
    match root {
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| types.contains(&t))
            {
                out.push(root);
            }
            for v in map.values() {
                nodes_of(v, types, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| nodes_of(v, types, out)),
        _ => {}
    }
}

pub(crate) const FUNCTION_TYPES: [&str; 3] = [
    "FunctionDeclaration",
    "FunctionExpression",
    "ArrowFunctionExpression",
];

/// Statement-ish and literal-heavy nodes: what the canonical walk and the
/// content-shingle walk see as units.
const UNIT_TYPES: [&str; 12] = [
    "FunctionDeclaration",
    "FunctionExpression",
    "ArrowFunctionExpression",
    "VariableDeclaration",
    "ExpressionStatement",
    "IfStatement",
    "ForStatement",
    "SwitchStatement",
    "ReturnStatement",
    "ObjectExpression",
    "ClassDeclaration",
    "TemplateLiteral",
];

/// One side parsed, with its tables and program JSON.
pub(crate) fn with_side<T>(
    text: &str,
    run: impl FnOnce(&Ingest<'_>, &SymbolTables, &Value) -> T,
) -> T {
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, text, "input.js");
    let tables = SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    run(&ingest, &tables, &json)
}

// ---------------------------------------------------------------------------
// The canonical walk + the statement-align tokenizer (item 4)
// ---------------------------------------------------------------------------

fn canonical_and_content_lines(text: &str, out: &mut String) {
    with_side(text, |_, tables, json| {
        let mut units = Vec::new();
        nodes_of(json, &UNIT_TYPES, &mut units);
        for unit in units {
            let mk = canonical_serialize(unit, tables, &MATCH_KEY);
            let vb = canonical_serialize(unit, tables, &IDENTITY_KEY);
            let shingles = crate::matching::statement_align::content_shingles(tables, unit);
            let joined: Vec<&str> = shingles.iter().map(String::as_str).collect();
            let _ = writeln!(
                out,
                "{} {} {} {}",
                mk.hash,
                vb.hash,
                shingles.len(),
                digest(&joined.join("\u{1}"))
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The function features + the cascade shingle sets (items 1 and 3)
// ---------------------------------------------------------------------------

fn feature_lines(text: &str, out: &mut String) {
    with_side(text, |_, tables, json| {
        let mut fns = Vec::new();
        nodes_of(json, &FUNCTION_TYPES, &mut fns);
        for f in fns {
            let features = crate::matching::features::structural_features_of(f, tables);
            let _ = writeln!(out, "{features:?}");
        }
    });
}

fn index_shingle_sets(text: &str) -> Vec<std::collections::BTreeSet<String>> {
    with_side(text, |ingest, tables, _| {
        let graph = crate::graph::build_unified_graph(
            ingest.semantic(),
            ingest.program,
            "input.js",
            crate::rename::eligibility::NeverRename::UNIVERSAL,
            crate::toolchain::BundleLayout::SingleWrapperFunction,
        );
        let index = crate::matching::build_fingerprint_index(&graph, ingest.semantic(), tables);
        (0..graph.functions.len())
            .map(|i| index.compute_shingle_set(i))
            .collect()
    })
}

// ---------------------------------------------------------------------------
// The text-side extractors: the vendor-pairing lexer + features, the
// content anchor's rare literals, the fossil matcher (items 2 and 3)
// ---------------------------------------------------------------------------

/// The fixture cut into pseudo-modules: blank-line or 25-line chunks.
fn chunks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        cur.push(line);
        if line.trim().is_empty() || cur.len() >= 25 {
            out.push(cur.join("\n"));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join("\n"));
    }
    out
}

fn fossil_signature(chunk: &str) -> crate::place::assign::fossil_match::FossilSignature {
    let mut hashes: Vec<String> = chunk
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| digest(l.trim()))
        .collect();
    hashes.sort();
    let mut tokens: Vec<String> = crate::place::anchor::tokens_of(chunk)
        .into_iter()
        .map(str::to_string)
        .collect();
    tokens.sort();
    let lexed = crate::modules::lex::lex_raw(chunk);
    let declared: Vec<String> = lexed
        .windows(2)
        .filter(|w| matches!(w[0].as_str(), "var" | "let" | "const" | "function"))
        .map(|w| w[1].clone())
        .collect();
    crate::place::assign::fossil_match::FossilSignature {
        hashes,
        imports: Vec::new(),
        stem: None,
        tokens: Some(tokens),
        declared: Some(declared),
    }
}

#[test]
fn canonical_walk_and_content_shingles_are_pinned() {
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(out, "# {name}");
        canonical_and_content_lines(&a, &mut out);
        canonical_and_content_lines(&b, &mut out);
    }
    assert_pinned("canonical", &out, PIN_CANONICAL_AND_CONTENT);
}

#[test]
fn function_features_and_cascade_shingles_are_pinned() {
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(out, "# {name}");
        feature_lines(&a, &mut out);
        feature_lines(&b, &mut out);
        let (sa, sb) = (index_shingle_sets(&a), index_shingle_sets(&b));
        for s in sa.iter().chain(&sb) {
            let joined: Vec<&str> = s.iter().map(String::as_str).collect();
            let _ = writeln!(out, "{}", digest(&joined.join("\u{1}")));
        }
        for x in &sa {
            for y in &sb {
                let _ = write!(out, "{:016x} ", crate::similarity::jaccard(x, y).to_bits());
            }
            out.push('\n');
        }
    }
    assert_pinned("features", &out, PIN_FEATURES_AND_SHINGLES);
}

#[test]
fn text_extractors_and_pairings_are_pinned() {
    use crate::modules::lex::{lex, lex_raw};
    use crate::modules::vendor_pairing::{literal_idf, module_features, pair_by_content};
    use crate::place::anchor::{PriorStatement, content_anchor_verdicts, literals_of, tokens_of};
    let mut out = String::new();
    let mut all_features = Vec::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(out, "# {name}");
        for text in [&a, &b] {
            let _ = writeln!(out, "{}", digest(&lex(text).join("\u{1}")));
            let _ = writeln!(out, "{}", digest(&lex_raw(text).join("\u{1}")));
            let _ = writeln!(out, "{:?}", literals_of(text));
            let mut toks: Vec<&str> = tokens_of(text).into_iter().collect();
            toks.sort_unstable();
            let _ = writeln!(out, "{}", digest(&toks.join("\u{1}")));
        }
        let (ca, cb) = (chunks(&a), chunks(&b));
        let fa: Vec<_> = ca.iter().map(|c| module_features(c)).collect();
        let fb: Vec<_> = cb.iter().map(|c| module_features(c)).collect();
        let _ = writeln!(out, "{fa:?}");
        let idf = literal_idf(fa.iter());
        let _ = writeln!(out, "{:?}", pair_by_content(&fb, &fa, &idf));
        all_features.push(module_features(&a));
        // The content anchor over the chunks as statements.
        let prior: Vec<PriorStatement> = ca
            .iter()
            .map(|t| PriorStatement {
                text: t,
                file: "p.js",
            })
            .collect();
        let fresh: Vec<&str> = cb.iter().map(String::as_str).collect();
        let mut verdicts: Vec<_> = content_anchor_verdicts(&prior, &fresh)
            .into_iter()
            .collect();
        verdicts.sort_by_key(|(i, _)| *i);
        let _ = writeln!(out, "{verdicts:?}");
        // The fossil matcher over the chunks as modules.
        let sa: Vec<_> = ca.iter().map(|c| fossil_signature(c)).collect();
        let sb: Vec<_> = cb.iter().map(|c| fossil_signature(c)).collect();
        let fossil = crate::place::assign::fossil_match::match_fossil_modules(&sa, &sb);
        let mut pair_tiers: Vec<_> = fossil.pair_tiers.iter().collect();
        pair_tiers.sort();
        let _ = writeln!(
            out,
            "{:?} {:?} {pair_tiers:?}",
            fossil.matches, fossil.tiers
        );
    }
    // Whole-file pairing across fixtures (the IDF over all of them).
    let idf = literal_idf(all_features.iter());
    let mut idf_rows: Vec<_> = idf.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
    idf_rows.sort();
    let _ = writeln!(out, "{}", digest(&idf_rows.join("\u{1}")));
    let _ = writeln!(
        out,
        "{:?}",
        pair_by_content(&all_features, &all_features, &idf)
    );
    assert_pinned("text", &out, PIN_TEXT_EXTRACTORS);
}

// ---------------------------------------------------------------------------
// The whole cold match stage + transfer (every caller at once)
// ---------------------------------------------------------------------------

#[test]
fn match_stage_and_transfers_are_pinned() {
    use crate::prior::{PriorMatchInput, match_prior_version};
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let input = PriorMatchInput {
            layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
            fresh: &b,
            prior: &a,
            never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
            fast: false,
            same_program_check: false,
        };
        let line = match_prior_version(input, |stage| {
            let r = stage.function_result;
            let mut s = format!(
                "{:?}\n{:?}\n{:?}\n",
                r.pair_resolutions, r.pair_rejections, r.unmatched
            );
            if let Some(br) = stage.binding_result {
                let _ = write!(s, "{:?}\n{:?}\n", br.pair_resolutions, br.unmatched);
            }
            let _ = writeln!(s, "{:?}", stage.close_file);
            let (outcome, _) = crate::rename::transfer::apply_prior_version(
                stage,
                &Default::default(),
                crate::rename::name_profile::NameProfile::Bun,
            )?;
            let _ = writeln!(s, "{:?}", outcome.rename.trail().transfer_rows());
            Ok(s)
        })
        .unwrap_or_else(|e| format!("ERR {e}\n"));
        let _ = write!(out, "# {name}\n{line}");
    }
    assert_pinned("match", &out, PIN_MATCH_STAGE);
}

const PIN_CANONICAL_AND_CONTENT: &str = "105c08859ae699ba04218fc6";
const PIN_FEATURES_AND_SHINGLES: &str = "5798b8c971df98fa2cebeccf";
const PIN_TEXT_EXTRACTORS: &str = "5aed2a4d97e46b22238b4ab4";
const PIN_MATCH_STAGE: &str = "af413f7a8e6ddb94f7c1fccf";
