//! Library discovery over small synthetic bundles: each rule of the
//! fixpoint, the two guards, DOWN-CLASS and the candidate queue
//! (docs/design/library-discovery.md §5–§7).

use std::collections::HashSet;

use oxc_allocator::Allocator;

use crate::graph::references::{BundlerFacts, ReferenceGraph, build_reference_graph};
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
    // Unidentified app code outside the module calls it: not filled.
    assert_eq!(r.claim("var calledByApp"), None);
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
    assert_eq!(r.claim("var c"), None);
    assert_eq!(r.claim("var d"), Some(("other", Rule::Seed, 0)));
}

#[test]
fn the_content_floor_keeps_a_content_free_claim_from_owning_app_code() {
    let r = run(
        "var ns = {};\nrt(ns, { f: () => appModuleThing() });\nvar appModuleThing = () => \"app\";\nvar big = () => depOfBig() + \"padding padding padding\";\nvar depOfBig = () => 1;\nvar app = () => depOfBig() + big() + ns.f();\n",
        &[
            ("var ns", "pkgA", declared()),
            ("var big", "pkgB", declared()),
        ],
    );
    let class: Vec<u32> = r.found.library_class.keys().copied().collect();
    assert_eq!(class, vec![r.unit("var depOfBig")]);
    assert_eq!(r.found.library_class[&r.unit("var depOfBig")], "pkgB");
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
                Evidence::Banner => "banner".into(),
                Evidence::Index => "index".into(),
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
