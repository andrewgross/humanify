//! The graph-scope fixture (docs/matching-architecture.md §4, Andrew
//! 2026-10-13): one small library bundled into two different apps that
//! call it from different places and different numbers of times, looked up
//! in an index built from the library ALONE. Caller evidence that crosses
//! the app→library boundary (INTO edges) describes the app, not the
//! library, so it may never decide identity: both apps' library regions
//! must identify as the same package with the same per-piece claims.

use oxc_allocator::Allocator;

use crate::graph::edges::Scope;
use crate::ingest::Ingest;
use crate::libdetect::discovery::{
    BundlePieces, DiscoveryOptions, IndexLookup, LibraryProfile, LibraryRule, ProposeFn, RoundView,
    Rule, discover_in_bundle_with,
};
use crate::libindex::format::UnitRow;
use crate::libindex::lookup::{LibraryIndex, LookupRules};
use crate::matching::rules::{Evidence, RuleSpec, Stage};
use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};

/// The library, byte-identical in both apps: two indexed statements
/// (`fmt`, `entry`: 20+ nodes) calling each other, and two small private
/// helpers below the index's row floor (`util`, an export app 2 also calls
/// directly, and `pad`, `util`'s private helper).
const LIBRARY: &str = "var fmt = function (s) { return String(s).trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).join(\"_\"); };\nvar pad = (s) => s + \"!\";\nvar util = (s) => pad(s).trim();\nvar entry = function (t) { return t.split(\",\").map(fmt).map(util).filter(Boolean).join(\"|\").concat(t.length); };\n";

/// App 1 calls the entry once, from one place.
const APP1: &str = "var main = () => entry(\"a,b\");\nmain();\n";

/// App 2 calls the entry twice from two places and the small export
/// directly from two more.
const APP2: &str = "var go = (x) => util(x) + entry(x);\nvar run = () => go(\"c\") + entry(\"d,e\");\nvar also = () => util(\"z\") + util(\"y\");\nrun(); also();\n";

fn pieces() -> BundlePieces {
    BundlePieces {
        layout: BundleLayout::SingleWrapperFunction,
        grammar: ModuleWrapperGrammar::BunAndEsbuild,
        interop: InteropHelpers::Bun,
    }
}

/// The index built from the library alone, as the builder writes it: one
/// row per statement print of 20+ nodes (`UNIT_ROW_MIN_NODES`), owned by
/// `tinylib` — two entries.
fn library_index() -> LibraryIndex {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, LIBRARY, "lib.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let p = pieces();
    let prints = crate::libindex::units::bundle_unit_prints(
        LIBRARY, &ingest, p.layout, p.grammar, p.interop,
    );
    let rows: Vec<UnitRow> = prints
        .iter()
        .filter(|(_, p)| p.whole.nodes >= 20)
        .map(|(_, p)| UnitRow {
            hash: p.whole.hash.clone(),
            owner: "tinylib".into(),
            file: "index.js".into(),
            nodes: p.whole.nodes,
            content: p.whole.content,
            module_bytes: LIBRARY.len() as u64,
            versions: vec![("1.0.0".into(), "1.0.0".into())],
        })
        .collect();
    assert_eq!(rows.len(), 2, "a two-entry index: fmt and entry");
    LibraryIndex::from_rows(rows, vec![])
}

/// The structural rule (no literal agreement: the fixture has too few
/// literals for it), every other tunable at its default.
fn rules() -> LookupRules {
    LookupRules {
        agreement: false,
        ..LookupRules::default()
    }
}

/// Per library piece (its first characters; an app piece prefixed
/// `APP `), the deciding rule and the reference it was decided against —
/// the decision trail (`Discovery::decisions`), with the unit position
/// replaced by its text so two bundles compare.
fn library_claims(
    app: &str,
    index: &LibraryIndex,
    profile: &LibraryProfile,
) -> Vec<(String, String, &'static str)> {
    let code = format!("{LIBRARY}{app}");
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, &code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let rules = rules();
    let (graph, found) = discover_in_bundle_with(
        &code,
        &ingest,
        pieces(),
        &[],
        &[],
        Some(IndexLookup {
            index,
            rules,
            targeted: Some(LookupRules {
                min_nodes: 20,
                support: 0,
                ..rules
            }),
            census: false,
        }),
        DiscoveryOptions::default(),
        profile,
    );
    found
        .decisions(&graph, profile)
        .into_iter()
        .map(|d| {
            let span = d.query.trim_start_matches("unit@");
            let (start, end) = span.split_once('-').expect("unit@start-end");
            let (start, end): (usize, usize) = (start.parse().unwrap(), end.parse().unwrap());
            let head: String = code[start..end].chars().take(12).collect();
            let head = if start < LIBRARY.len() {
                head
            } else {
                format!("APP {head}")
            };
            (head, d.reference, d.rule)
        })
        .collect()
}

#[test]
fn two_apps_calling_one_library_differently_identify_it_the_same() {
    let index = library_index();
    let profile = LibraryProfile::standard();
    let one = library_claims(APP1, &index, &profile);
    let two = library_claims(APP2, &index, &profile);
    assert_eq!(
        one, two,
        "the app's calls into the library (INTO edges) must not decide identity"
    );
    let trail: Vec<(&str, &str, &str)> = one
        .iter()
        .map(|(h, r, rule)| (h.as_str(), r.as_str(), *rule))
        .collect();
    assert_eq!(
        trail,
        vec![
            ("var fmt = fu", "index:tinylib", "structural"),
            ("var pad = (s", "claims:tinylib", "down"),
            ("var util = (", "claims:tinylib", "down"),
            ("var entry = ", "index:tinylib", "structural"),
        ],
        "every library piece, the same per-piece decisions, no app piece"
    );
}

/// The pre-step-4 DOWN, as a probe: a unit is P's when its library callers
/// are all P AND nothing else calls it — the second half reads the INTO
/// edges of P's region (how the app calls the library).
fn probe_down_reading_into(v: &RoundView<'_, '_>, out: &mut Vec<(u32, u32)>) {
    for u in 0..v.units() {
        if !v.is_statement(u) || v.is_external(u) {
            continue;
        }
        let packages = v.inside_caller_packages(u);
        if let [p] = packages.into_iter().collect::<Vec<_>>()[..]
            && v.into_callers(u, p).is_empty()
        {
            out.push((u, p));
        }
    }
}

const PROBE: RuleSpec = RuleSpec::new(
    "probeDownReadingInto",
    Stage::Propagate,
    &[
        Evidence::Graph(Scope::Inside),
        Evidence::Graph(Scope::Into),
        Evidence::Claims,
    ],
);

#[test]
fn the_library_profile_refuses_a_rule_that_reads_into_edges() {
    let mut rules = vec![LibraryRule {
        spec: PROBE,
        rule: Rule::Down,
        propose: probe_down_reading_into as ProposeFn,
    }];
    rules.extend(
        LibraryProfile::standard_rules()
            .into_iter()
            .filter(|r| r.rule != Rule::Down),
    );
    match LibraryProfile::with_propagation(rules) {
        Err(refusal) => {
            assert_eq!(refusal.rule, "probeDownReadingInto");
            assert!(refusal.why.contains("graph:into"), "{}", refusal.why);
        }
        Ok(profile) => {
            // The refusal is gone, so the probe runs — and the app's calls
            // decide what the library is.
            let index = library_index();
            let one = library_claims(APP1, &index, &profile);
            let two = library_claims(APP2, &index, &profile);
            assert_eq!(one, two, "a rule reading INTO edges made the apps disagree");
            panic!("the library profile admitted a rule that reads INTO edges");
        }
    }
}
