//! The TS spec (src/split/fossil-assign.test.ts), case for case. Bodies
//! come from the real inventory (program-body fixtures — no wrapper at
//! these sizes), hashes are the Rust statement hash (same partition).

use super::{FossilAssignment, FossilOptions, assign_fossil};
use crate::hash::forms::STATEMENT;
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
    let (inv, values) = statement_inventory_with_values(
        &code,
        "shipped",
        None,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .expect("inventory");
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
        hash_version: Some(STATEMENT.version),
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
fn a_statement_whose_hash_lived_in_another_prior_module_stays_there() {
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
    // The statement's module is beta's, but the statement itself is
    // unchanged and lived in alpha's file: it stays there (exp226).
    let kept = trail.rows.iter().find(|r| r.index == 4).expect("row 4");
    assert_eq!(kept.prior_file.as_deref(), Some("src/alpha.js"));
    assert_eq!(kept.file, "src/alpha.js");
    assert_eq!(kept.placed_by, "fossil:kept-statement");
    let init = trail.rows.iter().find(|r| r.index == 6).expect("row 6");
    assert_eq!(init.file, "src/beta.js");
}

/// The names mode's pre-filled modules (library-names plan step 4,
/// exp156) record their OWN trail tier: the trail is the one record of
/// what the run itself moved, so a run can state its placed bytes without
/// re-deriving the layout.
#[test]
fn the_trail_records_library_prefilled_modules_as_library_index() {
    let b = bundle();
    let mut trail = PlacementTrail::default();
    assign_fossil(
        &b.values,
        &b.spans,
        &b.hashes,
        None,
        FossilOptions {
            trail: Some(&mut trail),
            library: &[(0, "vendor/zod/index.js".to_string())],
            ..FossilOptions::default()
        },
    )
    .expect("assign");
    // Module 0 is statements 0-3, the __esm helper declaration included
    // (the fossil extract's partition of the whole body).
    for i in 0..=3u64 {
        let row = trail
            .rows
            .iter()
            .find(|r| r.index == i)
            .expect("module 0 row");
        assert_eq!(row.placed_by, "library-index", "statement {i}");
        assert_eq!(row.file, "vendor/zod/index.js", "statement {i}");
    }
    assert_eq!(
        trail
            .rows
            .iter()
            .filter(|r| r.placed_by == "library-index")
            .count(),
        4
    );
    // Nothing else inherits the tier: the beta module (statements 4-6)
    // still records its fresh fossil names.
    for i in 4..=6u64 {
        let beta = trail.rows.iter().find(|r| r.index == i).expect("beta row");
        assert_eq!(beta.placed_by, "fossil-fresh", "statement {i}");
    }
    let eager = trail.rows.iter().find(|r| r.index == 7).expect("eager");
    assert_eq!(eager.placed_by, "fossil-eager");
}

/// Two modules whose wrappers carry module names (the naming stage's
/// module step, docs/design/module-naming.md): the first is helper-first
/// (`roundToNearest` leads a color module), the second imports it.
fn module_named_bundle() -> Body {
    body_of(&[
        ESM,
        "function roundToNearest(x) { return Math.round(x); }",
        "var spinnerGlyphs;",
        "var initColorUtils = __esm(() => { spinnerGlyphs = roundToNearest(1); });",
        "function themeRender(y) { return spinnerGlyphs + y; }",
        "var themeCache;",
        "var initThemePicker = __esm(() => { initColorUtils(); themeCache = themeRender(2); });",
        "console.log(initThemePicker);",
    ])
}

#[test]
fn a_module_named_wrapper_names_its_file_and_its_folder() {
    let b = module_named_bundle();
    let out = assign(&b, None);
    assert_eq!(out.assignment[1], "src/theme-picker/color-utils.js");
    assert_eq!(out.assignment[4], "src/theme-picker/theme-picker.js");
    assert_eq!(out.stats.module_named_files, 2);
}

#[test]
fn the_ledger_records_each_modules_mechanical_stem() {
    let out = assign(&module_named_bundle(), None);
    let stems: Vec<Option<&str>> = out
        .fossil_modules
        .iter()
        .map(|m| m.mechanical_stem.as_deref())
        .collect();
    assert_eq!(stems, vec![Some("round-to-nearest"), Some("theme-render")]);
}

#[test]
fn an_inherited_path_is_never_renamed_by_a_module_name() {
    let b = module_named_bundle();
    let first = assign(&b, None);
    let mut kept = first.fossil_modules[1].clone();
    kept.file = "src/legacy/old-theme.js".into();
    let out = assign(&b, Some(&ledger(vec![kept])));
    assert_eq!(out.assignment[4], "src/legacy/old-theme.js");
    assert_eq!(out.stats.inherited_files, 1);
    // The unmatched module still takes its module name.
    assert!(
        out.assignment[1].ends_with("/color-utils.js"),
        "{}",
        out.assignment[1]
    );
}

#[test]
fn the_same_stem_match_reads_the_priors_recorded_mechanical_stem() {
    // The prior's file carries a module name, its ledger the first-function
    // stem: the same-file-name tier must still pair the module (a file stem
    // comparison would never agree once files carry module names).
    let b = bundle();
    let first = assign(&b, None);
    let mut prior = first.fossil_modules[1].clone();
    prior.file = "src/rendering/beta-view.js".into();
    prior.mechanical_stem = Some("beta-render".into());
    prior.hashes.push("0000extra".into());
    prior.declared = None;
    prior.tokens = None;
    let out = assign(&b, Some(&ledger(vec![prior])));
    assert_eq!(out.assignment[4], "src/rendering/beta-view.js");
    assert!(
        out.stats
            .match_tiers
            .contains(&("stem-corroborated".to_string(), 1)),
        "{:?}",
        out.stats.match_tiers
    );
}

#[test]
fn a_ts_era_ledgers_fossil_modules_are_refused() {
    // The prior's fossil modules carry TS statement-hash bytes
    // (hashVersion 1): never matched against the Rust's own hashes, even
    // when the bytes coincide — the module then places as with no prior
    // instead of inheriting `kept-name.js`.
    let b = bundle();
    let first = assign(&b, None);
    let mut kept = first.fossil_modules[0].clone();
    kept.file = "src/legacy/kept-name.js".into();
    let mut prior = ledger(vec![kept, first.fossil_modules[1].clone()]);
    prior.hash_version = Some(1);
    let second = assign(&b, Some(&prior));
    assert_ne!(second.assignment[1], "src/legacy/kept-name.js");
    assert_eq!(second.assignment, first.assignment);
}

/// File names never come from a library import (finding #91): a module
/// whose first declaration is `pathModule23 = require("path")` is named
/// after its next declaration, never `path-module23.js`.
#[test]
fn a_library_import_never_names_a_module() {
    let b = body_of(&[
        ESM,
        "function setupCore() { return 1; }",
        "var init_core = __esm(() => { setupCore(); });",
        "var pathModule23, baseSettingsPath;",
        "var init_settings = __esm(() => { pathModule23 = require(\"path\"); baseSettingsPath = pathModule23.join(\"a\", \"b\"); });",
        "var fsModule2;",
        "var init_fs = __esm(() => { init_settings(); fsModule2 = require(\"node:fs\"); });",
        "console.log(init_fs);",
    ]);
    let out = assign(&b, None);
    let a = &out.assignment;
    let base = |f: &str| f[f.rfind('/').map_or(0, |i| i + 1)..].to_string();
    assert_eq!(base(&a[3]), "base-settings-path.js", "{a:?}");
    // Every declaration but the init a library import: the init names it.
    assert_eq!(base(&a[5]), "init-fs.js", "{a:?}");
}

// ---- an unchanged module keeps its prior file when upstream splits it (exp215)

/// v1: an init-less group (`groupRead`, `groupWrite`) is glued to the
/// line module that follows it — the markers record ONE module.
fn glued_v1() -> Body {
    body_of(&[
        ESM,
        "function groupRead(a) { return a.read(); }",
        "function groupWrite(a, b) { return a.write(b); }",
        "class LineCore { decode(x) { return x.split(\"\\n\"); } }",
        "var lineState;",
        "var init_line = __esm(() => { lineState = new WeakMap(); LineCore.CHARS = 2; });",
        "function betaRender(y) { return lineState + y; }",
        "var betaCache;",
        "var init_beta = __esm(() => { init_line(); betaCache = betaRender(2); });",
        "console.log(init_beta);",
    ])
}

/// v2: upstream gave the group its own init (and one new function): the
/// markers now record two modules where the prior had one. Every
/// statement of the line module is byte-for-byte the prior's.
fn glued_v2() -> Body {
    body_of(&[
        ESM,
        "function groupRead(a) { return a.read(); }",
        "function groupWrite(a, b) { return a.write(b); }",
        "function groupQuery(q) { return Object.keys(q).join(\"&\"); }",
        "var init_group = __esm(() => { groupQuery({}); });",
        "class LineCore { decode(x) { return x.split(\"\\n\"); } }",
        "var lineState;",
        "var init_line = __esm(() => { lineState = new WeakMap(); LineCore.CHARS = 2; });",
        "function betaRender(y) { return lineState + y; }",
        "var betaCache;",
        "var init_beta = __esm(() => { init_line(); betaCache = betaRender(2); });",
        "console.log(init_beta);",
    ])
}

fn glued_prior() -> StableSplitLedger {
    let first = assign(&glued_v1(), None);
    assert_eq!(first.fossil_modules.len(), 2, "v1 records the glued module");
    let mut glued = first.fossil_modules[0].clone();
    glued.file = "src/legacy/runtime.js".into();
    let mut beta = first.fossil_modules[1].clone();
    beta.file = "src/legacy/beta.js".into();
    ledger(vec![glued, beta])
}

#[test]
fn a_module_split_out_of_a_prior_module_keeps_the_prior_file() {
    let out = assign(&glued_v2(), Some(&glued_prior()));
    let a = &out.assignment;
    // Both halves of the prior module stay where the prior put them: the
    // group with its new function and init, and the unchanged line module.
    for i in [1, 2, 3, 4, 5, 6, 7] {
        assert_eq!(a[i], "src/legacy/runtime.js", "statement {i}: {a:?}");
    }
    assert_eq!(a[8], "src/legacy/beta.js");
    assert_eq!(out.stats.fresh_named_files, 0, "{:?}", out.stats);
    // The ledger records both modules at the one file, so the next hop
    // matches each to itself.
    let files: Vec<&str> = out.fossil_modules.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(
        files,
        vec![
            "src/legacy/runtime.js",
            "src/legacy/runtime.js",
            "src/legacy/beta.js"
        ]
    );
}

#[test]
fn the_kept_file_holds_on_the_next_hop() {
    let second = assign(&glued_v2(), Some(&glued_prior()));
    let third = assign(&glued_v2(), Some(&ledger(second.fossil_modules.clone())));
    assert_eq!(third.assignment, second.assignment);
}

#[test]
fn a_module_whose_carried_statements_came_from_two_prior_files_is_named_fresh() {
    // The prior held groupRead in one file and groupWrite in another: the
    // carried statements disagree, so no prior file is the module's.
    let b = glued_v2();
    let mut prior = glued_prior();
    let moved = b.hashes[2].clone();
    let modules = prior.fossil_modules.as_mut().expect("modules");
    modules[0].hashes.retain(|h| *h != moved);
    modules[1].hashes.push(moved);
    let out = assign(&b, Some(&prior));
    let a = &out.assignment;
    // The module is named fresh: its new function and its init go there.
    let fresh = a[3].as_str();
    assert_ne!(fresh, "src/legacy/runtime.js", "{a:?}");
    assert_ne!(fresh, "src/legacy/beta.js", "{a:?}");
    assert_eq!(a[4], fresh, "{a:?}");
    // Each carried statement stays in the file it lived in (exp226).
    assert_eq!(a[1], "src/legacy/runtime.js", "{a:?}");
    assert_eq!(a[2], "src/legacy/beta.js", "{a:?}");
}

#[test]
fn a_mostly_new_module_is_named_fresh() {
    // One carried statement among four is not the module's file.
    let b = body_of(&[
        ESM,
        "function groupRead(a) { return a.read(); }",
        "function freshOne(q) { return q + 1; }",
        "function freshTwo(q) { return q * 2; }",
        "var init_group = __esm(() => { freshOne(1); });",
        "class LineCore { decode(x) { return x.split(\"\\n\"); } }",
        "var lineState;",
        "var init_line = __esm(() => { lineState = new WeakMap(); LineCore.CHARS = 2; });",
        "function betaRender(y) { return lineState + y; }",
        "var betaCache;",
        "var init_beta = __esm(() => { init_line(); betaCache = betaRender(2); });",
        "console.log(init_beta);",
    ]);
    let out = assign(&b, Some(&glued_prior()));
    let a = &out.assignment;
    // The module's file is fresh; its one carried statement stays (exp226).
    for i in [2, 3, 4] {
        assert_ne!(a[i], "src/legacy/runtime.js", "statement {i}: {a:?}");
    }
    assert_eq!(a[1], "src/legacy/runtime.js", "{a:?}");
    assert_eq!(a[5], "src/legacy/runtime.js");
}

// ---- an unchanged statement keeps its prior file when its run is glued
// ---- to another module (exp226)

/// The prior ledger as the split writes it: the modules AND the
/// per-statement layout (`hashes` + `order`, parallel to the body), with
/// each fresh file renamed through `rename`.
fn written_ledger(b: &Body, out: &FossilAssignment, rename: &[(&str, &str)]) -> StableSplitLedger {
    let to = |f: &str| -> String {
        rename
            .iter()
            .find(|(from, _)| *from == f)
            .map_or(f.to_string(), |(_, to)| to.to_string())
    };
    let modules = out
        .fossil_modules
        .iter()
        .map(|m| FossilLedgerModule {
            file: to(&m.file),
            ..m.clone()
        })
        .collect();
    StableSplitLedger {
        order: out.assignment.iter().map(|f| to(f)).collect(),
        hashes: Some(b.hashes.clone()),
        ..ledger(modules)
    }
}

/// v1: two init-less helpers sit before the image module and are glued to
/// it; the limits module lives elsewhere in the bundle.
fn image_v1() -> Body {
    body_of(&[
        ESM,
        "function limitTiles(n) { return n * 4; }",
        "var limitMax;",
        "var init_limits = __esm(() => { limitMax = limitTiles(8); });",
        "function mimeOf(bytes) { return bytes[0] === 137 ? \"png\" : \"jpeg\"; }",
        "function dimensionsOf(bytes) { return { w: bytes[16], h: bytes[20] }; }",
        "function resizeImage(img) { return mimeOf(img) + dimensionsOf(img).w; }",
        "var resizeState;",
        "var init_resize = __esm(() => { init_limits(); resizeState = resizeImage([137]); });",
        "console.log(init_resize);",
    ])
}

/// v2: the bundler now emits the limits module between the helpers and
/// the image module (a new import moved it up), so the markers glue the
/// unchanged helpers to the limits module. Every statement is the
/// prior's, byte for byte.
fn image_v2() -> Body {
    body_of(&[
        ESM,
        "function mimeOf(bytes) { return bytes[0] === 137 ? \"png\" : \"jpeg\"; }",
        "function dimensionsOf(bytes) { return { w: bytes[16], h: bytes[20] }; }",
        "function limitTiles(n) { return n * 4; }",
        "var limitMax;",
        "var init_limits = __esm(() => { limitMax = limitTiles(8); });",
        "function resizeImage(img) { return mimeOf(img) + dimensionsOf(img).w; }",
        "var resizeState;",
        "var init_resize = __esm(() => { init_limits(); resizeState = resizeImage([137]); });",
        "console.log(init_resize);",
    ])
}

fn image_prior() -> StableSplitLedger {
    let b = image_v1();
    let first = assign(&b, None);
    let files: Vec<String> = first
        .fossil_modules
        .iter()
        .map(|m| m.file.clone())
        .collect();
    written_ledger(
        &b,
        &first,
        &[
            (files[0].as_str(), "src/media/limits.js"),
            (files[1].as_str(), "src/media/resize.js"),
        ],
    )
}

#[test]
fn an_unchanged_run_glued_to_another_module_stays_in_its_prior_file() {
    let out = assign(&image_v2(), Some(&image_prior()));
    let a = &out.assignment;
    assert_eq!(a[1], "src/media/resize.js", "{a:?}");
    assert_eq!(a[2], "src/media/resize.js", "{a:?}");
    for i in [3, 4, 5] {
        assert_eq!(a[i], "src/media/limits.js", "statement {i}: {a:?}");
    }
    for i in [6, 7, 8] {
        assert_eq!(a[i], "src/media/resize.js", "statement {i}: {a:?}");
    }
    assert_eq!(out.stats.kept_statements, 2, "{:?}", out.stats);
}

#[test]
fn a_kept_statement_holds_on_the_next_hop() {
    let b = image_v2();
    let second = assign(&b, Some(&image_prior()));
    let third = assign(&b, Some(&written_ledger(&b, &second, &[])));
    assert_eq!(third.assignment, second.assignment);
    assert_eq!(third.stats.kept_statements, 2, "{:?}", third.stats);
}

#[test]
fn a_statement_never_returns_to_a_vendor_file() {
    let b = image_v1();
    let first = assign(&b, None);
    let files: Vec<String> = first
        .fossil_modules
        .iter()
        .map(|m| m.file.clone())
        .collect();
    let mut prior = written_ledger(
        &b,
        &first,
        &[
            (files[0].as_str(), "src/media/limits.js"),
            (files[1].as_str(), "src/media/resize.js"),
        ],
    );
    // The prior filed the two helpers under vendor/ (a library claim).
    prior.order[4] = "vendor/pkg/index.js".into();
    prior.order[5] = "vendor/pkg/index.js".into();
    let out = assign(&image_v2(), Some(&prior));
    assert_eq!(
        out.assignment[1], "src/media/limits.js",
        "{:?}",
        out.assignment
    );
    assert_eq!(out.stats.kept_statements, 0, "{:?}", out.stats);
}

#[test]
fn the_trail_records_a_kept_statement_with_its_own_tier() {
    let trail = trail_run(&image_v2(), &image_prior());
    let row = trail.rows.iter().find(|r| r.index == 1).expect("row 1");
    assert_eq!(row.file, "src/media/resize.js");
    assert_eq!(row.placed_by, "fossil:kept-statement");
    assert_eq!(row.prior_file.as_deref(), Some("src/media/resize.js"));
}

#[test]
fn a_statement_whose_shape_this_bundle_holds_twice_is_not_refiled() {
    // A new function with the same rename-blind shape as a carried one:
    // the hash no longer says which statement the prior's was, so neither
    // copy is filed by it.
    let b = body_of(&[
        ESM,
        "function mimeOf(bytes) { return bytes[0] === 137 ? \"png\" : \"jpeg\"; }",
        "function dimensionsOf(bytes) { return { w: bytes[16], h: bytes[20] }; }",
        "function limitTiles(n) { return n * 4; }",
        "function tileKind(t) { return t[0] === 137 ? \"png\" : \"jpeg\"; }",
        "var limitMax;",
        "var init_limits = __esm(() => { limitMax = limitTiles(8); });",
        "function resizeImage(img) { return mimeOf(img) + dimensionsOf(img).w; }",
        "var resizeState;",
        "var init_resize = __esm(() => { init_limits(); resizeState = resizeImage([137]); });",
        "console.log(init_resize);",
    ]);
    assert_eq!(b.hashes[1], b.hashes[4], "the two functions share a shape");
    let out = assign(&b, Some(&image_prior()));
    let a = &out.assignment;
    assert_eq!(a[1], "src/media/limits.js", "{a:?}");
    assert_eq!(a[4], "src/media/limits.js", "{a:?}");
    assert_eq!(a[2], "src/media/resize.js", "{a:?}");
    assert_eq!(out.stats.kept_statements, 1, "{:?}", out.stats);
}
