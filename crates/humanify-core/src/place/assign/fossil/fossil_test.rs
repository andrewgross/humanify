//! The TS spec (src/split/fossil-assign.test.ts), case for case. Bodies
//! come from the real inventory (program-body fixtures — no wrapper at
//! these sizes), hashes are the Rust statement hash (same partition).

use super::{FossilAssignment, FossilOptions, assign_fossil};
use crate::place::assign::namer::{SplitNameRequest, SplitNamer};
use crate::place::ledger::{FossilLedgerModule, StableSplitLedger};
use crate::place::trail::PlacementTrail;
use crate::twins::statement_inventory_with_values;
use serde_json::Value;

struct Body {
    values: Vec<Value>,
    spans: Vec<(u32, u32)>,
    hashes: Vec<String>,
}

fn body_of(lines: &[&str]) -> Body {
    let code = lines.join("\n");
    let (inv, values) = statement_inventory_with_values(&code, "shipped", None).expect("inventory");
    Body {
        values,
        spans: inv
            .statements
            .iter()
            .map(|s| (s.span.start, s.span.end))
            .collect(),
        hashes: inv.statements.iter().map(|s| s.hash.clone()).collect(),
    }
}

const ESM: &str = "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);";

fn bundle() -> Body {
    body_of(&[
        ESM,
        "function alphaCore(x) { return x + 1; }",
        "var alphaState;",
        "var init_alpha = __esm(() => { alphaState = alphaCore(1); });",
        "function betaRender(y) { return alphaState + y; }",
        "var betaCache;",
        "var init_beta = __esm(() => { init_alpha(); betaCache = betaRender(2); });",
        "console.log(init_beta);",
    ])
}

fn assign(b: &Body, prior: Option<&StableSplitLedger>) -> FossilAssignment {
    assign_fossil(
        &b.values,
        &b.spans,
        &b.hashes,
        prior,
        FossilOptions::default(),
    )
    .expect("assign")
}

fn ledger(modules: Vec<FossilLedgerModule>) -> StableSplitLedger {
    StableSplitLedger {
        version: 1,
        hash_version: Some(1),
        fossil_modules: Some(modules),
        ..StableSplitLedger::default()
    }
}

fn folder_of(f: &str) -> &str {
    &f[..f.rfind('/').unwrap_or(0)]
}

#[test]
fn assigns_each_module_to_one_file_and_the_eager_tail_to_bootstrap() {
    let b = bundle();
    let out = assign(&b, None);
    let a = &out.assignment;
    assert_eq!(a.len(), b.values.len());
    assert_eq!(a[2], a[1]);
    assert_eq!(a[3], a[1]);
    assert_ne!(a[1], a[4]);
    assert_eq!(a[6], a[4]);
    assert_eq!(a[7], "src/index.js");
    assert!(a[1].starts_with("src/") && a[1].ends_with("alpha-core.js"));
    assert_eq!(out.fossil_modules.len(), 2);
    assert_eq!(out.fossil_modules[0].file, a[1]);
}

#[test]
fn matched_modules_inherit_their_prior_path_verbatim() {
    let b = bundle();
    let first = assign(&b, None);
    let mut kept = first.fossil_modules[0].clone();
    kept.file = "src/legacy/kept-name.js".into();
    let prior = ledger(vec![kept, first.fossil_modules[1].clone()]);
    let second = assign(&b, Some(&prior));
    assert_eq!(second.assignment[1], "src/legacy/kept-name.js");
    assert_eq!(second.fossil_modules[0].file, "src/legacy/kept-name.js");
}

#[test]
fn throws_loudly_when_the_bundle_records_no_fossils() {
    let b = body_of(&["var a = 1;", "console.log(a);"]);
    let err = assign_fossil(
        &b.values,
        &b.spans,
        &b.hashes,
        None,
        FossilOptions::default(),
    )
    .expect_err("no fossils");
    assert!(err.contains("no module fossils"), "{err}");
}

#[test]
fn a_barrel_module_anchors_a_folder() {
    let b = body_of(&[
        ESM,
        "function leafAlphaThing(x) { return x; }",
        "var init_a = __esm(() => { leafAlphaThing(1); });",
        "function leafBetaThing(x) { return x; }",
        "var init_b = __esm(() => { leafBetaThing(2); });",
        "var init_bundle = __esm(() => { init_a(); init_b(); });",
        "function useAll() { return 1; }",
        "var init_main = __esm(() => { init_bundle(); useAll(); });",
    ]);
    let out = assign(&b, None);
    let a = &out.assignment;
    assert_eq!(folder_of(&a[1]), folder_of(&a[5]));
    assert_eq!(folder_of(&a[3]), folder_of(&a[5]));
}

#[test]
fn shared_modules_with_identical_importer_sets_group() {
    let b = body_of(&[
        ESM,
        "function sharedOne(x) { return x; }",
        "var init_s1 = __esm(() => { sharedOne(1); });",
        "function sharedTwo(x) { return x; }",
        "var init_s2 = __esm(() => { sharedTwo(2); });",
        "function sharedThree(x) { return x; }",
        "var init_s3 = __esm(() => { sharedThree(3); });",
        "function consumerA() { return 1; }",
        "var init_ca = __esm(() => { init_s1(); init_s2(); init_s3(); consumerA(); });",
        "function consumerB() { return 2; }",
        "var init_cb = __esm(() => { init_s1(); init_s2(); init_s3(); consumerB(); });",
    ]);
    let out = assign(&b, None);
    let a = &out.assignment;
    assert_eq!(folder_of(&a[1]), folder_of(&a[3]));
    assert_ne!(folder_of(&a[1]), "src");
}

#[test]
fn a_flat_file_whose_importers_agree_moves_in_with_them() {
    let b = body_of(&[
        ESM,
        "function helper(x) { return x; }",
        "var init_h = __esm(() => { helper(0); });",
        "function leafA() { return 1; }",
        "var init_a = __esm(() => { init_h(); leafA(); });",
        "function leafB() { return 2; }",
        "var init_b = __esm(() => { init_h(); leafB(); });",
        "function leafC() { return 3; }",
        "var init_c = __esm(() => { init_h(); leafC(); });",
        "function anchor() { return 4; }",
        "var init_anchor = __esm(() => { init_a(); init_b(); init_c(); anchor(); });",
    ]);
    let out = assign(&b, None);
    let a = &out.assignment;
    assert_eq!(folder_of(&a[1]), folder_of(&a[3]));
    assert_ne!(folder_of(&a[1]), "src");
}

fn hoist_src() -> Body {
    body_of(&[
        ESM,
        "function leafA() { return 1; }",
        "var init_a = __esm(() => { leafA(); });",
        "function leafB() { return 2; }",
        "var init_b = __esm(() => { leafB(); });",
        "function leafC() { return 3; }",
        "var init_c = __esm(() => { leafC(); });",
        "function anchorMod() { return 4; }",
        "var init_anchor = __esm(() => { init_a(); init_b(); init_c(); anchorMod(); });",
    ])
}

#[test]
fn a_fresh_file_alone_in_its_folder_is_hoisted() {
    let b = hoist_src();
    let first = assign(&b, None);
    let carried: Vec<FossilLedgerModule> = first.fossil_modules[..3]
        .iter()
        .enumerate()
        .map(|(i, m)| FossilLedgerModule {
            file: format!("src/carried/mod-{i}.js"),
            ..m.clone()
        })
        .collect();
    let out = assign(&b, Some(&ledger(carried)));
    assert_eq!(out.assignment[7], "src/anchor-mod.js");
    assert_eq!(out.stats.hoisted_singletons, 1);
}

#[test]
fn never_hoists_an_inherited_path() {
    let b = hoist_src();
    let first = assign(&b, None);
    let lonely: Vec<FossilLedgerModule> = first
        .fossil_modules
        .iter()
        .enumerate()
        .map(|(i, m)| FossilLedgerModule {
            file: format!("src/lonely-{i}/mod-{i}.js"),
            ..m.clone()
        })
        .collect();
    let out = assign(&b, Some(&ledger(lonely)));
    assert_eq!(out.assignment[1], "src/lonely-0/mod-0.js");
    assert_eq!(out.stats.hoisted_singletons, 0);
}

#[test]
fn keeps_the_folder_when_hoisting_would_collide() {
    let b = hoist_src();
    let first = assign(&b, None);
    let modules: Vec<FossilLedgerModule> = first
        .fossil_modules
        .iter()
        .enumerate()
        .map(|(i, m)| match i {
            3 => FossilLedgerModule {
                hashes: vec!["no-match-1".into(), "no-match-2".into()],
                ..m.clone()
            },
            0 => FossilLedgerModule {
                file: "src/anchor-mod.js".into(),
                ..m.clone()
            },
            _ => FossilLedgerModule {
                file: format!("src/carried/mod-{i}.js"),
                ..m.clone()
            },
        })
        .collect();
    let out = assign(&b, Some(&ledger(modules)));
    assert_eq!(out.assignment[7], "src/anchor-mod/anchor-mod.js");
    assert_eq!(out.stats.hoisted_singletons, 0);
}

fn trail_run(b: &Body, prior: &StableSplitLedger) -> PlacementTrail {
    let mut trail = PlacementTrail::default();
    assign_fossil(
        &b.values,
        &b.spans,
        &b.hashes,
        Some(prior),
        FossilOptions {
            trail: Some(&mut trail),
            ..FossilOptions::default()
        },
    )
    .expect("assign");
    trail
}

#[test]
fn the_trail_records_every_statement_with_hash_keyed_prior_files() {
    let b = bundle();
    let first = assign(&b, None);
    let mut kept = first.fossil_modules[0].clone();
    kept.file = "src/legacy/kept-name.js".into();
    let prior = ledger(vec![kept, first.fossil_modules[1].clone()]);
    let trail = trail_run(&b, &prior);
    for i in 1..b.values.len() {
        assert!(
            trail.rows.iter().any(|r| r.index == i as u64),
            "statement {i} missing"
        );
    }
    let a = trail.rows.iter().find(|r| r.index == 1).expect("row 1");
    assert_eq!(a.file, "src/legacy/kept-name.js");
    assert!(a.placed_by.starts_with("fossil:"), "{}", a.placed_by);
    assert_eq!(a.prior_file.as_deref(), Some("src/legacy/kept-name.js"));
    assert_eq!(a.prior_file_from.as_deref(), Some("hash"));
    let last = (b.values.len() - 1) as u64;
    let eager = trail.rows.iter().find(|r| r.index == last).expect("eager");
    assert_eq!(eager.file, "src/index.js");
    assert_eq!(eager.placed_by, "fossil-eager");
}

#[test]
fn a_statement_whose_hash_lived_in_another_prior_module_reads_as_a_move() {
    let b = bundle();
    let first = assign(&b, None);
    let moved_hash = b.hashes[4].clone();
    let mut m0 = first.fossil_modules[0].clone();
    m0.hashes.push(moved_hash.clone());
    m0.file = "src/alpha.js".into();
    let mut m1 = first.fossil_modules[1].clone();
    m1.hashes.retain(|h| *h != moved_hash);
    m1.file = "src/beta.js".into();
    let trail = trail_run(&b, &ledger(vec![m0, m1]));
    let moved = trail.rows.iter().find(|r| r.index == 4).expect("row 4");
    assert_eq!(moved.prior_file.as_deref(), Some("src/alpha.js"));
    assert_ne!(moved.prior_file.as_deref(), Some(moved.file.as_str()));
}

/// A namer that records every request and answers with a fixed proposal.
struct FixedNamer {
    answer: Option<String>,
    asked: Vec<SplitNameRequest>,
}

impl SplitNamer for FixedNamer {
    fn name(&mut self, requests: &[SplitNameRequest]) -> Vec<Option<String>> {
        self.asked.extend(requests.iter().cloned());
        requests.iter().map(|_| self.answer.clone()).collect()
    }
}

fn assign_named(b: &Body, prior: &StableSplitLedger, namer: &mut FixedNamer) -> FossilAssignment {
    assign_fossil(
        &b.values,
        &b.spans,
        &b.hashes,
        Some(prior),
        FossilOptions {
            mint_namer: Some(namer),
            ..FossilOptions::default()
        },
    )
    .expect("assign")
}

#[test]
fn names_an_unmatched_module_from_the_namer_and_never_asks_for_matched_ones() {
    let b = bundle();
    let first = assign(&b, None);
    let prior = ledger(vec![first.fossil_modules[0].clone()]);
    let mut namer = FixedNamer {
        answer: Some("tenguFeatureFlags".into()),
        asked: Vec::new(),
    };
    let out = assign_named(&b, &prior, &mut namer);
    assert_eq!(out.assignment[1], first.fossil_modules[0].file);
    assert!(
        out.assignment[4].ends_with("tengu-feature-flags.js"),
        "{}",
        out.assignment[4]
    );
    assert_eq!(namer.asked.len(), 1);
    assert!(namer.asked[0].bindings.contains(&"betaRender".to_string()));
    assert_eq!(out.stats.llm_named_mints, 1);
}

#[test]
fn a_null_proposal_keeps_the_mechanical_stem() {
    let b = bundle();
    let first = assign(&b, None);
    let prior = ledger(vec![first.fossil_modules[0].clone()]);
    let mut namer = FixedNamer {
        answer: None,
        asked: Vec::new(),
    };
    let out = assign_named(&b, &prior, &mut namer);
    assert!(out.assignment[4].ends_with("beta-render.js"));
    assert_eq!(out.stats.llm_named_mints, 0);
}

#[test]
fn a_colliding_proposal_falls_back_to_the_mechanical_stem() {
    let b = bundle();
    let first = assign(&b, None);
    let kept = &first.fossil_modules[0].file;
    let base = &kept[kept.rfind('/').unwrap() + 1..];
    // kebab → camel, as the TS test derives it.
    let mut camel = String::new();
    let mut up = false;
    for c in base.trim_end_matches(".js").chars() {
        if c == '-' {
            up = true;
        } else if up {
            camel.extend(c.to_uppercase());
            up = false;
        } else {
            camel.push(c);
        }
    }
    let prior = ledger(vec![first.fossil_modules[0].clone()]);
    let mut namer = FixedNamer {
        answer: Some(camel),
        asked: Vec::new(),
    };
    let out = assign_named(&b, &prior, &mut namer);
    assert!(
        out.assignment[4].ends_with("beta-render.js"),
        "{}",
        out.assignment[4]
    );
    assert!(!out.assignment[4].ends_with("-2.js"));
}

#[test]
fn mint_siblings_are_the_mints_own_folder_stems() {
    let b = bundle();
    let first = assign(&b, None);
    let mut kept = first.fossil_modules[0].clone();
    kept.file = "src/legacy/kept-name.js".into();
    let mut namer = FixedNamer {
        answer: None,
        asked: Vec::new(),
    };
    assign_named(&b, &ledger(vec![kept]), &mut namer);
    assert_eq!(namer.asked.len(), 1);
    assert!(namer.asked[0].siblings.len() <= 24);
    assert!(!namer.asked[0].siblings.contains(&"kept-name".to_string()));
}
