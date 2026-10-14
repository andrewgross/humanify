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
}

impl<'h> Held<'h> {
    fn whole(head: &'h str, owner: &'h str) -> Held<'h> {
        Held {
            head,
            owner,
            function: false,
            module_bytes: 500,
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
            .unwrap_or_else(|| panic!("no unit {:?}", h.head));
        let print = if h.function {
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
        .unwrap_or_else(|| panic!("no unit {head:?}"))
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
