//! Library discovery over small synthetic bundles: each rule of the
//! fixpoint, the two guards, DOWN-CLASS and the candidate queue
//! (docs/design/library-discovery.md §5–§7).

use std::collections::HashSet;

use oxc_allocator::Allocator;

use crate::cut::units::BundlerFacts;
use crate::graph::references::{ReferenceGraph, build_reference_graph};
use crate::ingest::Ingest;
use crate::libdetect::discovery::{Discovery, DiscoveryOptions, Evidence, Rule, Seed, discover};
use crate::libindex::families::PackageFamilies;

struct Run {
    code: &'static str,
    graph: ReferenceGraph,
    found: Discovery,
}

impl Run {
    fn unit(&self, head: &str) -> u32 {
        (0..self.graph.units.len() as u32)
            .rev()
            .find(|&u| {
                let s = self.graph.units[u as usize].span;
                self.code[s.start as usize..s.end as usize].starts_with(head)
            })
            .unwrap_or_else(|| panic!("no unit starts with {head:?}"))
    }

    fn claim(&self, head: &str) -> Option<(&str, Rule, u32)> {
        self.found
            .claims
            .get(&self.unit(head))
            .map(|c| (c.package.as_str(), c.rule, c.round))
    }
}

fn declared() -> Evidence {
    Evidence::Declared {
        reader: "package-json".into(),
    }
}

fn run(code: &'static str, seeds: &[(&str, &str, Evidence)]) -> Run {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let facts = BundlerFacts {
        lazy_init: HashSet::from(["b".to_string()]),
        namespace_fillers: HashSet::from(["rt".to_string()]),
        ..BundlerFacts::default()
    };
    let graph = build_reference_graph(code, ingest.semantic(), &ingest.program.body, &[], &facts);
    let mut r = Run {
        code,
        graph,
        found: Discovery::default(),
    };
    let seeds: Vec<Seed> = seeds
        .iter()
        .map(|(head, package, evidence)| Seed {
            unit: r.unit(head),
            package: package.to_string(),
            evidence: evidence.clone(),
        })
        .collect();
    r.found = discover(
        &r.graph,
        &seeds,
        &PackageFamilies::shipped(),
        DiscoveryOptions::default(),
    );
    r
}

#[test]
fn down_claims_private_code_round_by_round() {
    let r = run(
        "var lib = () => priv1();\nvar priv1 = () => priv2();\nvar priv2 = () => 1;\nvar shared = () => 2;\nvar lib2 = () => shared() + lib();\nvar app = () => lib() + shared();\n",
        &[("var lib =", "P", declared())],
    );
    assert_eq!(r.claim("var lib ="), Some(("P", Rule::Seed, 0)));
    assert_eq!(r.claim("var priv1"), Some(("P", Rule::Down, 1)));
    assert_eq!(r.claim("var priv2"), Some(("P", Rule::Down, 2)));
    // Shared with app code: no claim; callers of a library are app code.
    assert_eq!(r.claim("var shared"), None);
    assert_eq!(r.claim("var app"), None);
    assert_eq!(r.claim("var lib2"), None);
    assert_eq!(r.found.rounds.len(), 2);
    assert_eq!(r.found.rounds[0].units, 1);
}

#[test]
fn module_fills_only_behind_its_closure_guard_and_glue_takes_the_wrapper() {
    let r = run(
        "var orphan = () => 1;\nvar orphanOut = () => appUtil();\nvar calledByApp = () => 3;\nvar x;\nvar I = b(() => {\n  x = calledByApp();\n});\nvar appUtil = () => 2;\nvar app = () => calledByApp();\n",
        &[("x = calledByApp", "P", declared())],
    );
    assert_eq!(r.claim("var orphan ="), Some(("P", Rule::Module, 1)));
    assert_eq!(r.claim("var x"), Some(("P", Rule::Down, 1)));
    assert_eq!(r.claim("var I"), Some(("P", Rule::Glue, 1)));
    // Its closure leaves the module: not filled.
    assert_eq!(r.claim("var orphanOut"), None);
    // P's own code calls it: DOWN, though the app calls it too — who calls
    // a library from outside (an INTO edge) is the app's, not identity.
    assert_eq!(r.claim("var calledByApp"), Some(("P", Rule::Down, 1)));
    assert_eq!(r.claim("var appUtil"), None);
}

#[test]
fn the_isolation_veto_drops_index_claims_with_no_same_family_neighbour() {
    let r = run(
        "var kept = () => 1;\nvar app1 = () => kept();\nvar app2 = () => kept() + 1;\nvar real = () => realPriv();\nvar realPriv = () => 1;\nvar appUse = () => real();\nvar said = () => 2;\nvar app3 = () => said();\n",
        &[
            ("var kept", "ink", Evidence::Index),
            ("var real =", "ink", Evidence::Index),
            ("var realPriv", "ink", Evidence::Index),
            ("var said", "ink", declared()),
        ],
    );
    assert_eq!(r.claim("var kept"), None, "a fork's kept table: isolated");
    assert_eq!(r.found.vetoed.len(), 1);
    assert_eq!(r.found.vetoed[0].unit, r.unit("var kept"));
    assert_eq!(r.claim("var real ="), Some(("ink", Rule::Seed, 0)));
    assert_eq!(r.claim("var realPriv"), Some(("ink", Rule::Seed, 0)));
    // A declaration is its own evidence: exempt.
    assert_eq!(r.claim("var said"), Some(("ink", Rule::Seed, 0)));
}

#[test]
fn the_veto_repeats_once_a_dropped_seed_strands_another() {
    // b's only same-family neighbour is a; a is isolated from the start
    // only once b... no: a and b support each other, c hangs off a only
    // through a vetoed seed d.
    let r = run(
        "var d = () => c();\nvar c = () => 1;\nvar app = () => d() + c();\n",
        &[
            ("var d", "lib", Evidence::Index),
            ("var c", "lib", Evidence::Index),
        ],
    );
    // d and c are neighbours of one family: both survive.
    assert_eq!(r.claim("var d"), Some(("lib", Rule::Seed, 0)));
    assert_eq!(r.claim("var c"), Some(("lib", Rule::Seed, 0)));
    let r = run(
        "var d = () => c();\nvar c = () => 1;\nvar e = () => d();\nvar app = () => d() + c() + e();\n",
        &[
            ("var d", "other", Evidence::Index),
            ("var c", "lib", Evidence::Index),
            ("var e", "other", Evidence::Index),
        ],
    );
    // c is isolated (its neighbour d is another family): dropped. d keeps
    // e as a same-family neighbour and survives.
    let vetoed: Vec<u32> = r.found.vetoed.iter().map(|s| s.unit).collect();
    assert_eq!(vetoed, vec![r.unit("var c")]);
    assert_eq!(r.claim("var d"), Some(("other", Rule::Seed, 0)));
    // Its one library caller is d: without an index nothing contradicts
    // DOWN (with one, containment does — `containment_*` below).
    assert_eq!(r.claim("var c"), Some(("other", Rule::Down, 1)));
}

#[test]
fn the_content_floor_keeps_a_content_free_claim_from_owning_app_code() {
    // Two libraries share `dep` and `thing` (DOWN cannot pick one), so
    // both stay unclaimed; DOWN-CLASS marks what a claim of at least the
    // floor reaches (big -> dep), never what only a content-free claim
    // reaches (ns -> thing).
    let r = run(
        "var ns = {};\nvar fill = () => thing();\nvar thing = () => \"app\";\nvar big = () => dep() + \"padding padding padding\";\nvar dep = () => 1;\nvar c = () => dep()+thing();\nvar app = () => dep() + big() + ns.f();\n",
        &[
            ("var ns", "pkgA", declared()),
            ("var fill", "pkgA", declared()),
            ("var big", "pkgB", declared()),
            ("var c", "pkgC", declared()),
        ],
    );
    assert_eq!(r.claim("var dep"), None, "two libraries call it");
    assert_eq!(r.claim("var thing"), None, "two libraries call it");
    let class: Vec<u32> = r.found.library_class.keys().copied().collect();
    assert_eq!(class, vec![r.unit("var dep")]);
    assert_eq!(r.found.library_class[&r.unit("var dep")], "pkgB");
}

#[test]
fn closed_regions_calling_a_library_are_queued_never_named() {
    let r = run(
        "var lib = () => 1;\nvar caller = () => lib() + helper();\nvar helper = () => 2;\nvar top = () => caller();\n",
        &[("var lib", "P", declared())],
    );
    assert_eq!(r.claim("var caller"), None);
    let queued: Vec<u32> = r.found.queue.iter().map(|q| q.unit).collect();
    assert_eq!(queued, vec![r.unit("var caller")]);
    let q = &r.found.queue[0];
    assert_eq!(q.uses, vec!["P".to_string()]);
    assert_eq!(q.region_units, 2, "caller and its private helper");
}

fn seeds_of(code: &'static str, factories: &[&str]) -> (ReferenceGraph, Vec<Seed>) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let spans: Vec<oxc_span::Span> = factories
        .iter()
        .map(|f| {
            let at = code.find(f).expect("factory text") as u32;
            oxc_span::Span::new(at, at + f.len() as u32)
        })
        .collect();
    let graph = build_reference_graph(
        code,
        ingest.semantic(),
        &ingest.program.body,
        &spans,
        &BundlerFacts::default(),
    );
    let seeds = crate::libdetect::discovery::evidence_seeds(code, ingest.semantic(), &graph);
    (graph, seeds)
}

#[test]
fn seeds_come_from_declarations_and_kept_banners() {
    let code = "var pkg = {name: \"left-pad\", version: \"1.3.0\", main: \"index.js\", license: \"MIT\"};\nvar cmd = () => x.n(\"BedrockRuntimeClient\", \"InvokeModelCommand\");\nvar sts = () => y.n(\"STSClient\", \"AssumeRoleCommand\");\nvar oidc = () => z.n(\"SSOOIDCClient\", \"CreateTokenCommand\");\n/*! tiny-lib v2.0.1 | MIT */\nvar tiny = () => 1;\n/*! vendored-lib v1.0.0 */\nvar F = Q((e, m) => { m.exports = 1; });\nvar app = () => 2;\n";
    let (g, seeds) = seeds_of(code, &["F = Q((e, m) => { m.exports = 1; })"]);
    let text = |u: u32| {
        let s = g.units[u as usize].span;
        &code[s.start as usize..s.end as usize]
    };
    let rows: Vec<(&str, &str, String)> = seeds
        .iter()
        .map(|s| {
            let kind = match &s.evidence {
                Evidence::Declared { reader } => reader.clone(),
                e => e.kind().to_string(),
            };
            (
                text(s.unit).split(' ').nth(1).unwrap_or(""),
                s.package.as_str(),
                kind,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("pkg", "left-pad", "package-json".to_string()),
            (
                "cmd",
                "@aws-sdk/client-bedrock-runtime",
                "aws-sdk-client".to_string()
            ),
            ("sts", "@aws-sdk/client-sts", "aws-sdk-client".to_string()),
            (
                "oidc",
                "@aws-sdk/client-ssooidc",
                "aws-sdk-client".to_string()
            ),
            ("tiny", "tiny-lib", "banner".to_string()),
        ],
        "a banner before a bundled module is the module's, not the next statement's"
    );
}

#[test]
fn a_forks_kept_tables_cannot_support_themselves_through_a_module_fill() {
    // The ink fork on Claude Code 2.1.86: the module's only claims are two
    // kept tables; MODULE would fill the app's own parser around them, and
    // the fill must not count as the tables' family support.
    let code = "var tableA = (k) => [\"[a\", \"[b\", \"[c\"].includes(k);\nvar tableB = (k) => [\"Oa\", \"Ob\", \"Oc\"].includes(k);\nvar parse = (s) => tableA(s) || tableB(s) ? 1 : 0;\nvar entry = (s) => parse(s);\nvar I = b(() => {});\nvar app = () => entry(\"x\");\n";
    let r = run(
        code,
        &[
            ("var tableA", "ink", Evidence::Index),
            ("var tableB", "ink", Evidence::Index),
        ],
    );
    assert_eq!(r.found.vetoed.len(), 2);
    assert!(r.found.claims.is_empty(), "{:?}", r.found.claims);
}

#[test]
fn the_report_lists_packages_by_default_and_units_only_when_asked() {
    use crate::libdetect::discovery::DiscoveryReport;
    let r = run(
        "var lib = () => priv();\nvar priv = () => \"padding padding padding padding\";\nvar app = () => lib();\nvar other = () => 1;\nvar app2 = () => other();\n",
        &[
            ("var lib", "P", declared()),
            ("var other", "Q", Evidence::Index),
        ],
    );
    let short = DiscoveryReport::new(&r.graph, &r.found, false);
    assert!(short.claims.is_empty() && short.queue.is_empty() && short.library_class.is_empty());
    let rows: Vec<(&str, u32, u32, u32)> = short
        .packages
        .iter()
        .map(|p| {
            (
                p.package.as_str(),
                p.seed_units,
                p.grown_units,
                p.vetoed_units,
            )
        })
        .collect();
    assert_eq!(rows, vec![("P", 1, 1, 0), ("Q", 0, 0, 1)]);
    let p = &short.packages[0];
    assert_eq!(p.queue_units, 1, "app calls P: one queued region uses P");
    let json = serde_json::to_value(&short).unwrap();
    assert!(
        json.get("claims").is_none(),
        "no per-unit listing by default"
    );
    let long = DiscoveryReport::new(&r.graph, &r.found, true);
    assert_eq!(long.claims.len(), 2);
    assert_eq!(long.packages, short.packages);
}

#[test]
fn index_seeds_and_targeted_queue_lookups_claim_through_the_index() {
    use crate::libdetect::discovery::{BundlePieces, IndexLookup, discover_in_bundle};
    use crate::libindex::format::UnitRow;
    use crate::libindex::lookup::{LibraryIndex, LookupRules};
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let code = "var helper = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar lib = function (t) { return t.split(\",\").map(helper).filter(Boolean).join(\"|\").concat(t.length); };\nvar small = (x) => lib(x).toUpperCase().trim().split(\"|\").concat([x]).join(\"+\");\nvar app = () => small(\"a,b\") + other();\nvar other = () => 1;\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let pieces = BundlePieces {
        layout: BundleLayout::SingleWrapperFunction,
        grammar: ModuleWrapperGrammar::BunAndEsbuild,
        interop: InteropHelpers::Bun,
    };
    // The index: this very code's units, owned by zod.
    let prints = crate::libindex::units::bundle_unit_prints(
        code,
        &ingest,
        pieces.layout,
        pieces.grammar,
        pieces.interop,
    );
    let nodes: Vec<u32> = prints.iter().map(|(_, p)| p.whole.nodes).collect();
    assert!(
        nodes[0] >= 30 && nodes[1] >= 30 && (20..30).contains(&nodes[2]),
        "{nodes:?}"
    );
    let rows = prints[..3]
        .iter()
        .map(|(_, p)| UnitRow {
            hash: p.whole.hash.clone(),
            owner: "zod".into(),
            file: "index.js".into(),
            nodes: p.whole.nodes,
            content: p.whole.content,
            module_bytes: 500,
            versions: vec![("3.0.0".into(), "3.0.0".into())],
        })
        .collect();
    let index = LibraryIndex::from_rows(rows, vec![]);
    let rules = LookupRules {
        agreement: false,
        support: 0,
        ..LookupRules::default()
    };
    let (graph, found) = discover_in_bundle(
        code,
        &ingest,
        pieces,
        &[],
        &[],
        Some(IndexLookup {
            index: &index,
            rules,
            targeted: Some(LookupRules {
                min_nodes: 20,
                ..rules
            }),
        }),
        DiscoveryOptions::default(),
    );
    let claim = |head: &str| {
        let u = (0..graph.units.len() as u32)
            .find(|&u| code[graph.units[u as usize].span.start as usize..].starts_with(head))
            .unwrap();
        found
            .claims
            .get(&u)
            .map(|c| (c.package.clone(), c.evidence.as_ref().map(|e| e.kind())))
    };
    assert_eq!(claim("var helper"), Some(("zod".into(), Some("index"))));
    assert_eq!(claim("var lib"), Some(("zod".into(), Some("index"))));
    assert_eq!(
        claim("var small"),
        Some(("zod".into(), Some("queue-index"))),
        "under the main floor; found by the targeted lookup of a region calling zod"
    );
    assert_eq!(claim("var app"), None);
    // The scope producer: every edge carries its ends' regions (zod is
    // region 0), read relative to a region.
    use crate::graph::edges::Scope;
    let unit = |head: &str| {
        (0..graph.units.len() as u32)
            .find(|&u| code[graph.units[u as usize].span.start as usize..].starts_with(head))
            .unwrap()
    };
    let scope = |from: &str, to: &str| {
        let (a, b) = (unit(from), unit(to));
        let e = graph.edges.iter().find(|e| e.from == a && e.to == b);
        e.and_then(|e| e.scope).and_then(|s| s.relative_to(0))
    };
    assert!(graph.edges.iter().all(|e| e.scope.is_some()));
    assert_eq!(scope("var small", "var lib"), Some(Scope::Inside));
    assert_eq!(scope("var app", "var small"), Some(Scope::Into));
    assert_eq!(scope("var app", "var other"), None, "app code: no region");
}

#[test]
fn a_request_carries_its_rules_and_defaults_to_agreement() {
    use crate::libdetect::discovery::DiscoveryRequest;
    use crate::libindex::lookup::{LibraryIndex, LookupRules};
    let index = LibraryIndex::from_rows(vec![], vec![]);
    let default = DiscoveryRequest {
        index: Some(&index),
        ..DiscoveryRequest::default()
    };
    let l = default.lookup().expect("an index");
    let t = l.targeted.expect("a targeted round");
    assert!(l.rules.agreement && t.agreement, "the pipeline's rule");
    assert_eq!((t.min_nodes, t.support), (20, 0));
    let structural = DiscoveryRequest {
        rules: Some(LookupRules {
            agreement: false,
            ..LookupRules::default()
        }),
        ..default
    };
    let l = structural.lookup().expect("an index");
    let t = l.targeted.expect("a targeted round");
    assert!(!l.rules.agreement && !t.agreement, "both rounds structural");
    assert_eq!((t.min_nodes, t.support), (20, 0));
}

#[test]
fn unit_rows_list_every_graph_unit_with_its_prints_and_bindings() {
    use crate::libdetect::discovery::{BundlePieces, discover_in_bundle, unit_rows};
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let code = "var helper = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\"); };\nvar a = 1, b = function (x, y) { var z = x + y; return z; };\nvar I = init(() => { b(1, 2); });\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let pieces = BundlePieces {
        layout: BundleLayout::SingleWrapperFunction,
        grammar: ModuleWrapperGrammar::BunAndEsbuild,
        interop: InteropHelpers::Bun,
    };
    let rows = unit_rows(code, &ingest, pieces, &[]);
    let (graph, _) = discover_in_bundle(
        code,
        &ingest,
        pieces,
        &[],
        &[],
        None,
        DiscoveryOptions::default(),
    );
    assert_eq!(rows.len(), graph.units.len(), "parallel to the graph");
    for (r, u) in rows.iter().zip(&graph.units) {
        assert_eq!(
            (r.start, r.end, r.bytes),
            (u.span.start, u.span.end, u.bytes)
        );
    }
    let row = |head: &str| {
        rows.iter()
            .find(|r| code[r.start as usize..].starts_with(head))
            .unwrap_or_else(|| panic!("no row {head:?}"))
    };
    let helper = row("var helper");
    let whole = helper.whole.as_ref().expect("a statement has a print");
    assert!(whole.nodes >= 20 && whole.content >= 3, "{whole:?}");
    assert_eq!(helper.functions.len(), 1, "the function expression");
    assert_eq!(helper.bindings, 2, "helper and e");
    assert_eq!(row("b = function").bindings, 4, "b, x, y, z");
    assert_eq!(row("a = 1").bindings, 1);
    assert!(rows.iter().all(|r| r.module_bytes >= u64::from(r.bytes)));
}

/// Per graph unit, its first characters and its claim (package, rule).
type ClaimRows = Vec<(String, Option<(String, Rule)>)>;

/// Discovery over `code` with an index holding, per listed statement
/// head, that statement's whole print under the listed owners (structural
/// rule, no neighbour support).
fn run_indexed(code: &str, held: &[(&str, &[&str])]) -> (ReferenceGraph, Discovery, ClaimRows) {
    let rows: Vec<Held<'_>> = held
        .iter()
        .flat_map(|(head, owners)| owners.iter().map(move |o| Held::whole(head, o)))
        .collect();
    run_indexed_with(code, &rows, vec![], DiscoveryOptions::default())
}

/// One index row of a test index: statement `head`'s whole print, or its
/// largest function's, under `owner`, seen in a module of `module_bytes`.
#[derive(Clone, Copy)]
struct Held<'h> {
    head: &'h str,
    owner: &'h str,
    function: bool,
    module_bytes: u64,
    /// Hold the unit's function print whose span starts with this text
    /// (instead of the whole print or the largest function).
    inner: Option<&'h str>,
}

impl<'h> Held<'h> {
    fn whole(head: &'h str, owner: &'h str) -> Held<'h> {
        Held {
            head,
            owner,
            function: false,
            module_bytes: 500,
            inner: None,
        }
    }

    /// The function print of unit `head` whose span starts with `inner`,
    /// in a module of `module_bytes`.
    fn inner(head: &'h str, inner: &'h str, owner: &'h str, module_bytes: u64) -> Held<'h> {
        Held {
            inner: Some(inner),
            module_bytes,
            ..Held::whole(head, owner)
        }
    }

    fn function(head: &'h str, owner: &'h str) -> Held<'h> {
        Held {
            function: true,
            ..Held::whole(head, owner)
        }
    }

    /// Seen only in a module far bigger than any query's: the lookup's
    /// specificity rule never seeds from it, the index still holds it.
    fn unspecific(self) -> Held<'h> {
        Held {
            module_bytes: 100_000_000,
            ..self
        }
    }
}

fn run_indexed_with(
    code: &str,
    held: &[Held<'_>],
    literals: Vec<crate::libindex::format::LiteralRow>,
    options: DiscoveryOptions,
) -> (ReferenceGraph, Discovery, ClaimRows) {
    use crate::libdetect::discovery::{BundlePieces, IndexLookup, discover_in_bundle};
    use crate::libindex::format::UnitRow;
    use crate::libindex::lookup::{LibraryIndex, LookupRules};
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let pieces = BundlePieces {
        layout: BundleLayout::SingleWrapperFunction,
        grammar: ModuleWrapperGrammar::BunAndEsbuild,
        interop: InteropHelpers::Bun,
    };
    let prints = crate::libindex::units::bundle_unit_prints(
        code,
        &ingest,
        pieces.layout,
        pieces.grammar,
        pieces.interop,
    );
    let mut rows = Vec::new();
    for h in held {
        let (_, p) = prints
            .iter()
            .find(|(u, _)| code[u.span.start as usize..].starts_with(h.head))
            .unwrap_or_else(|| {
                let heads: Vec<&str> = prints
                    .iter()
                    .map(|(u, _)| {
                        &code[u.span.start as usize..(u.span.start as usize + 12).min(code.len())]
                    })
                    .collect();
                panic!("no unit {:?} in {heads:?}", h.head)
            });
        let print = if let Some(inner) = h.inner {
            p.functions
                .iter()
                .find(|f| code[f.span.start as usize..].starts_with(inner))
                .unwrap_or_else(|| panic!("{:?} has no function {inner:?}", h.head))
        } else if h.function {
            p.functions
                .iter()
                .max_by_key(|f| f.nodes)
                .unwrap_or_else(|| panic!("{:?} has no function", h.head))
        } else {
            &p.whole
        };
        rows.push(UnitRow {
            hash: print.hash.clone(),
            owner: h.owner.into(),
            file: "index.js".into(),
            nodes: print.nodes,
            content: print.content,
            module_bytes: h.module_bytes,
            versions: vec![("1.0.0".into(), "1.0.0".into())],
        });
    }
    let index = LibraryIndex::from_rows(rows, literals);
    let rules = LookupRules {
        agreement: false,
        support: 0,
        ..LookupRules::default()
    };
    let (graph, found) = discover_in_bundle(
        code,
        &ingest,
        pieces,
        &[],
        &[],
        Some(IndexLookup {
            index: &index,
            rules,
            targeted: None,
        }),
        options,
    );
    let rows = graph
        .units
        .iter()
        .enumerate()
        .map(|(u, unit)| {
            let head: String = code[unit.span.start as usize..].chars().take(10).collect();
            let claim = found
                .claims
                .get(&(u as u32))
                .map(|c| (c.package.clone(), c.rule));
            (head, claim)
        })
        .collect();
    (graph, found, rows)
}

fn claim_of(rows: &ClaimRows, head: &str) -> Option<(String, Rule)> {
    rows.iter()
        .find(|(h, _)| h.starts_with(head))
        .unwrap_or_else(|| {
            let heads: Vec<&String> = rows.iter().map(|(h, _)| h).collect();
            panic!("no unit {head:?} in {heads:?}")
        })
        .1
        .clone()
}

const CONTAINED: &str = "var lib = function (t) { return t.split(\",\").map(dep).map(tiny).filter(Boolean).join(\"|\").concat(t.length); };\nvar dep = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar tiny = (x) => x + 1;\nvar app = () => lib(\"a\") + dep(\"b\");\n";

#[test]
fn containment_refuses_a_propagated_unit_the_package_s_build_does_not_hold() {
    // A bundled library is a subset of its package's build. dep is at or
    // over the index's row floor and P's rows do not hold it (Q's do, or
    // nobody's): P's build cannot contain it, so DOWN from P's claim does
    // not take it. tiny is under the floor: the index can say nothing, DOWN
    // takes it. app is called by nobody: never claimed.
    for held in [
        &[("var lib", &["P"][..]), ("var dep", &["Q"][..])][..],
        &[("var lib", &["P"][..])][..],
    ] {
        let (_, _, rows) = run_indexed(CONTAINED, held);
        assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
        assert_eq!(claim_of(&rows, "var dep"), None, "{held:?}");
        assert_eq!(claim_of(&rows, "var tiny"), Some(("P".into(), Rule::Down)));
        assert_eq!(claim_of(&rows, "var app"), None);
    }
}

#[test]
fn an_ambiguous_seed_cannot_support_itself_through_its_own_claims() {
    // lib's only neighbour is tiny, claimed by DOWN from lib alone. Held
    // by P only, lib keeps that support (as before step 4); held by P and
    // Q (a sponge, a fork), it needs independent evidence and is vetoed.
    let code = "var lib = function (t) { return t.split(\",\").map(tiny).filter(Boolean).join(\"|\").concat(t.length); };\nvar tiny = (x) => x + 1;\nvar app = () => lib(\"a\");\n";
    let (_, found, rows) = run_indexed(code, &[("var lib", &["P"])]);
    assert!(found.vetoed.is_empty());
    assert_eq!(claim_of(&rows, "var tiny"), Some(("P".into(), Rule::Down)));
    let (_, found, rows) = run_indexed(code, &[("var lib", &["P", "Q"])]);
    assert_eq!(found.vetoed.len(), 1);
    assert_eq!(claim_of(&rows, "var lib"), None);
    assert_eq!(claim_of(&rows, "var tiny"), None);
}

#[test]
fn the_boundary_check_counts_into_edges_after_the_claims() {
    // P's rows hold dep too: DOWN may take it, though the app calls it.
    let (_, found, rows) = run_indexed(CONTAINED, &[("var lib", &["P"]), ("var dep", &["P", "Q"])]);
    assert_eq!(claim_of(&rows, "var dep").map(|c| c.0), Some("P".into()));
    // app calls lib and dep: two INTO edges landing on two P units.
    let b = found.boundary.get("P").copied().unwrap_or_default();
    assert_eq!((b.into_edges, b.into_units), (2, 2));
}

/// dep's whole print is one family's; one nested function of it is
/// another's: the unit's own evidence says the first.
const CONFLICT: &str = "var lib = function (t) { return t.split(\",\").map(dep).map(tiny).filter(Boolean).join(\"|\").concat(t.length, t.length); };\nvar tiny = (x) => x + 1;\nvar dep = function (e) { var inner = function (z) { return z.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, z.length); }; return [inner(e), e.length, e.indexOf(\"x\"), e.lastIndexOf(\"y\"), e.charAt(0)].join(\"\"); };\nvar app = () => lib(\"a\");\n";

#[test]
fn evidence_conflict_refuses_a_propagated_unit_another_family_covers_more_of() {
    // lib seeds P. dep: P's build holds only its nested `inner`, Q's holds
    // the whole statement (seen only in a big module, so it seeds
    // nothing). Containment passes (P holds a print of dep); the conflict
    // guard does not: Q's evidence covers more of dep than P's. (Every dep
    // row is unspecific, so dep is no seed of its own.)
    let held = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
        Held::whole("var dep", "Q").unspecific(),
    ];
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], DiscoveryOptions::default());
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    let on = DiscoveryOptions {
        evidence_conflict: true,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var dep"), None);
    assert_eq!(claim_of(&rows, "var tiny"), Some(("P".into(), Rule::Down)));
    // A tie (a fork holding the same print) is no conflict: DOWN takes it.
    let tie = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
        Held::function("var dep", "Q").unspecific(),
    ];
    let (_, _, rows) = run_indexed_with(CONFLICT, &tie, vec![], on);
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
}

/// tbl is a table under the index's row floor: one long string, which no
/// unit print can speak for — only the literal table can.
const TABLE: &str = "var lib = function (t) { return t.split(\",\").map(tbl).map(tiny).filter(Boolean).join(\"|\").concat(t.length, t.length); };\nvar tiny = (x) => x + 1;\nvar tbl = (i) => \"!}.&u%}'&}*'~!6*)%&,~!J~!J~%L~y<~!R,~~%Lu~~#GD~~#|)1#%}^%}2%+#.##%##%}&%##%'#%##&%#%#'%#&#%#&#'#%%#&#%##%#)%''%&%#%#'%#%%#%%}%%%#%#&(23#%%#\".charCodeAt(i);\nvar app = () => lib(\"a\");\n";

fn literal_row(owner: &str, statement: &str) -> crate::libindex::format::LiteralRow {
    use crate::libindex::literals::{LiteralMode, code_literals};
    crate::libindex::format::LiteralRow {
        owner: owner.into(),
        file: "index.js".into(),
        lits: code_literals(statement, LiteralMode::Bundled)
            .expect("parses")
            .into_iter()
            .collect(),
        versions: vec![("1.0.0".into(), "1.0.0".into())],
    }
}

#[test]
fn literal_conflict_refuses_a_propagated_unit_whose_long_literal_is_another_family_s() {
    let tbl = TABLE.lines().find(|l| l.starts_with("var tbl")).unwrap();
    let held = [Held::whole("var lib", "P")];
    let on = DiscoveryOptions {
        literal_conflict: true,
        ..DiscoveryOptions::default()
    };
    // Q's files hold tbl's long string; P's do not.
    let q_only = || vec![literal_row("Q", tbl)];
    let (_, _, rows) = run_indexed_with(TABLE, &held, q_only(), DiscoveryOptions::default());
    assert_eq!(claim_of(&rows, "var tbl"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(TABLE, &held, q_only(), on);
    assert_eq!(claim_of(&rows, "var tbl"), None);
    assert_eq!(claim_of(&rows, "var tiny"), Some(("P".into(), Rule::Down)));
    // P's files hold it too, or nobody's do: no conflict.
    for lits in [vec![literal_row("Q", tbl), literal_row("P", tbl)], vec![]] {
        let (_, _, rows) = run_indexed_with(TABLE, &held, lits, on);
        assert_eq!(claim_of(&rows, "var tbl"), Some(("P".into(), Rule::Down)));
    }
}

#[test]
fn contested_cover_refuses_a_big_unit_claimed_through_a_thin_shared_print() {
    // dep's nested `inner` is held by P and Q alike; the rest of dep is
    // nobody's. Under the contested-cover floor (P's cover / dep's nodes),
    // a unit another family also speaks for is refused; over it, or with
    // nobody else speaking, DOWN takes it.
    let held = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
        Held::function("var dep", "Q").unspecific(),
    ];
    let with = |floor: f64| DiscoveryOptions {
        contested_cover: floor,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], with(0.0));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], with(0.99));
    assert_eq!(claim_of(&rows, "var dep"), None);
    assert_eq!(claim_of(&rows, "var tiny"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], with(0.01));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    let uncontested = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
    ];
    let (_, _, rows) = run_indexed_with(CONFLICT, &uncontested, vec![], with(0.99));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
}

#[test]
fn evidence_conflict_drops_an_index_seed_another_family_covers_more_of() {
    // lib and dep seed as P (dep through its nested `inner` (the lookup's function
    // cover; Q's whole-print row is too unspecific for the lookup to read) and
    // support each other.
    // The index still says Q holds all of dep: with the guard on, the seed
    // is dropped like a propagated claim would be.
    let code = "var lib = function (t) { return t.split(\",\").map(dep).filter(Boolean).join(\"|\").concat(t.length, t.length); };\nvar dep = function (e) { var inner = function (z) { return z.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, z.length).concat([z.length, z.charAt(0)]); }; return inner(e); };\nvar app = () => lib(\"a\");\n";
    let held = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P"),
        Held::whole("var dep", "Q").unspecific(),
    ];
    let (_, _, rows) = run_indexed_with(code, &held, vec![], DiscoveryOptions::default());
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Seed)));
    let on = DiscoveryOptions {
        evidence_conflict: true,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(code, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var dep"), None);
}

/// exp107: webpack's module table is ONE statement holding every module
/// (`var e={101(e){…},202(e){…}}`). Bun's rule that the statements before a
/// module are its hoisted declarations would put the table in its FIRST
/// module's group, and MODULE then filled the whole table as that module's
/// package. A statement holding two or more modules is their container: in
/// no module's group.
#[test]
fn a_statement_holding_several_modules_is_no_module_s_member() {
    use crate::libdetect::discovery::{BundlePieces, discover_in_bundle};
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let code = "(()=>{var e={101(e){e.exports=function(){return 1}},202(e){e.exports=function(){return 2}}},t={};function n(r){var o=t[r];if(void 0!==o)return o.exports;var i=t[r]={exports:{}};return e[r](i,i.exports,n),i.exports}var o=n(101);module.exports=o})();";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let pieces = BundlePieces {
        layout: BundleLayout::WebpackBootstrap,
        grammar: ModuleWrapperGrammar::Webpack,
        interop: InteropHelpers::Bun,
    };
    let seed = code.find("e.exports=function(){return 1}").unwrap() as u32;
    let (graph, found) = discover_in_bundle(
        code,
        &ingest,
        pieces,
        &[],
        &[(seed, "P".to_string())],
        None,
        DiscoveryOptions::default(),
    );
    let unit = |head: &str| {
        (0..graph.units.len() as u32)
            .find(|&u| {
                let s = graph.units[u as usize].span;
                code[s.start as usize..s.end as usize].starts_with(head)
            })
            .unwrap_or_else(|| {
                let all: Vec<&str> = graph
                    .units
                    .iter()
                    .map(|u| &code[u.span.start as usize..u.span.end as usize])
                    .collect();
                panic!("no unit starts with {head:?}: {all:?}")
            })
    };
    let claimed = |head: &str| {
        found
            .claims
            .get(&unit(head))
            .map(|c| (c.package.clone(), c.rule))
    };
    assert_eq!(
        claimed("e.exports=function(){return 1}"),
        Some(("P".into(), Rule::Seed))
    );
    assert_eq!(
        claimed("e={101"),
        None,
        "the module table is no module's code"
    );
}
// ---- exp106 matcher hygiene ----------------------------------------------

/// Two banners: one names `noble-curves` (npm has `@noble/curves`), one a
/// name no index knows. `held` rows only make names known (unspecific:
/// they seed nothing).
const BANNERS: &str = "/*! noble-curves v1.2.0 */\nvar lib = function (t) { return t.split(\",\").map(Number).filter(Boolean).join(\"|\").concat(t.length); };\n/*! no-such-package v1.0.0 */\nvar other = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").slice(0, e.length); };\nvar known = function (k) { return [k, k + 1, k * 2, String(k), k.toFixed(2), k.toString(16)].join(\",\"); };\nvar app = () => lib(\"a\") + other(\"b\") + known(1);\n";

#[test]
fn banner_names_known_keeps_only_names_the_index_knows_resolved_to_npm_spelling() {
    use crate::libdetect::discovery::BannerNames;
    let held = [Held::whole("var known", "@noble/curves").unspecific()];
    let (_, _, rows) = run_indexed_with(BANNERS, &held, vec![], DiscoveryOptions::default());
    assert_eq!(
        claim_of(&rows, "var lib"),
        Some(("noble-curves".into(), Rule::Seed))
    );
    assert_eq!(
        claim_of(&rows, "var other"),
        Some(("no-such-package".into(), Rule::Seed))
    );
    let on = DiscoveryOptions {
        banner_names: BannerNames::Known,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(BANNERS, &held, vec![], on);
    assert_eq!(
        claim_of(&rows, "var lib"),
        Some(("@noble/curves".into(), Rule::Seed)),
        "scope-name spelling resolves to the one known scoped package"
    );
    assert_eq!(
        claim_of(&rows, "var other"),
        None,
        "an unknown name is no seed"
    );
}

#[test]
fn helper_prints_held_by_many_families_seed_and_carry_nothing() {
    // lib's whole statement is P's alone, but its function (nearly all of
    // it) is held by four families: emitted helper code (a tsc / Babel
    // helper every package that needs it inlines). Under the helper floor
    // it is no seed. dep's function likewise, so DOWN from a real P seed
    // (root) does not take it.
    let code = "var root = function (r) { return [r.a, r.b, r.c, r.d].map(lib).map(dep).map(more).filter(Boolean).join(\";\").concat(r.length); };\nvar more = function (m) { return [m, m * 3, m - 7, Math.max(m, 9), Math.min(m, 2), m % 5].reduce((x, y) => x + y, 0); };\nvar lib = function (t) { return t.split(\",\").map(Number).filter(Boolean).join(\"|\").concat(t.length); };\nvar dep = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar app = () => root(\"a\");\n";
    let mut held = vec![
        Held::whole("var root", "P"),
        Held::whole("var more", "P"),
        Held::whole("var lib", "P"),
    ];
    for q in ["Q1", "Q2", "Q3", "P"] {
        held.push(Held::function("var lib", q).unspecific());
        held.push(Held::function("var dep", q).unspecific());
    }
    let with = |n: u32| DiscoveryOptions {
        helper_families: n,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(0));
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(4));
    assert_eq!(claim_of(&rows, "var root"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var lib"), None);
    assert_eq!(claim_of(&rows, "var dep"), None);
    // Held by fewer families than the floor: not a helper.
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(5));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
}

#[test]
fn a_data_payload_is_not_claimed_by_propagation_without_its_own_evidence() {
    // lib (P) calls blob, a big base64 string the index holds no print or
    // literal of for P: DOWN would take it as P's on the call alone. Under
    // the payload floor a propagated unit that is mostly string bytes needs
    // P's own evidence (a print, or its long literal in P's files).
    let blob: String = "QUJD".repeat(400);
    let code: &'static str = Box::leak(
        format!("var lib = function (t) {{ return t.split(\",\").map(Number).filter(Boolean).join(\"|\").concat(t.length, blob.length, more(t)); }};\nvar more = function (m) {{ return [m, m * 3, m - 7, Math.max(m, 9), Math.min(m, 2), m % 5].reduce((x, y) => x + y, 0); }};\nvar blob = \"{blob}\";\nvar app = () => lib(\"a\");\n")
            .into_boxed_str(),
    );
    let held = [Held::whole("var lib", "P"), Held::whole("var more", "P")];
    let with = |n: u32| DiscoveryOptions {
        payload_min_bytes: n,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(0));
    assert_eq!(claim_of(&rows, "var blob"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(1024));
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var blob"), None);
    // P's files hold the long string: its own evidence, DOWN takes it.
    let line = code.lines().find(|l| l.starts_with("var blob")).unwrap();
    let (_, _, rows) = run_indexed_with(code, &held, vec![literal_row("P", line)], with(1024));
    assert_eq!(claim_of(&rows, "var blob"), Some(("P".into(), Rule::Down)));
    // Under the byte floor: not a payload.
    let (_, _, rows) = run_indexed_with(code, &held, vec![], with(100_000));
    assert_eq!(claim_of(&rows, "var blob"), Some(("P".into(), Rule::Down)));
}

#[test]
fn a_module_wrapper_is_judged_by_its_own_bytes() {
    // I is a lazy module whose body (a data table: a highlight.js
    // language, a moment locale) is P's seed by its own print. The wrapper
    // itself is a few bytes of call; its span holds the body, but the body
    // is a unit of its own, judged as one. Read by its span, the wrapper
    // looked like a string payload P has no evidence of: GLUE was refused,
    // and the seed, left without support, was vetoed with it.
    let words: Vec<String> = (0..300).map(|i| format!("kw{i}")).collect();
    let code: &'static str = Box::leak(
        format!("var b=(e,t)=>()=>(e&&(t=e(e=0)),t);\nvar N;\nvar I=b(()=>{{\n  N=lang({{name: \"Lang\", keyword: \"{0}\", built_in: \"{0}\", literal: \"{0}\", type: [\"a\", \"b\", \"c\"], aliases: [\"l\", \"lg\"], contains: [N, I, b, lang, Number, String, Boolean]}});\n}});\nvar main=()=>(I(),N);\n", words.join(" "))
            .into_boxed_str(),
    );
    let held = [Held::whole("N=lang", "P")];
    let off = DiscoveryOptions::default();
    let (_, _, rows) = run_indexed_with(code, &held, vec![], off);
    assert_eq!(claim_of(&rows, "N=lang"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var I="), Some(("P".into(), Rule::Glue)));
    let on = DiscoveryOptions {
        payload_min_bytes: 1024,
        ..off
    };
    let (_, _, rows) = run_indexed_with(code, &held, vec![], on);
    assert_eq!(claim_of(&rows, "N=lang"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var I="), Some(("P".into(), Rule::Glue)));
}

#[test]
fn an_opaque_run_inside_code_is_a_payload() {
    // The wasm-bindgen shape: a small function whose argument is a long
    // base64 string (the wasm module). Its bytes are the run's, so it is a
    // payload whatever code wraps it.
    let blob: String = "AGFzbQEAAAAB".repeat(200);
    let code: &'static str = Box::leak(
        format!("var lib = function (t) {{ return t.split(\",\").map(Number).filter(Boolean).join(\"|\").concat(t.length, wasm(t), more(t)); }};\nvar more = function (m) {{ return [m, m * 3, m - 7, Math.max(m, 9), Math.min(m, 2), m % 5].reduce((x, y) => x + y, 0); }};\nvar wasm = function (a) {{ return load(1, null, \"{blob}\"); }};\nvar app = () => lib(\"a\");\n")
            .into_boxed_str(),
    );
    let held = [Held::whole("var lib", "P"), Held::whole("var more", "P")];
    let off = DiscoveryOptions::default();
    let (_, _, rows) = run_indexed_with(code, &held, vec![], off);
    assert_eq!(claim_of(&rows, "var wasm"), Some(("P".into(), Rule::Down)));
    let on = DiscoveryOptions {
        payload_min_bytes: 1024,
        ..off
    };
    let (_, _, rows) = run_indexed_with(code, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var wasm"), None);
}

/// Standard base64 (padded), for payload fixtures.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn an_opaque_payload_that_names_its_package_is_its_own() {
    // A base64-embedded worker script whose decoded text opens with the
    // package's own banner ("/*! @acme/worker-kit 1.2.3 */"): the payload
    // names whose it is, so the call reaching it may claim it. A payload
    // naming another package, or nothing, stays refused.
    let unit = |banner: &str| {
        let script = format!(
            "/*! {banner} 1.2.3 | https://example.com */\n{}",
            "self.onmessage=function(e){postMessage(e.data*2)};".repeat(40)
        );
        let code: &'static str = Box::leak(
            format!("var lib = function (t) {{ return t.split(\",\").map(Number).filter(Boolean).join(\"|\").concat(t.length, worker.length, more(t)); }};\nvar more = function (m) {{ return [m, m * 3, m - 7, Math.max(m, 9), Math.min(m, 2), m % 5].reduce((x, y) => x + y, 0); }};\nvar worker = \"{}\";\nvar app = () => lib(\"a\");\n", base64(script.as_bytes()))
                .into_boxed_str(),
        );
        code
    };
    let held = [
        Held::whole("var lib", "@acme/worker-kit"),
        Held::whole("var more", "@acme/worker-kit"),
    ];
    let on = DiscoveryOptions {
        payload_min_bytes: 1024,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(unit("@acme/worker-kit"), &held, vec![], on);
    assert_eq!(
        claim_of(&rows, "var worker"),
        Some(("@acme/worker-kit".into(), Rule::Down))
    );
    for other in ["@acme/worker-kit-pro", "@other/thing", "no banner"] {
        let (_, _, rows) = run_indexed_with(unit(other), &held, vec![], on);
        assert_eq!(claim_of(&rows, "var worker"), None, "{other}");
    }
}

#[test]
fn specific_rivals_only_a_sponge_s_unspecific_print_is_no_conflict() {
    // CONFLICT: Q holds dep's whole print, but only in a module far bigger
    // than any query's (a sponge bundling P's code). With rivals read
    // under the lookup's specificity rule, that is no evidence against P.
    let held = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
        Held::whole("var dep", "Q").unspecific(),
    ];
    let on = DiscoveryOptions {
        evidence_conflict: true,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var dep"), None);
    let specific = DiscoveryOptions {
        specific_rivals: true,
        ..on
    };
    let (_, _, rows) = run_indexed_with(CONFLICT, &held, vec![], specific);
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    // A specific rival still conflicts.
    let rival = [
        Held::whole("var lib", "P"),
        Held::function("var dep", "P").unspecific(),
        Held::whole("var dep", "Q"),
    ];
    let (_, _, rows) = run_indexed_with(CONFLICT, &rival, vec![], specific);
    assert_ne!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
}

// ---- exp114 dev precision --------------------------------------------------

/// A Bun lazy module (`init_m`) whose group holds lib, dep and x: lib and
/// dep sit in a module of the group's bytes. `UNGROUPED` is the same code
/// with no wrapper (every unit its own module).
const GROUPED: &str = "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\nvar lib = function (t) { return t.split(\",\").map(dep).filter(Boolean).join(\"|\").concat(t.length); };\nvar dep = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar x;\nvar init_m = __esm(() => { x = lib(\"a\"); });\nvar app = () => init_m();\n";
const UNGROUPED: &str = "var lib = function (t) { return t.split(\",\").map(dep).filter(Boolean).join(\"|\").concat(t.length); };\nvar dep = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar app = () => lib(\"a\");\n";

fn held_at(head: &'static str, owner: &'static str, module_bytes: u64) -> Held<'static> {
    Held {
        module_bytes,
        ..Held::whole(head, owner)
    }
}

#[test]
fn context_presence_drops_a_package_seen_only_in_contexts_bigger_than_the_bundle_s() {
    // P's index holds lib only in a 3,000-byte module: bigger than the
    // ~300-byte module lib sits in here (still inside the lookup's
    // specificity bound, so it seeds). Every P seed oversized: P's copy of
    // this code lives in an aggregate (a dist bundling its dependency); the
    // bundle's smaller module is the code's origin, not P. P is no seed and
    // propagates nothing.
    let on = DiscoveryOptions {
        context_presence: 1.5,
        context_presence_slack: 0,
        ..DiscoveryOptions::default()
    };
    // dep: P's build holds it (containment), in no module a query reads.
    let oversized = [
        held_at("var lib", "P", 3000),
        Held::whole("var dep", "P").unspecific(),
    ];
    let (_, _, rows) = run_indexed_with(GROUPED, &oversized, vec![], DiscoveryOptions::default());
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Down)));
    let (_, _, rows) = run_indexed_with(GROUPED, &oversized, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), None);
    assert_eq!(claim_of(&rows, "var dep"), None);
    // One right-sized seed of P anywhere in the bundle: P is present, every
    // seed of it stays.
    let mixed = [held_at("var lib", "P", 3000), held_at("var dep", "P", 100)];
    let (_, _, rows) = run_indexed_with(GROUPED, &mixed, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var dep"), Some(("P".into(), Rule::Seed)));
    // A unit in no module group (scope-hoisted code) has no module size to
    // compare: not judged.
    let (_, _, rows) = run_indexed_with(UNGROUPED, &oversized, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    // The slack: a context within factor x module + slack is right-sized.
    let slack = DiscoveryOptions {
        context_presence_slack: 4096,
        ..on
    };
    let (_, _, rows) = run_indexed_with(GROUPED, &oversized, vec![], slack);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
}

/// lib and h2 (lib calls h2) are held by P and Q alike: ties, which the
/// lookup gives P (name order). other is Q's alone.
const SHADOW: &str = "var lib = function (t) { return t.split(\",\").map(h2).filter(Boolean).join(\"|\").concat(t.length); };\nvar h2 = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar other = function (k) { return [k, k + 1, k * 2, String(k), k.toFixed(2), k.toString(16)].join(\",\"); };\nvar app = () => lib(\"a\") + other(1);\n";

#[test]
fn shadow_ties_give_a_package_with_only_ambiguous_seeds_to_the_rival_the_bundle_evidences() {
    let on = DiscoveryOptions {
        shadow_ties: true,
        ..DiscoveryOptions::default()
    };
    let held = [
        Held::whole("var lib", "P"),
        Held::whole("var lib", "Q"),
        Held::whole("var h2", "P"),
        Held::whole("var h2", "Q"),
        Held::whole("var other", "Q"),
    ];
    let (_, _, rows) = run_indexed_with(SHADOW, &held, vec![], DiscoveryOptions::default());
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var h2"), Some(("P".into(), Rule::Seed)));
    // P's every seed is a tie with Q, and Q has evidence of its own here:
    // P is Q's shadow (a package carrying Q's code), its ties are Q's.
    let (_, _, rows) = run_indexed_with(SHADOW, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("Q".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var h2"), Some(("Q".into(), Rule::Seed)));
    // Q with no evidence of its own: nothing to prefer, the ties stay P's.
    let (_, _, rows) = run_indexed_with(SHADOW, &held[..4], vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    // P with a seed of its own (other held by P alone too would tie; give
    // P a unique unit instead): P is present, its ties stay.
    let own = [
        Held::whole("var lib", "P"),
        Held::whole("var lib", "Q"),
        Held::whole("var h2", "P"),
        Held::whole("var other", "Q"),
    ];
    let (_, _, rows) = run_indexed_with(SHADOW, &own, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
    // Q holds the tied code only in a bigger module than P does: Q would be
    // the CARRIER of P's code, not its origin — the ties stay P's.
    let bigger = [
        Held::whole("var lib", "P"),
        held_at("var lib", "Q", 4000),
        Held::whole("var h2", "P"),
        held_at("var h2", "Q", 4000),
        Held::whole("var other", "Q"),
    ];
    let (_, _, rows) = run_indexed_with(SHADOW, &bigger, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("P".into(), Rule::Seed)));
}

/// GROUPED with a caller of lib, `user`, that is Q's: with P dropped, Q's
/// DOWN would take lib and dep (Q's build holds them, unspecific).
const FENCED: &str = "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\nvar lib = function (t) { return t.split(\",\").map(dep).filter(Boolean).join(\"|\").concat(t.length); };\nvar dep = function (e) { return e.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, e.length); };\nvar user = function (k) { return [k, k + 1, k * 2, String(k), k.toFixed(2), lib(String(k))].join(\",\"); };\nvar x;\nvar init_m = __esm(() => { x = user(1); });\nvar app = () => init_m();\n";

#[test]
fn presence_fence_names_neither_the_dropped_package_nor_its_rival() {
    let held = [
        held_at("var lib", "P", 3000),
        Held::whole("var dep", "P").unspecific(),
        held_at("var user", "Q", 100),
        Held::whole("var lib", "Q").unspecific(),
        Held::whole("var dep", "Q").unspecific(),
    ];
    let drop = DiscoveryOptions {
        context_presence: 1.5,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(FENCED, &held, vec![], drop);
    assert_eq!(claim_of(&rows, "var lib"), Some(("Q".into(), Rule::Down)));
    // Fenced: the code P's seeds stood on is ambiguous between the shipper
    // and the origin; nothing names it.
    let fence = DiscoveryOptions {
        presence_fence: true,
        ..drop
    };
    let (_, found, rows) = run_indexed_with(FENCED, &held, vec![], fence);
    assert_eq!(claim_of(&rows, "var lib"), None);
    assert_eq!(claim_of(&rows, "var dep"), None);
    assert!(found.claims.values().all(|c| !c.package.starts_with('<')));
}

fn run_with_options(
    code: &'static str,
    seeds: &[(&str, &str, Evidence)],
    options: DiscoveryOptions,
) -> Run {
    let mut r = run(code, &[]);
    let seeds: Vec<Seed> = seeds
        .iter()
        .map(|(head, package, evidence)| Seed {
            unit: r.unit(head),
            package: package.to_string(),
            evidence: evidence.clone(),
        })
        .collect();
    r.found = discover(&r.graph, &seeds, &PackageFamilies::shipped(), options);
    r
}

#[test]
fn a_declared_seed_never_claims_a_statement_bigger_than_the_declared_cap() {
    // exp112: a package.json literal (`{name:"P",version:...}`) inside a
    // huge statement (a nested bundle's one factory holding many packages)
    // says P's manifest is somewhere in it, not that the statement is P's.
    // Under the cap the declared seed is refused, and nothing propagates
    // from it; a small statement still seeds.
    let code: &'static str = Box::leak(
        format!(
            "var big = function () {{ var m = {{ name: \"P\", version: \"1.0.0\" }}; return [m, {}].join(); }};\nvar small = {{ name: \"Q\", version: \"2.0.0\" }};\nvar app = () => big() + small.name;\n",
            (0..200).map(|i| format!("\"payload{i}\"")).collect::<Vec<_>>().join(", ")
        )
        .into_boxed_str(),
    );
    let seeds = [("var big", "P", declared()), ("var small", "Q", declared())];
    let off = run_with_options(code, &seeds, DiscoveryOptions::default());
    assert_eq!(off.claim("var big"), Some(("P", Rule::Seed, 0)));
    assert_eq!(off.claim("var small"), Some(("Q", Rule::Seed, 0)));
    let capped = run_with_options(
        code,
        &seeds,
        DiscoveryOptions {
            declared_max_bytes: 1024,
            ..DiscoveryOptions::default()
        },
    );
    assert_eq!(capped.claim("var big"), None);
    assert_eq!(capped.claim("var small"), Some(("Q", Rule::Seed, 0)));
}

// ---- exp121 sponge rule ----------------------------------------------------

/// lib holds two inner functions, a and b, that cover most of it, and calls
/// tiny (under the row floor: DOWN takes it, and it keeps lib's seed off
/// the isolation veto); other is a unit of its own.
const COPIED: &str = "var lib = function (t) { var a = function (x) { return x.trim().toLowerCase().replace(/\\s+/g, \"-\").split(\"-\").filter(Boolean).slice(0, x.length); }; var b = function (y) { return [y, y + 1, y * 2, y % 7, String(y), y.toFixed(2), y.toString(16)].join(\",\"); }; return tiny(a(t) + b(t.length)); };\nvar tiny = (x) => x + 1;\nvar other = function (k) { return [k, k - 1, k * 3, String(k), k.toFixed(3), k.toString(8)].join(\";\"); };\nvar app = () => lib(\"a\") + other(1);\n";

#[test]
fn copy_presence_drops_a_package_whose_every_seed_is_an_inlined_copy_of_a_smaller_origin() {
    // S (a sponge) holds both of lib's inner functions in a 4,000-byte
    // module of its dist; Q holds one of them in a 500-byte module of its
    // own: S's copy of this code is Q's, inlined. Q alone covers too little
    // of lib to seed it.
    let held = [
        Held::inner("var lib", "function (x)", "S", 4000),
        Held::inner("var lib", "function (y)", "S", 4000),
        Held::inner("var lib", "function (x)", "Q", 500),
    ];
    let off = DiscoveryOptions::default();
    let (_, _, rows) = run_indexed_with(COPIED, &held, vec![], off);
    assert_eq!(claim_of(&rows, "var lib"), Some(("S".into(), Rule::Seed)));
    let on = DiscoveryOptions {
        copy_presence: 0.1,
        copy_presence_factor: 4,
        ..off
    };
    let (_, _, rows) = run_indexed_with(COPIED, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), None);
    // The co-holder's module is not the smaller context (2,000 x 4 > 4,000):
    // no origin, S keeps its seed.
    let bigger = [
        Held::inner("var lib", "function (x)", "S", 4000),
        Held::inner("var lib", "function (y)", "S", 4000),
        Held::inner("var lib", "function (x)", "Q", 2000),
    ];
    let (_, _, rows) = run_indexed_with(COPIED, &bigger, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("S".into(), Rule::Seed)));
    // A seed of S's own (code no smaller context holds) anywhere in the
    // bundle: S is present, every seed of it stays (other's own seed is
    // then lost to the isolation veto, as without the rule).
    let present = [
        Held::inner("var lib", "function (x)", "S", 4000),
        Held::inner("var lib", "function (y)", "S", 4000),
        Held::inner("var lib", "function (x)", "Q", 500),
        Held::whole("var other", "S"),
    ];
    let (_, _, rows) = run_indexed_with(COPIED, &present, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("S".into(), Rule::Seed)));
    // A scope sibling's own seed does not vouch for the sponge's copies:
    // judged per package, @acme/server is dropped though @acme/core (one
    // family) is present.
    let sibling = [
        Held::inner("var lib", "function (x)", "@acme/server", 4000),
        Held::inner("var lib", "function (y)", "@acme/server", 4000),
        Held::inner("var lib", "function (x)", "Q", 500),
        Held::whole("var other", "@acme/core"),
    ];
    let (_, _, rows) = run_indexed_with(COPIED, &sibling, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), None);
    // Under the share: Q co-holds a, about half of S's cover, under 0.9.
    let thin = DiscoveryOptions {
        copy_presence: 0.9,
        ..on
    };
    let (_, _, rows) = run_indexed_with(COPIED, &held, vec![], thin);
    assert_eq!(claim_of(&rows, "var lib"), Some(("S".into(), Rule::Seed)));
}

#[test]
fn copy_presence_origin_gives_a_sponge_s_copy_seeds_to_the_origin() {
    // As above, with the origin preferred: S's copy seed of lib becomes
    // Q's (the family co-holding most of S's cover in the smaller context),
    // and Q's claim propagates like any seed.
    let held = [
        Held::inner("var lib", "function (x)", "S", 4000),
        Held::inner("var lib", "function (y)", "S", 4000),
        Held::inner("var lib", "function (x)", "Q", 500),
    ];
    let on = DiscoveryOptions {
        copy_presence: 0.1,
        copy_presence_factor: 4,
        copy_presence_origin: true,
        presence_fence: true,
        ..DiscoveryOptions::default()
    };
    let (_, _, rows) = run_indexed_with(COPIED, &held, vec![], on);
    assert_eq!(claim_of(&rows, "var lib"), Some(("Q".into(), Rule::Seed)));
    assert_eq!(claim_of(&rows, "var tiny"), Some(("Q".into(), Rule::Down)));
}
