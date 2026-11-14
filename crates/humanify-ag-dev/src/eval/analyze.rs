//! ONE scorecard per pair (TS: `034/analyze.ts`): the determinism breakdown
//! from the pipeline's `--stats-json`, the statement churn of the bundle
//! (rename-invariant: a hash present on both sides is structurally
//! unchanged, so any text difference is naming noise; a new hash is real
//! change), relocation and tree churn from the split ledgers, the on-disk
//! diff of `src/` (layout) and of `vendor/`, and every KPI split by surface.
//! Every card carries `scorerVersion` (scorer v3: the KPI statement form
//! over oxc, lines counted by the minimal diff; `card::SCORER_VERSION`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use humanify_ag_model::js::JsValue;

use super::card::{SCORER_VERSION, count, fixed, int, obj, text, texts};
use super::compose::{Composition, compose_diff};
use super::difftool::Differ;
use super::line_churn::{build_constant_churn, name_only_churn};
use super::stmts::{Stmt, bundle_statements};
use super::surfaces::{
    AppLines, CrossLines, LayoutIn, Ledger, MintCounts, NameCounts, RelocCounts, Split3,
    StatementCounts, VendorLines, boundary_lines, layout_in, mint_split, name_split, reloc_split,
    statement_kinds, statement_split, unique_statement_pairs,
};
use super::vendor::{VendorChurn, decompose};

/// What one pair is scored from: two emitted trees and the fresh run's
/// stats record.
#[derive(Clone, Debug)]
pub struct PairInputs {
    pub pair: String,
    pub fresh_tree: PathBuf,
    pub prior_tree: PathBuf,
    /// The fresh run's `--stats-json` (only its `coverage` block is read).
    pub stats: PathBuf,
}

/// The determinism breakdown from the stats' coverage block.
fn determinism(stats: &serde_json::Value) -> JsValue {
    let f = &stats["coverage"]["functions"];
    let mb = &stats["coverage"]["moduleBindings"];
    let n = |v: &serde_json::Value, k: &str| v[k].as_f64().unwrap_or(0.0);
    let total = n(f, "total");
    let det = n(f, "cached") + n(f, "alreadyNamed") + n(f, "nothingToRename");
    let llm = n(f, "llm");
    let pct = |x: f64| {
        if total != 0.0 {
            fixed(100.0 * x / total, 2)
        } else {
            JsValue::Number(0.0)
        }
    };
    obj(vec![
        (
            "functions",
            obj(vec![
                ("total", JsValue::Number(total)),
                ("deterministic", JsValue::Number(det)),
                ("closeMatchLLM", JsValue::Number(n(f, "closeMatch"))),
                ("coldLLM", JsValue::Number(llm - n(f, "closeMatch"))),
                ("failed", JsValue::Number(n(f, "failed"))),
                ("pctDeterministic", pct(det)),
                ("pctReachingLLM", pct(llm)),
            ]),
        ),
        (
            "moduleBindings",
            obj(vec![
                ("total", JsValue::Number(n(mb, "total"))),
                ("cached", JsValue::Number(n(mb, "cached"))),
                ("llm", JsValue::Number(n(mb, "llm"))),
            ]),
        ),
        (
            "mintedLeftovers",
            JsValue::Number(
                stats["coverage"]["mintedCensus"]["total"]
                    .as_f64()
                    .unwrap_or(0.0),
            ),
        ),
    ])
}

fn statement_block(c: &StatementCounts) -> Vec<(&'static str, JsValue)> {
    vec![
        ("total", count(c.total)),
        ("unchangedClean", count(c.unchanged_clean)),
        ("unchangedChurned", count(c.unchanged_churned)),
        ("novel", count(c.novel)),
        ("namingNoiseLines", count(c.naming_noise_lines)),
        ("realLines", count(c.real_lines)),
    ]
}

/// Same-name bindings whose home file changed, and names new to the prior
/// ledger (name-keyed: `nameToFiles[name][0]`).
fn relocations(fresh: &Ledger, prior: &Ledger) -> JsValue {
    let pf: HashMap<&str, &str> = prior
        .name_to_files
        .iter()
        .filter_map(|(k, v)| Some((k.as_str(), v.first()?.as_str())))
        .collect();
    let (mut moved, mut novel, mut names) = (0, 0, 0);
    for (name, files) in &fresh.name_to_files {
        let Some(file) = files.first() else { continue };
        names += 1;
        match pf.get(name.as_str()) {
            None => novel += 1,
            Some(p) if p != file => moved += 1,
            Some(_) => {}
        }
    }
    obj(vec![
        ("sameNameMovedFile", count(moved)),
        ("novelNames", count(novel)),
        ("freshNames", count(names)),
    ])
}

/// Tree churn from the ledgers: 1:1 hash-identified statements that
/// changed file, and file renames by majority hash overlap.
fn tree_churn(fresh: &Ledger, prior: &Ledger) -> JsValue {
    let empty = Vec::new();
    let fo = fresh.order.as_ref().unwrap_or(&empty);
    let po = prior.order.as_ref().unwrap_or(&empty);
    let pairs = unique_statement_pairs(fresh, prior);
    let mut relocated = 0;
    let mut by_pair: Vec<(String, usize)> = Vec::new();
    for (p, f) in &pairs {
        if p != f {
            relocated += 1;
            let key = format!("{p} -> {f}");
            match by_pair.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 += 1,
                None => by_pair.push((key, 1)),
            }
        }
    }
    let uniq = |xs: &[String]| {
        let mut seen = std::collections::HashSet::new();
        xs.iter()
            .filter(|x| seen.insert(x.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    };
    let (fresh_files, prior_files) = (uniq(fo), uniq(po));
    let added: Vec<&String> = fresh_files
        .iter()
        .filter(|f| !prior_files.contains(f))
        .collect();
    let removed: Vec<&String> = prior_files
        .iter()
        .filter(|f| !fresh_files.contains(f))
        .collect();
    let mut moved_into: Vec<(String, Vec<(String, usize)>)> = Vec::new();
    for (p, f) in &pairs {
        if !removed.contains(&p) {
            continue;
        }
        let idx = match moved_into.iter().position(|(k, _)| k == p) {
            Some(i) => i,
            None => {
                moved_into.push((p.clone(), Vec::new()));
                moved_into.len() - 1
            }
        };
        let dests = &mut moved_into[idx].1;
        match dests.iter_mut().find(|(k, _)| k == f) {
            Some(e) => e.1 += 1,
            None => dests.push((f.clone(), 1)),
        }
    }
    let mut renamed = 0;
    let mut renamed_pairs = Vec::new();
    for (p, dests) in &moved_into {
        let total: usize = dests.iter().map(|d| d.1).sum();
        let mut sorted = dests.clone();
        sorted.sort_by_key(|d| std::cmp::Reverse(d.1));
        if let Some((top, n)) = sorted.first()
            && added.contains(&top)
            && n * 2 > total
        {
            renamed += 1;
            if renamed_pairs.len() < 10 {
                renamed_pairs.push(format!("{p} => {top}"));
            }
        }
    }
    let mut moves = by_pair.clone();
    moves.sort_by_key(|m| std::cmp::Reverse(m.1));
    let top: Vec<String> = moves
        .iter()
        .take(8)
        .map(|(k, n)| format!("{n}x {k}"))
        .collect();
    obj(vec![
        ("statementsCompared", count(pairs.len())),
        ("relocatedStatements", count(relocated)),
        ("filesFresh", count(fresh_files.len())),
        ("filesPrior", count(prior_files.len())),
        ("filesAdded", count(added.len())),
        ("filesRemoved", count(removed.len())),
        ("filesRenamed", count(renamed)),
        ("renamedPairs", texts(&renamed_pairs)),
        ("topMoves", texts(&top)),
    ])
}

/// The scored layout: the composition plus the two line-level views.
struct Layout {
    comp: Composition,
    build_lines: usize,
    build_files: usize,
    build_by_key: Vec<(String, usize)>,
    name_only_lines: usize,
    name_only_files: usize,
}

fn layout_block(l: &Layout) -> JsValue {
    let t = &l.comp.tally;
    let li = layout_in(t, l.build_lines);
    let bc = l.build_lines as i64;
    obj(vec![
        ("churnLines", int(li.churn_lines)),
        ("churnLinesExBuild", int(li.churn_lines - bc)),
        ("buildConstantLines", count(l.build_lines)),
        ("buildConstantFiles", count(l.build_files)),
        (
            "buildConstantByKey",
            obj(l
                .build_by_key
                .iter()
                .map(|(k, n)| (k.as_str(), count(*n)))
                .collect()),
        ),
        ("real", int(li.real)),
        ("realExBuild", int(li.real - bc)),
        ("fileAddRemove", int(li.file_add_remove)),
        ("noise", int(li.noise)),
        ("naming", int(li.naming)),
        ("nameOnlyLines", count(l.name_only_lines)),
        ("nameOnlyFiles", count(l.name_only_files)),
        ("spellingIdenticalLines", count(t.spelling_identical_lines)),
        ("churnLinesClean", int(li.churn_lines_clean)),
        ("realClean", int(li.real_clean)),
        ("alias", int(li.alias)),
        ("reorder", int(li.reorder)),
    ])
}

fn vendor_block(v: &VendorChurn) -> JsValue {
    let pairs: Vec<JsValue> = v
        .relocated_pairs
        .iter()
        .map(|r| {
            obj(vec![
                ("prior", text(&r.prior)),
                ("fresh", text(&r.fresh)),
                ("score", fixed(r.score, 3)),
                ("realLines", count(r.real_lines)),
                ("drawLines", count(r.draw_lines)),
            ])
        })
        .collect();
    obj(vec![
        ("churnLines", count(v.total_lines)),
        ("noise", count(v.noise_lines)),
        ("real", count(v.real_lines)),
        ("manifest", count(v.manifest_changed_lines)),
        (
            "bodiesNameOnly",
            count(v.name_only.lines + v.free_reroll.lines),
        ),
        (
            "relocated",
            obj(vec![
                ("files", count(v.relocated.files)),
                ("lines", count(v.relocated.lines)),
                ("pairs", JsValue::Array(pairs)),
            ]),
        ),
    ])
}

fn split3<T>(s: &Split3<T>, f: impl Fn(&T) -> JsValue) -> JsValue {
    obj(vec![
        ("app", f(&s.app)),
        ("vendor", f(&s.vendor)),
        ("crossBoundary", f(&s.cross)),
    ])
}

fn names_counts(c: &NameCounts) -> JsValue {
    obj(vec![
        ("freshNames", count(c.fresh_names)),
        ("novelNames", count(c.novel_names)),
        ("sameNameMovedFile", count(c.same_name_moved_file)),
    ])
}

fn reloc_counts(c: &RelocCounts) -> JsValue {
    obj(vec![
        ("statementsCompared", count(c.statements_compared)),
        ("relocatedStatements", count(c.relocated_statements)),
    ])
}

fn mints(m: &MintCounts) -> JsValue {
    obj(vec![
        ("app", count(m.app)),
        ("vendor", count(m.vendor)),
        ("unattributed", count(m.unattributed)),
    ])
}

fn app_lines(a: &AppLines) -> JsValue {
    obj(vec![
        ("churnLines", int(a.churn_lines)),
        ("churnLinesClean", int(a.churn_lines_clean)),
        ("real", int(a.real)),
        ("realClean", int(a.real_clean)),
        ("fileAddRemove", int(a.file_add_remove)),
        ("noise", int(a.noise)),
        ("naming", int(a.naming)),
        ("alias", int(a.alias)),
        ("reorder", int(a.reorder)),
    ])
}

fn vendor_lines(v: &VendorLines) -> JsValue {
    obj(vec![
        ("churnLines", int(v.churn_lines)),
        ("noise", int(v.noise)),
        ("real", int(v.real)),
    ])
}

fn cross_lines(c: &CrossLines) -> JsValue {
    let dir = |d: (usize, usize)| obj(vec![("statements", count(d.0)), ("lines", count(d.1))]);
    obj(vec![
        ("churnLines", count(c.churn_lines)),
        ("appSide", count(c.app_side)),
        ("appSideFromFiles", count(c.app_side_from_files)),
        ("vendorSide", count(c.vendor_side)),
        ("toVendor", dir(c.to_vendor)),
        ("toApp", dir(c.to_app)),
        ("capped", count(c.capped)),
        ("refused", count(c.refused)),
        ("unparsedVendorFiles", count(c.unparsed_vendor_files)),
        (
            "moves",
            JsValue::Array(
                c.moves
                    .iter()
                    .map(|m| {
                        obj(vec![
                            ("from", text(&m.from)),
                            ("to", text(&m.to)),
                            ("statements", count(m.statements)),
                            ("lines", count(m.lines)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// The trees' paths a pair reads.
struct Tree<'p>(&'p Path);

impl Tree<'_> {
    fn meta(&self, f: &str) -> PathBuf {
        self.0.join(".humanify").join(f)
    }
}

/// Score one pair into its card.
/// copies (only when an input has CRLF line ends).
pub fn score_pair(inputs: &PairInputs) -> Result<JsValue, String> {
    let fresh = Tree(&inputs.fresh_tree);
    let prior = Tree(&inputs.prior_tree);
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let fresh_ledger = Ledger::read(&fresh.meta("split-ledger.json"))?;
    let prior_ledger = Ledger::read(&prior.meta("split-ledger.json"))?;
    let stats: serde_json::Value = serde_json::from_str(&read(&inputs.stats)?)
        .map_err(|e| format!("{}: {e}", inputs.stats.display()))?;
    let fresh_stmts: Vec<Stmt> =
        bundle_statements(&read(&fresh.meta("humanified.js"))?, "fresh bundle", true)?;
    let prior_stmts: Vec<Stmt> =
        bundle_statements(&read(&prior.meta("humanified.js"))?, "prior bundle", false)?;
    let kinds = statement_kinds(&fresh_stmts, &prior_stmts);
    let mut totals = StatementCounts::default();
    for (s, k) in fresh_stmts.iter().zip(&kinds) {
        totals.add(*k, s.lines);
    }
    let (fresh_src, prior_src) = (inputs.fresh_tree.join("src"), inputs.prior_tree.join("src"));
    let (fresh_vendor, prior_vendor) = (
        inputs.fresh_tree.join("vendor"),
        inputs.prior_tree.join("vendor"),
    );
    let layout = if fresh_src.is_dir() && prior_src.is_dir() {
        let bc = build_constant_churn(&prior_src, &fresh_src)?;
        let no = name_only_churn(&prior_src, &fresh_src)?;
        Some(Layout {
            comp: compose_diff(&prior_src, &fresh_src, false)?,
            build_lines: bc.lines,
            build_files: bc.files,
            build_by_key: bc.by_key,
            name_only_lines: no.lines,
            name_only_files: no.files,
        })
    } else {
        None
    };
    let differ = Differ;
    let vendor = if fresh_vendor.is_dir() && prior_vendor.is_dir() {
        Some(decompose(&prior_vendor, &fresh_vendor, &differ)?)
    } else {
        None
    };

    let mut churn = vec![
        (
            "statements",
            obj(vec![
                ("total", count(totals.total)),
                ("unchangedClean", count(totals.unchanged_clean)),
                ("unchangedChurned", count(totals.unchanged_churned)),
                ("novel", count(totals.novel)),
            ]),
        ),
        (
            "lines",
            obj(vec![
                ("namingNoiseLines", count(totals.naming_noise_lines)),
                ("realLines", count(totals.real_lines)),
            ]),
        ),
        ("relocations", relocations(&fresh_ledger, &prior_ledger)),
        ("tree", tree_churn(&fresh_ledger, &prior_ledger)),
    ];
    if let Some(l) = &layout {
        churn.push(("layout", layout_block(l)));
    }
    if let Some(v) = &vendor {
        churn.push(("vendor", vendor_block(v)));
    }
    let mut surfaces: Vec<(&str, JsValue)> = Vec::new();
    let split = statement_split(
        &fresh_stmts,
        &prior_stmts,
        &kinds,
        fresh_ledger.order.as_deref(),
        prior_ledger.order.as_deref(),
    );
    if let Some((s, to_vendor, to_app)) = &split {
        let mut cross = statement_block(&s.cross);
        cross.push(("toVendor", count(*to_vendor)));
        cross.push(("toApp", count(*to_app)));
        surfaces.push((
            "statements",
            obj(vec![
                ("app", obj(statement_block(&s.app))),
                ("vendor", obj(statement_block(&s.vendor))),
                ("crossBoundary", obj(cross)),
            ]),
        ));
    }
    surfaces.push((
        "names",
        split3(&name_split(&fresh_ledger, &prior_ledger), names_counts),
    ));
    surfaces.push((
        "tree",
        split3(&reloc_split(&fresh_ledger, &prior_ledger), reloc_counts),
    ));
    let census = stats["coverage"]["mintedCensus"]["names"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        });
    if let (Some(_), Some(order), Some(census)) = (&split, &fresh_ledger.order, &census) {
        surfaces.push(("mints", mints(&mint_split(census, &fresh_stmts, order))));
    }
    if let (Some(l), Some(v)) = (&layout, &vendor) {
        let li: LayoutIn = layout_in(&l.comp.tally, l.build_lines);
        let (app, ven, cross) = boundary_lines(
            &prior_src,
            &fresh_src,
            &prior_vendor,
            &fresh_vendor,
            &l.comp.real_by_file,
            &li,
            v,
        )?;
        surfaces.push((
            "lines",
            obj(vec![
                ("app", app_lines(&app)),
                ("vendor", vendor_lines(&ven)),
                ("crossBoundary", cross_lines(&cross)),
            ]),
        ));
    }
    churn.push(("surfaces", obj(surfaces)));
    Ok(obj(vec![
        ("pair", text(&inputs.pair)),
        ("scorerVersion", JsValue::Number(SCORER_VERSION as f64)),
        ("determinism", determinism(&stats)),
        ("churn", obj(churn)),
    ]))
}

/// Every partition the surface split promises, checked against the card's
/// own totals (app + vendor + cross = the field; mints + unattributed).
/// Empty = all add up.
pub fn partition_failures(card: &JsValue) -> Vec<String> {
    use super::card::{at, number_at};
    let n = |p: &str| number_at(card, p);
    let mut bad = Vec::new();
    let Some(_) = at(card, "churn.surfaces") else {
        return vec!["no surfaces block".into()];
    };
    let mut eq = |what: &str, got: Option<f64>, want: Option<f64>| {
        if got != want {
            bad.push(format!("{what}: split {got:?} != total {want:?}"));
        }
    };
    let sum3 = |axis: &str, field: &str| -> Option<f64> {
        let mut t = 0.0;
        for side in ["app", "vendor", "crossBoundary"] {
            t += n(&format!("churn.surfaces.{axis}.{side}.{field}"))?;
        }
        Some(t)
    };
    if at(card, "churn.surfaces.statements").is_some() {
        for (what, field, total) in [
            ("stmts", "total", "churn.statements.total"),
            ("clean", "unchangedClean", "churn.statements.unchangedClean"),
            (
                "noise",
                "unchangedChurned",
                "churn.statements.unchangedChurned",
            ),
            ("novel", "novel", "churn.statements.novel"),
            (
                "noiseLn",
                "namingNoiseLines",
                "churn.lines.namingNoiseLines",
            ),
            ("realLn", "realLines", "churn.lines.realLines"),
        ] {
            eq(what, sum3("statements", field), n(total));
        }
    } else {
        eq("statement split", None, Some(0.0));
    }
    for (what, field, total) in [
        (
            "reloc",
            "sameNameMovedFile",
            "churn.relocations.sameNameMovedFile",
        ),
        ("newName", "novelNames", "churn.relocations.novelNames"),
        ("freshNames", "freshNames", "churn.relocations.freshNames"),
    ] {
        eq(what, sum3("names", field), n(total));
    }
    eq(
        "relocSt",
        sum3("tree", "relocatedStatements"),
        n("churn.tree.relocatedStatements"),
    );
    eq(
        "compared",
        sum3("tree", "statementsCompared"),
        n("churn.tree.statementsCompared"),
    );
    if at(card, "churn.surfaces.mints").is_some() {
        let m = ["app", "vendor", "unattributed"]
            .iter()
            .map(|k| n(&format!("churn.surfaces.mints.{k}")))
            .sum::<Option<f64>>();
        eq("mints", m, n("determinism.mintedLeftovers"));
    }
    if at(card, "churn.surfaces.lines").is_some() {
        let l = |p: &str| n(&format!("churn.surfaces.lines.{p}"));
        let add = |xs: &[Option<f64>]| xs.iter().copied().sum::<Option<f64>>();
        eq(
            "treeLn+vendor",
            add(&[
                l("app.churnLines"),
                l("vendor.churnLines"),
                l("crossBoundary.churnLines"),
            ]),
            add(&[n("churn.layout.churnLines"), n("churn.vendor.churnLines")]),
        );
        eq(
            "real+vendorReal",
            add(&[
                l("app.real"),
                l("vendor.real"),
                l("crossBoundary.churnLines"),
                l("crossBoundary.appSideFromFiles").map(|x| -x),
            ]),
            add(&[n("churn.layout.real"), n("churn.vendor.real")]),
        );
        eq(
            "fileAddRemove",
            add(&[l("app.fileAddRemove"), l("crossBoundary.appSideFromFiles")]),
            n("churn.layout.fileAddRemove"),
        );
        eq("app noise", l("app.noise"), n("churn.layout.noise"));
        eq("vendor noise", l("vendor.noise"), n("churn.vendor.noise"));
    }
    bad
}

#[cfg(test)]
#[path = "analyze_test.rs"]
mod analyze_test;
