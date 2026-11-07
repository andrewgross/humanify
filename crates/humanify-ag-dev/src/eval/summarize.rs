//! `eval summarize`: a label's cards -> `summary.json` + the console table
//! (TS: `034/summarize.ts`, with `experiments/lib/invariants.ts`'s readers
//! of the run statuses and verdicts and `run-manifest.ts`'s provenance).
//! Every recorded verdict reaches the banner, printed BEFORE the numbers
//! and again last: a fact written and unread reads as assurance.

use std::cmp::Ordering;
use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue, locale_compare, to_fixed};

use super::card::{at, is_scorecard, number_at, obj, scorer_version, text, texts};
use super::kpis::{Denominator, Kpi, caveat_lines, named};
use crate::committed::eval::SelfHopReference;

/// `a.localeCompare(b, undefined, { numeric: true })` for version-ish
/// strings: digit runs compare by value.
pub fn numeric_compare(a: &str, b: &str) -> Ordering {
    fn chunks(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(&cb) {
        let o = match (x.0, y.0) {
            (true, true) => {
                let (nx, ny) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
                nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny))
            }
            _ => locale_compare(x.1, y.1),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len())
}

fn read_json(p: &Path) -> Option<JsValue> {
    JsValue::parse(&std::fs::read_to_string(p).ok()?).ok()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|r| {
            r.filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Every scorecard in a label directory, sorted by pair (numeric).
pub fn load_cards(dir: &Path) -> Vec<JsValue> {
    let mut cards: Vec<JsValue> = files(dir)
        .iter()
        .filter(|f| f.ends_with(".json") && *f != "summary.json")
        .filter_map(|f| read_json(&dir.join(f)))
        .filter(is_scorecard)
        .collect();
    let pair = |c: &JsValue| {
        at(c, "pair")
            .and_then(JsValue::as_str)
            .unwrap_or("")
            .to_string()
    };
    cards.sort_by(|a, b| numeric_compare(&pair(a), &pair(b)));
    cards
}

/// `flattenSurfaces`: every numeric leaf of a card's `churn.surfaces`, as
/// `split.<path>` (arrays are not totals).
pub fn flatten_surfaces(v: &JsValue, at_path: &str, out: &mut Vec<(String, f64)>) {
    match v {
        JsValue::Number(n) => out.push((format!("split.{at_path}"), *n)),
        JsValue::Object(o) => {
            for (k, x) in o.entries() {
                let p = if at_path.is_empty() {
                    k.clone()
                } else {
                    format!("{at_path}.{k}")
                };
                flatten_surfaces(x, &p, out);
            }
        }
        _ => {}
    }
}

/// The folded totals, and how many cards carried each optional block.
pub struct Totals {
    pub totals: JsObject,
    pub tree_cards: usize,
    pub spelling_cards: usize,
    pub clean_cards: usize,
    pub surface_cards: usize,
    pub layout_cards: usize,
}

const FIXED_TOTALS: &[(&str, &str)] = &[
    ("stmts", "churn.statements.total"),
    ("unchangedClean", "churn.statements.unchangedClean"),
    ("unchangedChurned", "churn.statements.unchangedChurned"),
    ("namingNoiseLines", "churn.lines.namingNoiseLines"),
    ("novel", "churn.statements.novel"),
    ("realLines", "churn.lines.realLines"),
    ("sameNameMovedFile", "churn.relocations.sameNameMovedFile"),
    ("novelNames", "churn.relocations.novelNames"),
    ("freshNames", "churn.relocations.freshNames"),
    ("mintedLeftovers", "determinism.mintedLeftovers"),
];

const LAYOUT_TOTALS: &[(&str, &str)] = &[
    ("layoutChurnLines", "churnLines"),
    ("layoutBuildConstantLines", "buildConstantLines"),
    ("layoutNameOnlyLines", "nameOnlyLines"),
    ("layoutSpellingIdenticalLines", "spellingIdenticalLines"),
    ("layoutChurnLinesClean", "churnLinesClean"),
    ("layoutRealClean", "realClean"),
    ("layoutReal", "real"),
    ("layoutNoise", "noise"),
    ("layoutNaming", "naming"),
    ("layoutAlias", "alias"),
    ("layoutReorder", "reorder"),
];

/// Fold cards into the published totals (`summarizeCards`).
pub fn summarize_cards(cards: &[JsValue]) -> Totals {
    let mut sums: Vec<(String, f64)> = Vec::new();
    let names = FIXED_TOTALS
        .iter()
        .map(|(k, _)| *k)
        .chain(["relocatedStatements"])
        .chain(LAYOUT_TOTALS.iter().map(|(k, _)| *k))
        .chain(["vendorChurnLines", "vendorNoise", "vendorReal"]);
    for n in names {
        sums.push((n.to_string(), 0.0));
    }
    let add = |sums: &mut Vec<(String, f64)>, k: &str, v: f64| match sums
        .iter_mut()
        .find(|(n, _)| n == k)
    {
        Some(e) => e.1 += v,
        None => sums.push((k.to_string(), v)),
    };
    let mut t = Totals {
        totals: JsObject::new(),
        tree_cards: 0,
        spelling_cards: 0,
        clean_cards: 0,
        surface_cards: 0,
        layout_cards: 0,
    };
    for c in cards {
        for (k, p) in FIXED_TOTALS {
            add(&mut sums, k, number_at(c, p).unwrap_or(0.0));
        }
        if let Some(n) = number_at(c, "churn.tree.relocatedStatements") {
            add(&mut sums, "relocatedStatements", n);
            t.tree_cards += 1;
        }
        if at(c, "churn.layout").is_some() {
            t.layout_cards += 1;
            if number_at(c, "churn.layout.spellingIdenticalLines").is_some() {
                t.spelling_cards += 1;
            }
            let clean = number_at(c, "churn.layout.churnLinesClean").is_some();
            if clean {
                t.clean_cards += 1;
            }
            for (k, f) in LAYOUT_TOTALS {
                let is_clean = *f == "churnLinesClean" || *f == "realClean";
                if is_clean && !clean {
                    continue;
                }
                add(
                    &mut sums,
                    k,
                    number_at(c, &format!("churn.layout.{f}")).unwrap_or(0.0),
                );
            }
        }
        if at(c, "churn.vendor").is_some() {
            add(
                &mut sums,
                "vendorChurnLines",
                number_at(c, "churn.vendor.churnLines").unwrap_or(0.0),
            );
            add(
                &mut sums,
                "vendorNoise",
                number_at(c, "churn.vendor.noise").unwrap_or(0.0),
            );
            add(
                &mut sums,
                "vendorReal",
                number_at(c, "churn.vendor.real").unwrap_or(0.0),
            );
        }
        if let Some(s) = at(c, "churn.surfaces") {
            t.surface_cards += 1;
            let mut flat = Vec::new();
            flatten_surfaces(s, "", &mut flat);
            for (k, v) in flat {
                add(&mut sums, &k, v);
            }
        }
    }
    for (k, v) in sums {
        t.totals.insert(k, JsValue::Number(v));
    }
    t
}

// ------------------------------------------------------- verdicts, statuses

/// Every `*-run-status.json`'s recorded fields, sorted by version.
pub fn run_statuses(dir: &Path) -> Vec<JsValue> {
    let mut out: Vec<JsValue> = files(dir)
        .iter()
        .filter(|f| f.ends_with("-run-status.json"))
        .filter_map(|f| read_json(&dir.join(f)))
        .filter_map(|j| {
            let version = at(&j, "version")?.as_str()?.to_string();
            let exit = number_at(&j, "exitCode")?;
            let mut o = JsObject::new();
            o.insert("version", text(&version));
            o.insert("exitCode", JsValue::Number(exit));
            o.insert(
                "errors",
                match at(&j, "errors") {
                    Some(e @ JsValue::Array(_)) => e.clone(),
                    _ => JsValue::Array(Vec::new()),
                },
            );
            for k in ["providerErrors", "libraryMatching"] {
                if let Some(b) = at(&j, k)
                    && matches!(at(b, "refused"), Some(JsValue::Bool(_)))
                {
                    o.insert(k, b.clone());
                }
            }
            Some(JsValue::Object(o))
        })
        .collect();
    let v = |s: &JsValue| {
        at(s, "version")
            .and_then(JsValue::as_str)
            .unwrap_or("")
            .to_string()
    };
    out.sort_by(|a, b| numeric_compare(&v(a), &v(b)));
    out
}

fn boot_of(j: &JsValue) -> Option<JsValue> {
    let v = at(j, "boot.version")?.as_str()?;
    let JsValue::Bool(ok) = at(j, "boot.ok")? else {
        return None;
    };
    Some(obj(vec![("version", text(v)), ("ok", JsValue::Bool(*ok))]))
}

fn self_hop_of(j: &JsValue) -> Option<JsValue> {
    let s = at(j, "selfHop")?;
    let v = at(s, "version")?.as_str()?;
    let JsValue::Bool(identical) = at(s, "identical")? else {
        return None;
    };
    let mut o = JsObject::new();
    o.insert("version", text(v));
    o.insert(
        "ran",
        JsValue::Bool(!matches!(at(s, "ran"), Some(JsValue::Bool(false)))),
    );
    o.insert("identical", JsValue::Bool(*identical));
    o.insert(
        "diffLines",
        JsValue::Number(number_at(s, "diffLines").unwrap_or(-1.0)),
    );
    if let Some(cc) = at(s, "coldCache").and_then(JsValue::as_str) {
        o.insert("coldCache", text(cc));
    }
    if let Some(w) = at(s, "warm")
        && matches!(at(w, "ok"), Some(JsValue::Bool(_)))
    {
        o.insert("warm", w.clone());
    }
    Some(JsValue::Object(o))
}

fn preflight_of(j: &JsValue) -> Option<JsValue> {
    let v = at(j, "preflight.verdict")?.as_str()?;
    let st = number_at(j, "preflight.status")?;
    let mut o = vec![("verdict", text(v)), ("status", JsValue::Number(st))];
    if let Some(c) = at(j, "preflight.covers").and_then(JsValue::as_str) {
        o.push(("covers", text(c)));
    }
    Some(obj(o))
}

fn pipeline_of(j: &JsValue) -> Option<JsValue> {
    let kind = at(j, "pipeline.kind")?.as_str()?;
    let mut o = vec![("kind", text(kind))];
    if let Some(b) = at(j, "pipeline.bin") {
        let s = |k: &str| at(b, k).cloned().unwrap_or(JsValue::Null);
        o.push((
            "bin",
            obj(vec![
                ("sha256", s("sha256")),
                ("commit", s("commit")),
                ("dirty", s("dirty")),
            ]),
        ));
    }
    Some(obj(o))
}

/// Boot, self-hop, preflight and pipeline verdicts.
pub fn verdicts(dir: &Path) -> JsValue {
    let (mut boots, mut hops, mut extra) = (Vec::new(), Vec::new(), Vec::new());
    for f in files(dir) {
        let Some(j) = read_json(&dir.join(&f)) else {
            continue;
        };
        if f.ends_with("-boot.json") {
            boots.extend(boot_of(&j));
        } else if f.ends_with("-self-hop.json") {
            hops.extend(self_hop_of(&j));
        } else if f == "preflight-status.json" {
            extra.extend(preflight_of(&j).map(|p| ("preflight", p)));
        } else if f == "pipeline.json" {
            extra.extend(pipeline_of(&j).map(|p| ("pipeline", p)));
        }
    }
    let mut v = JsObject::new();
    v.insert("boots", JsValue::Array(boots));
    v.insert("selfHops", JsValue::Array(hops));
    for (k, x) in extra {
        v.insert(k, x);
    }
    JsValue::Object(v)
}

fn str_at<'v>(v: &'v JsValue, p: &str) -> Option<&'v str> {
    at(v, p).and_then(JsValue::as_str)
}

/// The cold half of the self-hop gate, as one sentence.
pub fn cold_range_verdict(s: &JsValue, reference: Option<(&str, Vec<u64>)>) -> String {
    let version = str_at(s, "version").unwrap_or("");
    let diff = number_at(s, "diffLines").unwrap_or(-1.0);
    let head = format!("cold self-hop {version}: {diff} ln");
    let Some((ref_version, counts)) = reference else {
        return format!(
            "{head} — not judged: no reference range is recorded for this naming mode."
        );
    };
    let lo = counts.iter().min().copied().unwrap_or(0) as f64;
    let hi = counts.iter().max().copied().unwrap_or(0) as f64;
    if str_at(s, "coldCache") == Some("seeded") {
        return format!(
            "{head} — not judged: the cold leg replayed a copy of --llm-cache, so it was not cold."
        );
    }
    if version != ref_version {
        return format!(
            "{head} — not judged: the reference range ({lo}-{hi} ln) is {ref_version}'s only."
        );
    }
    if diff > hi {
        return format!(
            "!! {head} — OUTSIDE the reference range {lo}-{hi} ln ({} cold runs on record).",
            counts.len()
        );
    }
    if diff < lo {
        return format!(
            "{head} — below the reference range {lo}-{hi} ln (fewer re-rolls than any cold run on record)."
        );
    }
    format!("{head} — inside the reference range {lo}-{hi} ln.")
}

fn reference_of(r: &SelfHopReference, mode: &str) -> Option<(String, Vec<u64>)> {
    match mode {
        "off" => Some((
            r.version.clone(),
            r.cold_diff_lines.values().copied().collect(),
        )),
        m => r.by_mode.get(m).map(|x| {
            (
                x.version.clone(),
                x.cold_diff_lines.values().copied().collect(),
            )
        }),
    }
}

/// The run-status lines: refused pairs and pipeline failures.
fn status_banner(statuses: &[JsValue]) -> Vec<String> {
    let mut lines = Vec::new();
    let refused = |k: &str| -> Vec<String> {
        statuses
            .iter()
            .filter(|s| matches!(at(s, &format!("{k}.refused")), Some(JsValue::Bool(true))))
            .map(|s| {
                str_at(s, &format!("{k}.pair"))
                    .or(str_at(s, "version"))
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    };
    for (k, why, hint) in [
        (
            "providerErrors",
            "their runs lost LLM answers (providerErrors)",
            "   See each pair's <v>-run-status.json; re-run them when the endpoint is healthy.",
        ),
        (
            "libraryMatching",
            "their runs SKIPPED library name matching (no index for the bundle's kind)",
            "   See each pair's <v>-run-status.json; pass the library index explicitly and re-run them.",
        ),
    ] {
        let r = refused(k);
        if !r.is_empty() {
            lines.push(format!(
                "!! {} pair(s) REFUSED — {why}, so no card was scored: {}",
                r.len(),
                r.join(", ")
            ));
            lines.push(hint.to_string());
        }
    }
    let failed: Vec<&JsValue> = statuses
        .iter()
        .filter(|s| number_at(s, "exitCode") != Some(0.0))
        .collect();
    if failed.is_empty() {
        return lines;
    }
    let names: Vec<&str> = failed
        .iter()
        .map(|s| str_at(s, "version").unwrap_or(""))
        .collect();
    lines.push(format!(
        "!! {} of {} scored pair(s) FAILED IN THE PIPELINE: {}",
        failed.len(),
        statuses.len(),
        names.join(", ")
    ));
    lines.push(
        "   The KPIs below were computed from tree(s) the pipeline itself marked invalid.".into(),
    );
    for f in failed {
        lines.push(format!(
            "   {} (exit {}):",
            str_at(f, "version").unwrap_or(""),
            number_at(f, "exitCode").unwrap_or(-1.0)
        ));
        if let Some(JsValue::Array(es)) = at(f, "errors") {
            lines.extend(
                es.iter()
                    .map(|e| format!("     {}", e.as_str().unwrap_or(""))),
            );
        }
    }
    lines
}

/// The warm self-hop's line, when it did not run or failed.
fn warm_line(s: &JsValue) -> Option<String> {
    let w = at(s, "warm")?;
    let ver = str_at(s, "version").unwrap_or("");
    if !matches!(at(w, "ran"), Some(JsValue::Bool(true))) {
        return Some(format!(
            "!! WARM SELF-HOP DID NOT RUN for {ver} — the determinism half of the gate was not checked."
        ));
    }
    if matches!(at(w, "ok"), Some(JsValue::Bool(true))) {
        return None;
    }
    let g = |k: &str| number_at(w, k).unwrap_or(-1.0);
    let ident = if matches!(at(w, "identical"), Some(JsValue::Bool(true))) {
        "tree identical, ".to_string()
    } else {
        format!(
            "{} file(s) / {} ln differ, ",
            g("diffFiles"),
            g("diffLines")
        )
    };
    Some(format!(
        "!! WARM SELF-HOP FAILED for {ver}: {ident}{} cache write(s), exit {} vs {} (must be 0 / 0 / 0 / equal) — nondeterminism with the model held fixed.",
        g("cacheWrites"),
        g("coldExit"),
        g("warmExit")
    ))
}

/// The pipeline and preflight lines.
fn pipeline_banner(v: &JsValue) -> Vec<String> {
    let mut lines = Vec::new();
    if str_at(v, "pipeline.kind") == Some("rust-bin") {
        let sha = str_at(v, "pipeline.bin.sha256")
            .map_or("?".to_string(), |s| s.chars().take(12).collect());
        let commit: String = str_at(v, "pipeline.bin.commit")
            .unwrap_or("")
            .chars()
            .take(12)
            .collect();
        let commit = if commit.is_empty() {
            "an UNKNOWN commit".to_string()
        } else {
            commit
        };
        let dirty = if matches!(at(v, "pipeline.bin.dirty"), Some(JsValue::Bool(true))) {
            " (DIRTY tree)"
        } else {
            ""
        };
        lines.push(format!(
            "NOTE: scored by the Rust binary {sha} built from {commit}{dirty}."
        ));
        if str_at(v, "preflight.covers") == Some("ts-matcher") {
            lines.push("NOTE: the matcher preflight validated the TS matcher (test/e2e/harness), not the binary that scored these pairs.".into());
        }
    }
    if let Some(p) = str_at(v, "preflight.verdict")
        && p != "ok"
    {
        lines.push(format!(
            "NOTE: matcher preflight '{p}' — these pairs were scored WITHOUT a validated fingerprint matcher. Usually the fixture builds are absent (a frozen worktree); it is not a matcher finding, but it is not a clean bill of health either."
        ));
    }
    lines
}

/// The verdict banner (`runStatusBanner` + `verdictBanner`).
pub fn banner(
    statuses: &[JsValue],
    v: &JsValue,
    reference: Option<(String, Vec<u64>)>,
) -> Vec<String> {
    let mut lines = status_banner(statuses);
    let arr = |p: &str| match at(v, p) {
        Some(JsValue::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    for b in arr("boots") {
        if !matches!(at(&b, "ok"), Some(JsValue::Bool(true))) {
            lines.push(format!(
                "!! BOOT FAILED for {} — the scored tree does not run.",
                str_at(&b, "version").unwrap_or("")
            ));
        }
    }
    let hops = arr("selfHops");
    let reference = reference.as_ref().map(|(v, c)| (v.as_str(), c.clone()));
    for s in &hops {
        let ran = matches!(at(s, "ran"), Some(JsValue::Bool(true)));
        if ran && matches!(at(s, "identical"), Some(JsValue::Bool(false))) {
            lines.push(format!(
                "NOTE: self-hop diverged for {} ({} diff lines). Expected on a COLD run (live LLM re-rolls, exp047); on a cached run this is a determinism regression.",
                str_at(s, "version").unwrap_or(""),
                number_at(s, "diffLines").unwrap_or(-1.0)
            ));
            lines.push(cold_range_verdict(s, reference.clone()));
        }
    }
    lines.extend(hops.iter().filter_map(warm_line));
    lines.extend(pipeline_banner(v));
    lines
}

// ------------------------------------------------------------- provenance

fn sorted_unique(mut xs: Vec<String>) -> Vec<String> {
    xs.sort();
    xs.dedup();
    xs
}

/// The run manifests' base mode (`baseModeOf`).
fn base_mode_of(m: &JsValue) -> String {
    if let Some(b) = str_at(m, "inputs.baseMode") {
        return b.to_string();
    }
    let prior = str_at(m, "inputs.prior").unwrap_or("");
    let kind = str_at(m, "inputs.priorKind")
        .map(str::to_string)
        .unwrap_or_else(|| {
            if prior.is_empty() {
                "unknown".into()
            } else if prior.contains("-rebased/") || prior.ends_with("-rebased") {
                "rebased".into()
            } else {
                "archive".into()
            }
        });
    match kind.as_str() {
        "rebased" => "seeded".into(),
        "archive" => "archive".into(),
        _ => "unknown".into(),
    }
}

/// What produced a label's numbers: its manifests' models, endpoints and
/// efforts, its base modes and naming modes.
pub fn provenance(dir: &Path) -> JsValue {
    let manifests: Vec<JsValue> = files(dir)
        .iter()
        .filter(|f| f.ends_with("-run.json"))
        .filter_map(|f| read_json(&dir.join(f)))
        .filter(|m| at(m, "config").is_some() && at(m, "inputs").is_some())
        .collect();
    let pick = |p: &str| {
        sorted_unique(
            manifests
                .iter()
                .filter_map(|m| str_at(m, p).map(str::to_string))
                .collect(),
        )
    };
    let base = sorted_unique(manifests.iter().map(base_mode_of).collect());
    obj(vec![
        ("models", texts(&pick("config.model"))),
        ("endpoints", texts(&pick("config.endpoint"))),
        ("reasoningEfforts", texts(&pick("config.reasoningEffort"))),
        ("baseModes", texts(&base)),
        ("namingModes", texts(&naming_modes(dir, &manifests))),
    ])
}

/// `labelNamingModes`: `pipeline.json`'s `namingMode`, else every
/// manifest's (`config.resolved.library.matching == names`), else off.
fn naming_modes(dir: &Path, manifests: &[JsValue]) -> Vec<String> {
    if let Some(p) = read_json(&dir.join("pipeline.json"))
        && let Some(m) = str_at(&p, "namingMode")
    {
        return vec![m.to_string()];
    }
    let modes = sorted_unique(
        manifests
            .iter()
            .map(|m| {
                if str_at(m, "config.resolved.library.matching") == Some("names") {
                    "names"
                } else {
                    "off"
                }
                .to_string()
            })
            .collect(),
    );
    if modes.is_empty() {
        vec!["off".into()]
    } else {
        modes
    }
}

// ------------------------------------------------------------------ tables

fn pad(s: &str, w: usize) -> String {
    format!("{s:>w$}")
}

fn num_text(n: f64) -> String {
    humanify_ag_model::js::number_to_string(n)
}

fn cp(n: f64, total: f64, w: usize) -> String {
    let pct = if total != 0.0 {
        to_fixed(100.0 * n / total, 1)
    } else {
        "0.0".into()
    };
    pad(&format!("{} ({pct}%)", num_text(n)), w)
}

const CHURN_COLUMNS: &[(&str, usize)] = &[
    ("clean", 15),
    ("noise", 14),
    ("novel", 13),
    ("noiseLn", 8),
    ("realLn", 8),
    ("reloc", 13),
    ("newName", 14),
    ("mints", 6),
];

fn churn_row(
    label: &str,
    shown: &[&Kpi],
    value: impl Fn(&Kpi) -> Option<f64>,
    statements: f64,
    names: f64,
    det: &str,
    llm: &str,
) -> String {
    let mut cells = vec![
        format!("{label:<16}"),
        pad(
            &if statements != 0.0 {
                num_text(statements)
            } else {
                String::new()
            },
            7,
        ),
    ];
    for (k, (_, w)) in shown.iter().zip(CHURN_COLUMNS) {
        cells.push(match value(k) {
            None => pad("-", *w),
            Some(v) => match k.denominator {
                Some(Denominator::Statements) => cp(v, statements, *w),
                Some(Denominator::Names) => cp(v, names, *w),
                None => pad(&num_text(v), *w),
            },
        });
    }
    cells.push(pad(det, 6));
    cells.push(pad(llm, 6));
    cells.join(" ")
}

/// Summarize the label in `dir`: write `summary.json`, return the console
/// text. `reference` is the committed self-hop reference.
pub fn summarize(dir: &Path, label: &str, reference: &SelfHopReference) -> Result<String, String> {
    let cards = load_cards(dir);
    if cards.is_empty() {
        return Err(format!("no scorecards in {}", dir.display()));
    }
    let t = summarize_cards(&cards);
    let totals = JsValue::Object(t.totals.clone());
    let statuses = run_statuses(dir);
    let v = verdicts(dir);
    let prov = provenance(dir);
    let modes: Vec<String> = match at(&prov, "namingModes") {
        Some(JsValue::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let mode = if modes.len() == 1 && modes[0] == "names" {
        "names"
    } else {
        "off"
    };
    let mut banner_lines = banner(&statuses, &v, reference_of(reference, mode));
    let n = cards.len();
    if t.tree_cards != n {
        banner_lines.push(format!("NOTE: relocSt totals {} of {n} pairs — the rest recorded no tree churn, and their statements are missing from the total rather than counted as zero.", t.tree_cards));
    }
    if t.surface_cards != n {
        banner_lines.push(format!("NOTE: the app/vendor split covers {} of {n} pairs — the rest predate it (exp170, 2026-10-31); re-score their trees with `eval rescore`. Their split is missing, not zero.", t.surface_cards));
    }
    if t.spelling_cards != t.layout_cards {
        banner_lines.push(format!("NOTE: spellingIdentical totals {} of the {} scored pairs — the rest predate the field (2026-09-29), and their soft-noise mass is missing from the total rather than counted as zero.", t.spelling_cards, t.layout_cards));
    }
    if t.clean_cards != t.layout_cards {
        banner_lines.push(format!("NOTE: the clean-diff totals cover {} of the {} scored pairs — the rest predate the breakdown (2026-10-02), and their clean numbers are missing from the total rather than counted as zero.", t.clean_cards, t.layout_cards));
    }
    let versions: Vec<u64> = {
        let mut v: Vec<u64> = cards.iter().map(scorer_version).collect();
        v.sort();
        v.dedup();
        v
    };
    if versions.len() > 1 {
        banner_lines.push(format!("!! SCORER: MIXED (versions {versions:?}) — this label's cards were scored by different scorers; re-score it with `eval rescore`"));
    }
    let list = |p: &str| match at(&prov, p) {
        Some(JsValue::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let base = list("baseModes");
    let naming = list("namingModes");
    let mut head = vec![
        match base.len() {
            1 => format!("BASE: {}", base[0]),
            0 => "BASE: UNKNOWN (no run manifests)".into(),
            _ => format!(
                "!! BASE: MIXED ({}) — this label's pairs were scored on different bases",
                base.join(" + ")
            ),
        },
        if naming.len() == 1 {
            format!("NAMING: {}", naming[0])
        } else {
            format!(
                "!! NAMING: MIXED ({}) — this label's pairs ran in different naming modes",
                naming.join(" + ")
            )
        },
        format!(
            "SCORER: v{}",
            versions
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("+v")
        ),
    ];
    head.extend(banner_lines);
    let banner_lines = head;
    let summary = obj(vec![
        ("model", text(label)),
        ("provenance", prov),
        ("pairs", JsValue::Array(cards.clone())),
        ("totals", totals.clone()),
        ("runStatuses", JsValue::Array(statuses.clone())),
        ("verdicts", v),
    ]);
    let path = dir.join("summary.json");
    std::fs::write(&path, crate::json::js_text(&summary, false))
        .map_err(|e| format!("{}: {e}", path.display()))?;

    let mut out = Vec::new();
    out.push(format!("\n=== eval: {label} ==="));
    out.extend(banner_lines.iter().cloned());
    if !banner_lines.is_empty() {
        out.push(String::new());
    }
    out.push("clean/noise/novel = % of stmts · reloc/newName = % of names · noise+reloc+mints are the reducible KPIs to drive to 0".into());
    let keys: Vec<&str> = CHURN_COLUMNS.iter().map(|(k, _)| *k).collect();
    let shown = named(&keys);
    let mut head_cells = vec![format!("{:<16}", "pair"), pad("stmts", 7)];
    head_cells.extend(CHURN_COLUMNS.iter().map(|(k, w)| pad(k, *w)));
    head_cells.push(pad("%det", 6));
    head_cells.push(pad("%llm", 6));
    let head = head_cells.join(" ");
    let rule = "-".repeat(head.chars().count());
    out.push(head);
    out.push(rule.clone());
    for c in &cards {
        let d = |p: &str| number_at(c, p).map(num_text).unwrap_or_default();
        out.push(churn_row(
            str_at(c, "pair").unwrap_or(""),
            &shown,
            |k| k.from_card(c),
            number_at(c, "churn.statements.total").unwrap_or(0.0),
            number_at(c, "churn.relocations.freshNames").unwrap_or(0.0),
            &d("determinism.functions.pctDeterministic"),
            &d("determinism.functions.pctReachingLLM"),
        ));
    }
    out.push(rule);
    let tot = |k: &str| number_at(&totals, k).unwrap_or(0.0);
    out.push(churn_row(
        "TOTAL",
        &shown,
        |k| k.from_totals(&totals),
        tot("stmts"),
        tot("freshNames"),
        "",
        "",
    ));
    out.extend(caveat_lines(&shown));
    print_layout(&cards, &totals, t.clean_cards, &mut out);
    print_surfaces(&cards, &totals, &mut out);
    out.push(format!("\nwrote {}", path.display()));
    if !banner_lines.is_empty() {
        out.push(String::new());
        out.extend(banner_lines);
    }
    if statuses.is_empty() {
        out.push("\nNOTE: no per-pair run status was recorded — this predates the check, so whether the pipeline accepted its own output is UNKNOWN, not clean.".into());
    }
    Ok(out.join("\n") + "\n")
}

fn print_layout(cards: &[JsValue], totals: &JsValue, clean_cards: usize, out: &mut Vec<String>) {
    let scored: Vec<&JsValue> = cards
        .iter()
        .filter(|c| at(c, "churn.layout").is_some())
        .collect();
    if scored.is_empty() {
        return;
    }
    out.push("\n=== on-disk diff composition (git lines; EVAL_LAYOUT) ===".into());
    let cols = [
        ("churn", 9),
        ("churnEXB", 9),
        ("bldConst", 9),
        ("real", 9),
        ("noise", 9),
        ("naming", 8),
        ("nameOnly", 9),
        ("alias", 7),
        ("reorder", 9),
        ("relocSt", 8),
        ("spell", 9),
    ];
    let head = std::iter::once(format!("{:<16}", "pair"))
        .chain(cols.iter().map(|(k, w)| pad(k, *w)))
        .collect::<Vec<_>>()
        .join(" ");
    let rule = "-".repeat(head.chars().count());
    out.push(head);
    out.push(rule.clone());
    let row = |label: &str, v: [f64; 9], reloc: f64| {
        let pct = if v[0] != 0.0 {
            format!("{}%", to_fixed(100.0 * v[8] / v[0], 1))
        } else {
            "-".into()
        };
        let cells = [
            num_text(v[0]),
            num_text(v[1]),
            num_text(v[2]),
            num_text(v[3]),
            num_text(v[4]),
            num_text(v[5]),
            num_text(v[6]),
            num_text(v[7]),
            format!("{} {pct}", num_text(v[8])),
            num_text(reloc),
        ];
        std::iter::once(format!("{label:<16}"))
            .chain(cells.iter().zip(&cols).map(|(c, (_, w))| pad(c, *w)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let tot = |k: &str| number_at(totals, k).unwrap_or(0.0);
    let line = |label: &str, v: [f64; 9], reloc: f64, spell: f64| {
        format!("{} {}", row(label, v, reloc), pad(&num_text(spell), 9))
    };
    out.push(line(
        "TOTAL",
        [
            tot("layoutChurnLines"),
            tot("layoutChurnLines") - tot("layoutBuildConstantLines"),
            tot("layoutBuildConstantLines"),
            tot("layoutReal"),
            tot("layoutNoise"),
            tot("layoutNaming"),
            tot("layoutNameOnlyLines"),
            tot("layoutAlias"),
            tot("layoutReorder"),
        ],
        tot("relocatedStatements"),
        tot("layoutSpellingIdenticalLines"),
    ));
    out.push(rule);
    for c in &scored {
        let l = |k: &str| number_at(c, &format!("churn.layout.{k}"));
        let churn = l("churnLines").unwrap_or(0.0);
        out.push(line(
            str_at(c, "pair").unwrap_or(""),
            [
                churn,
                l("churnLinesExBuild").unwrap_or(churn),
                l("buildConstantLines").unwrap_or(0.0),
                l("real").unwrap_or(0.0),
                l("noise").unwrap_or(0.0),
                l("naming").unwrap_or(0.0),
                l("nameOnlyLines").unwrap_or(0.0),
                l("alias").unwrap_or(0.0),
                l("reorder").unwrap_or(0.0),
            ],
            number_at(c, "churn.tree.relocatedStatements").unwrap_or(0.0),
            l("spellingIdenticalLines").unwrap_or(0.0),
        ));
    }
    out.push("reorder = byte-identical statements emitted at a different position; relocSt = statements that changed FILE (order-independent)".into());
    out.push(format!(
        "spell = SOFT noise inside real — of the {} real lines above, {} are spelling-only (statement pairs identical modulo wrapper arrow<->function spelling)",
        num_text(tot("layoutReal")),
        num_text(tot("layoutSpellingIdenticalLines"))
    ));
    // The clean diff.
    out.push("\n=== clean diff (raw vs clean; soft-flow breakdown) ===".into());
    let cols2 = [
        ("rawChurn", 10),
        ("-buildMeta", 11),
        ("-spelling", 11),
        ("cleanChurn", 12),
        ("rawReal", 9),
        ("cleanReal", 10),
    ];
    let head = std::iter::once(format!("{:<16}", "pair"))
        .chain(cols2.iter().map(|(k, w)| pad(k, *w)))
        .collect::<Vec<_>>()
        .join(" ");
    let rule = "-".repeat(head.chars().count());
    out.push(head);
    out.push(rule.clone());
    let row2 = |label: &str, v: [String; 6]| {
        std::iter::once(format!("{label:<16}"))
            .chain(v.iter().zip(&cols2).map(|(c, (_, w))| pad(c, *w)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let dash = |b: bool, x: f64| if b { num_text(x) } else { "-".into() };
    out.push(row2(
        "TOTAL",
        [
            num_text(tot("layoutChurnLines")),
            num_text(tot("layoutBuildConstantLines")),
            num_text(tot("layoutSpellingIdenticalLines")),
            dash(clean_cards > 0, tot("layoutChurnLinesClean")),
            num_text(tot("layoutReal")),
            dash(clean_cards > 0, tot("layoutRealClean")),
        ],
    ));
    out.push(rule);
    for c in &scored {
        let l = |k: &str| number_at(c, &format!("churn.layout.{k}"));
        let d = |k: &str| l(k).map(num_text).unwrap_or_else(|| "-".into());
        out.push(row2(
            str_at(c, "pair").unwrap_or(""),
            [
                num_text(l("churnLines").unwrap_or(0.0)),
                num_text(l("buildConstantLines").unwrap_or(0.0)),
                num_text(l("spellingIdenticalLines").unwrap_or(0.0)),
                d("churnLinesClean"),
                num_text(l("real").unwrap_or(0.0)),
                d("realClean"),
            ],
        ));
    }
    out.push("raw = the frozen charge every recorded label prints (the trend line). clean = raw minus the valued categories: buildMeta (VERSION/BUILD_TIME/GIT_SHA constants inlined at many sites) and spelling (wrapper arrow<->function re-serializations). cleanChurn still INCLUDES the noise columns above. A `-` clean number means the card predates 2026-10-02, not a clean diff of zero.".into());
}

fn print_surfaces(cards: &[JsValue], totals: &JsValue, out: &mut Vec<String>) {
    let split: Vec<&JsValue> = cards
        .iter()
        .filter(|c| at(c, "churn.surfaces").is_some())
        .collect();
    if split.is_empty() {
        return;
    }
    out.push("\n=== app vs vendor (exp170; app + vendor + cross = the column above) ===".into());
    let head = [
        format!("{:<12}", "kpi"),
        pad("app", 10),
        pad("vendor", 10),
        pad("cross", 10),
        pad("= total", 10),
    ]
    .join(" ");
    out.push(head.clone());
    out.push("-".repeat(head.chars().count()));
    // The split totals are FLAT keys with dots in them (`split.lines.app.
    // churnLines`), not nested records.
    let get = |k: &str| match totals
        .as_object()
        .and_then(|o| o.get(&format!("split.{k}")))
    {
        Some(JsValue::Number(n)) => Some(*n),
        _ => None,
    };
    let tot = |k: &str| number_at(totals, k).unwrap_or(0.0);
    let cell = |v: Option<f64>| pad(&v.map(num_text).unwrap_or_else(|| "-".into()), 10);
    let mut row = |name: &str, axis: &str, field: &str, total: f64, cross: Option<Option<f64>>| {
        let app = get(&format!("{axis}.app.{field}"));
        let ven = get(&format!("{axis}.vendor.{field}"));
        let cr = cross.unwrap_or_else(|| get(&format!("{axis}.crossBoundary.{field}")));
        out.push(
            [
                format!("{name:<12}"),
                cell(app),
                cell(ven),
                cell(cr),
                cell(Some(total)),
            ]
            .join(" "),
        );
    };
    row(
        "treeLn*",
        "lines",
        "churnLines",
        tot("layoutChurnLines") + tot("vendorChurnLines"),
        None,
    );
    let cross_ln = get("lines.crossBoundary.churnLines");
    let cross_files = get("lines.crossBoundary.appSideFromFiles");
    row(
        "real*",
        "lines",
        "real",
        tot("layoutReal") + tot("vendorReal"),
        Some(cross_ln.map(|c| c - cross_files.unwrap_or(0.0))),
    );
    row(
        "files*",
        "lines",
        "fileAddRemove",
        tot("layoutChurnLines") - tot("layoutNoise") - tot("layoutReal"),
        Some(cross_files),
    );
    row(
        "noise*",
        "lines",
        "noise",
        tot("layoutNoise") + tot("vendorNoise"),
        Some(Some(0.0)),
    );
    row(
        "noise",
        "statements",
        "unchangedChurned",
        tot("unchangedChurned"),
        None,
    );
    row(
        "noiseLn",
        "statements",
        "namingNoiseLines",
        tot("namingNoiseLines"),
        None,
    );
    row("novel", "statements", "novel", tot("novel"), None);
    row("realLn", "statements", "realLines", tot("realLines"), None);
    row(
        "reloc",
        "names",
        "sameNameMovedFile",
        tot("sameNameMovedFile"),
        None,
    );
    row(
        "relocSt",
        "tree",
        "relocatedStatements",
        tot("relocatedStatements"),
        None,
    );
    row("newName", "names", "novelNames", tot("novelNames"), None);
    out.push(
        [
            format!("{:<12}", "mints"),
            cell(get("mints.app")),
            cell(get("mints.vendor")),
            pad(
                &format!(
                    "{} unatt",
                    get("mints.unattributed")
                        .map(num_text)
                        .unwrap_or_else(|| "-".into())
                ),
                10,
            ),
            cell(Some(tot("mintedLeftovers"))),
        ]
        .join(" "),
    );
    out.push("* tree git lines: app = src/ (the composition), vendor = vendor/ (the vendor scorer: its noise is vendorLn, its real vendorReal), total = the two surfaces' old sum. Statement rows: the BUNDLE (everything but CJS library factories: app code and ESM-inlined library code), by the file each statement is filed in — so vendor there is what the library placement filed into vendor/. mints are attributed by name; the unattributed rest is never guessed.".into());
    for c in split {
        let Some(x) = at(c, "churn.surfaces.lines.crossBoundary") else {
            continue;
        };
        let g = |k: &str| number_at(x, k).unwrap_or(0.0);
        let mut line = format!(
            "cross-boundary {}: {} lines (app side {}, vendor side {}); to vendor {} stmts, to app {} stmts",
            str_at(c, "pair").unwrap_or(""),
            num_text(g("churnLines")),
            num_text(g("appSide")),
            num_text(g("vendorSide")),
            num_text(g("toVendor.statements")),
            num_text(g("toApp.statements"))
        );
        if g("capped") != 0.0 {
            line.push_str(&format!(
                "; {} statement lines over their file's charge, not taken",
                num_text(g("capped"))
            ));
        }
        if g("refused") != 0.0 {
            line.push_str(&format!(
                "; {} lines of one-line statements not paired (too generic to prove a move)",
                num_text(g("refused"))
            ));
        }
        if g("unparsedVendorFiles") != 0.0 {
            line.push_str(&format!(
                "; {} vendor files unparsed",
                num_text(g("unparsedVendorFiles"))
            ));
        }
        out.push(line);
        if let Some(JsValue::Array(moves)) = at(x, "moves") {
            for m in moves.iter().take(12) {
                out.push(format!(
                    "    {}x {} -> {} ({} ln)",
                    number_at(m, "statements").map(num_text).unwrap_or_default(),
                    str_at(m, "from").unwrap_or(""),
                    str_at(m, "to").unwrap_or(""),
                    number_at(m, "lines").map(num_text).unwrap_or_default()
                ));
            }
            if moves.len() > 12 {
                out.push(format!("    ... {} more", moves.len() - 12));
            }
        }
    }
}

#[cfg(test)]
#[path = "summarize_test.rs"]
mod summarize_test;
