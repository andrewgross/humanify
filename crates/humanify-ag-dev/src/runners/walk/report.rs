//! The per-hop report over one or more version walks (the port of
//! `experiments/walk/report.ts`). NO NEW METRIC in the tables: every number
//! is produced by an existing owner — the eval scorer run on (hop N-1
//! tree, hop N tree) exactly as the eval runs it on a pair (`scoring`), and
//! the changed lines per top-level dir from the system `diff -rN`. Cards
//! are cached in `<walk>/cards/`, so re-running the report is cheap.
//!
//! One section is new (exp198): HOPS TO LOOK AT — the hops whose noise, or
//! whose vendor structural-hash turnover (vendor files named by a content
//! hash, `module-<8 hex>.js` / `lib_<8 hex>.js`, present in only one of the
//! two trees: the vendor layout re-drawing itself), is far above the walk's
//! own median. A reader of a 124-hop walk should not have to find the one
//! hop where the vendor layout turned over by scrolling.
//!
//! What a walk hop is NOT: an eval pair. A hop's prior is the SAME walk's
//! previous output, so a per-hop delta between two walks also contains
//! divergence inherited from earlier hops. Read the totals.

use std::path::{Path, PathBuf};

use humanify_ag_model::js::{JsValue, stringify};

use super::super::scoring::{self, PairInputs};
use super::super::{Ctx, text, tree_diff, write};
use super::hunks::{self, Kind};

/// Never part of the reviewable tree (the history excludes the same set).
const NOT_SOURCE: &[&str] = &[".humanify", "node_modules", "package-lock.json"];
/// Cards scored at once (each analysis can hold tens of GB on the biggest
/// trees).
const SCORE_JOBS: usize = 4;
/// A hop is flagged when its noise is at least this many times the walk's
/// median noise …
const NOISE_FACTOR: f64 = 10.0;
/// … or when this share of its hash-named vendor files turned over (and at
/// least NOISE_FACTOR × the median turnover).
const TURNOVER_SHARE: f64 = 0.05;

fn read_js(path: &Path) -> Option<JsValue> {
    JsValue::parse(&std::fs::read_to_string(path).ok()?).ok()
}

fn at<'a>(v: Option<&'a JsValue>, path: &[&str]) -> Option<&'a JsValue> {
    let mut cur = v?;
    for k in path {
        cur = cur.as_object()?.get(k)?;
    }
    Some(cur)
}

fn num_at(v: Option<&JsValue>, path: &[&str]) -> Option<f64> {
    match at(v, path)? {
        JsValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// One hop's row.
pub struct Row {
    pub from: Option<String>,
    pub to: String,
    pub hop: JsValue,
    pub card: Option<JsValue>,
    pub card_error: Option<String>,
    /// (total, by top-level entry).
    pub lines: Option<(f64, Vec<(String, f64)>)>,
    /// (hash-named vendor files on one side only, in the union).
    pub turnover: Option<(usize, usize)>,
}

fn manifest(walk: &Path) -> Result<JsValue, String> {
    read_js(&walk.join("manifest.json")).ok_or_else(|| format!("{}: no manifest.json", text(walk)))
}

fn versions_of(m: &JsValue) -> Vec<String> {
    match at(Some(m), &["versions"]) {
        Some(JsValue::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn card_path(walk: &Path, from: &str, to: &str) -> PathBuf {
    walk.join("cards").join(format!("{from}__{to}.json"))
}

/// Score one hop with the eval's own scorer; cached. Err names the failure.
pub fn score_hop(ctx: &Ctx, walk: &Path, from: &str, to: &str) -> Result<(), String> {
    let dest = card_path(walk, from, to);
    if dest.is_file() {
        return Ok(());
    }
    let (fresh, prior) = (walk.join("trees").join(to), walk.join("trees").join(from));
    let stats = walk.join("runs").join(format!("{to}.stats.json"));
    let pair = format!("{from}->{to}");
    let inputs = PairInputs {
        fresh_tree: &fresh,
        prior_tree: &prior,
        stats: &stats,
        pair: &pair,
    };
    match scoring::analyze(ctx, &inputs) {
        Ok(card) => write(&dest, &card),
        Err(e) => {
            let _ = write(&PathBuf::from(format!("{}.stderr", text(&dest))), &e);
            Err(format!("the scorer failed (see {}.stderr)", text(&dest)))
        }
    }
}

fn top_entries(a: &Path, b: &Path) -> Vec<String> {
    let mut tops: Vec<String> = [a, b]
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| !NOT_SOURCE.contains(&n.as_str()))
        .collect();
    tops.sort_by(|x, y| humanify_ag_model::js::cmp_utf16(x, y));
    tops.dedup();
    tops
}

/// Changed lines per top-level entry of the two trees, cached.
fn tree_lines(walk: &Path, from: &str, to: &str) -> Result<(f64, Vec<(String, f64)>), String> {
    let dest = walk.join("cards").join(format!("{from}__{to}.lines.json"));
    if let Some(v) = read_js(&dest) {
        return Ok(lines_from(&v));
    }
    let (a, b) = (walk.join("trees").join(from), walk.join("trees").join(to));
    let empty = walk.join("cards").join(".empty");
    std::fs::create_dir_all(&empty).map_err(|e| e.to_string())?;
    let mut by_top = Vec::new();
    for top in top_entries(&a, &b) {
        let (pa, pb) = (a.join(&top), b.join(&top));
        let side = |p: &Path| {
            if p.exists() {
                p.to_path_buf()
            } else {
                empty.clone()
            }
        };
        let n = if pa.is_dir() || pb.is_dir() {
            tree_diff::changed_lines_in_trees(&side(&pa), &side(&pb))?
        } else {
            let file_side = |p: &Path| {
                if p.exists() {
                    p.to_path_buf()
                } else {
                    PathBuf::from("/dev/null")
                }
            };
            tree_diff::changed_lines(&file_side(&pa), &file_side(&pb))?
        };
        by_top.push((top, n as f64));
    }
    let total: f64 = by_top.iter().map(|(_, n)| n).sum();
    let mut o = humanify_ag_model::js::JsObject::new();
    o.insert("total", JsValue::Number(total));
    let mut bt = humanify_ag_model::js::JsObject::new();
    for (k, n) in &by_top {
        bt.insert(k.as_str(), JsValue::Number(*n));
    }
    o.insert("byTop", JsValue::Object(bt));
    write(&dest, &stringify(&JsValue::Object(o)))?;
    Ok((total, by_top))
}

fn lines_from(v: &JsValue) -> (f64, Vec<(String, f64)>) {
    let total = num_at(Some(v), &["total"]).unwrap_or(0.0);
    let by_top = at(Some(v), &["byTop"])
        .and_then(JsValue::as_object)
        .map(|o| {
            o.entries()
                .iter()
                .map(|(k, n)| (k.clone(), if let JsValue::Number(n) = n { *n } else { 0.0 }))
                .collect()
        })
        .unwrap_or_default();
    (total, by_top)
}

fn is_hash_named(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".js") else {
        return false;
    };
    let hex = stem
        .strip_prefix("module-")
        .or_else(|| stem.strip_prefix("lib_"));
    hex.is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn hash_named_vendor_files(tree: &Path) -> std::collections::BTreeSet<String> {
    fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_hash_named)
            {
                out.insert(p.strip_prefix(root).map(text).unwrap_or_default());
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    let root = tree.join("vendor");
    walk(&root, &root, &mut out);
    out
}

/// The vendor structural-hash turnover of a hop: hash-named vendor files
/// on one side only, and the union.
pub fn turnover(walk: &Path, from: &str, to: &str) -> (usize, usize) {
    let a = hash_named_vendor_files(&walk.join("trees").join(from));
    let b = hash_named_vendor_files(&walk.join("trees").join(to));
    (a.symmetric_difference(&b).count(), a.union(&b).count())
}

/// Every finished hop of a walk, scored (cards in parallel, `SCORE_JOBS`
/// at a time; completion order reaches nothing).
pub fn walk_rows(ctx: &Ctx, walk: &Path) -> Result<Vec<Row>, String> {
    let m = manifest(walk)?;
    let hops: Vec<JsValue> = versions_of(&m)
        .iter()
        .filter_map(|v| read_js(&walk.join("runs").join(format!("{v}.hop.json"))))
        .collect();
    let mut rows: Vec<Row> = Vec::new();
    for (i, hop) in hops.iter().enumerate() {
        let to = at(Some(hop), &["version"])
            .and_then(JsValue::as_str)
            .unwrap_or("")
            .to_string();
        let from = (i > 0)
            .then(|| {
                at(Some(&hops[i - 1]), &["version"])
                    .and_then(JsValue::as_str)
                    .map(str::to_string)
            })
            .flatten();
        rows.push(Row {
            from,
            to,
            hop: hop.clone(),
            card: None,
            card_error: None,
            lines: None,
            turnover: None,
        });
    }
    let todo: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].from.is_some())
        .collect();
    let scored = super::super::each(&todo, SCORE_JOBS, |&i| {
        let r = &rows[i];
        score_hop(ctx, walk, r.from.as_deref().unwrap_or(""), &r.to).err()
    });
    let errors: Vec<(usize, String)> = todo
        .iter()
        .zip(scored)
        .filter_map(|(&i, e)| e.map(|e| (i, e)))
        .collect();
    for &i in &todo {
        let from = rows[i].from.clone().unwrap_or_default();
        let to = rows[i].to.clone();
        match errors.iter().find(|(k, _)| *k == i) {
            Some((_, e)) => rows[i].card_error = Some(e.clone()),
            None => rows[i].card = read_js(&card_path(walk, &from, &to)),
        }
        rows[i].lines = tree_lines(walk, &from, &to).ok();
        rows[i].turnover = Some(turnover(walk, &from, &to));
    }
    Ok(rows)
}

// ── presentation ────────────────────────────────────────────────────────

/// The headline noise: src on-disk noise (naming + alias + reorder) +
/// vendor noise.
fn noise_lines(card: Option<&JsValue>) -> Option<f64> {
    let l = num_at(card, &["churn", "layout", "noise"])?;
    Some(l + num_at(card, &["churn", "vendor", "noise"]).unwrap_or(0.0))
}

/// `n.toLocaleString("en-US")`; `-` for a missing value.
pub fn fmt(n: Option<f64>) -> String {
    let Some(n) = n else { return "-".into() };
    if n.fract() != 0.0 {
        let s = format!("{:.3}", n.abs());
        let s = s.trim_end_matches('0').trim_end_matches('.');
        let (int, frac) = s.split_once('.').map_or((s, ""), |(a, b)| (a, b));
        let sign = if n < 0.0 { "-" } else { "" };
        let dot = if frac.is_empty() { "" } else { "." };
        return format!("{sign}{}{dot}{frac}", group(int));
    }
    let sign = if n < 0.0 { "-" } else { "" };
    format!("{sign}{}", group(&format!("{}", n.abs() as u64)))
}

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn sum(xs: &[Option<f64>]) -> Option<f64> {
    if xs.iter().all(Option::is_none) {
        return None;
    }
    Some(xs.iter().map(|x| x.unwrap_or(0.0)).sum())
}

type Get = fn(&Row) -> Option<f64>;

struct Column {
    head: &'static str,
    get: Get,
    total: bool,
}

fn layout(r: &Row, k: &str) -> Option<f64> {
    num_at(r.card.as_ref(), &["churn", "layout", k])
}

fn vendor(r: &Row, k: &str) -> Option<f64> {
    num_at(r.card.as_ref(), &["churn", "vendor", k])
}

fn by_top(r: &Row, k: &str) -> Option<f64> {
    r.lines
        .as_ref()?
        .1
        .iter()
        .find(|(t, _)| t == k)
        .map(|(_, n)| *n)
}

const DETAIL: &[Column] = &[
    Column {
        head: "exit",
        get: |r| num_at(Some(&r.hop), &["exitCode"]),
        total: false,
    },
    Column {
        head: "wall s",
        get: |r| num_at(Some(&r.hop), &["wallSeconds"]),
        total: true,
    },
    Column {
        head: "RSS MB",
        get: |r| num_at(Some(&r.hop), &["peakRssMb"]),
        total: false,
    },
    Column {
        head: "cache+",
        get: |r| num_at(Some(&r.hop), &["cacheWritten"]),
        total: true,
    },
    Column {
        head: "diffLn",
        get: |r| r.lines.as_ref().map(|l| l.0),
        total: true,
    },
    Column {
        head: "src",
        get: |r| by_top(r, "src"),
        total: true,
    },
    Column {
        head: "vendor",
        get: |r| by_top(r, "vendor"),
        total: true,
    },
    Column {
        head: "noiseLines",
        get: |r| noise_lines(r.card.as_ref()),
        total: true,
    },
    Column {
        head: "naming",
        get: |r| layout(r, "naming"),
        total: true,
    },
    Column {
        head: "alias",
        get: |r| layout(r, "alias"),
        total: true,
    },
    Column {
        head: "reorderLn",
        get: |r| layout(r, "reorder"),
        total: true,
    },
    Column {
        head: "vendorLn",
        get: |r| vendor(r, "noise"),
        total: true,
    },
    Column {
        head: "nameOnly",
        get: |r| layout(r, "nameOnlyLines"),
        total: true,
    },
    Column {
        head: "treeLn",
        get: |r| layout(r, "churnLines"),
        total: true,
    },
    Column {
        head: "realExBuild",
        get: |r| layout(r, "realExBuild"),
        total: true,
    },
    Column {
        head: "vendorReal",
        get: |r| vendor(r, "real"),
        total: true,
    },
    // The eval KPIs (kpis.ts `fromCard`), on the BUNDLE.
    Column {
        head: "noise",
        get: |r| {
            num_at(
                r.card.as_ref(),
                &["churn", "statements", "unchangedChurned"],
            )
        },
        total: true,
    },
    Column {
        head: "noiseLn",
        get: |r| num_at(r.card.as_ref(), &["churn", "lines", "namingNoiseLines"]),
        total: true,
    },
    Column {
        head: "novel",
        get: |r| num_at(r.card.as_ref(), &["churn", "statements", "novel"]),
        total: true,
    },
    Column {
        head: "realLn",
        get: |r| num_at(r.card.as_ref(), &["churn", "lines", "realLines"]),
        total: true,
    },
    Column {
        head: "relocSt",
        get: |r| num_at(r.card.as_ref(), &["churn", "tree", "relocatedStatements"]),
        total: true,
    },
    Column {
        head: "reloc",
        get: |r| {
            num_at(
                r.card.as_ref(),
                &["churn", "relocations", "sameNameMovedFile"],
            )
        },
        total: true,
    },
    Column {
        head: "mints",
        get: |r| num_at(r.card.as_ref(), &["determinism", "mintedLeftovers"]),
        total: true,
    },
];

fn hop_bool(r: &Row, k: &str) -> bool {
    at(Some(&r.hop), &["boot", k]) == Some(&JsValue::Bool(true))
}

fn boot_cell(r: &Row) -> String {
    if hop_bool(r, "ok") {
        return "OK".into();
    }
    if hop_bool(r, "versionOk") && hop_bool(r, "promptBlockedByAccount") {
        return "--version OK, -p UNVERIFIED (account quota)".into();
    }
    format!(
        "FAIL({}{})",
        if hop_bool(r, "versionOk") { "" } else { "ver" },
        if hop_bool(r, "promptOk") { "" } else { " -p" }
    )
}

fn hop_name(r: &Row) -> String {
    match &r.from {
        Some(f) => format!("{f}→{}", r.to),
        None => format!("{} (cold)", r.to),
    }
}

fn detail_table(name: &str, rows: &[Row]) -> Vec<String> {
    let mut head = vec!["hop"];
    head.extend(DETAIL.iter().map(|c| c.head));
    head.push("boot");
    let mut out = vec![
        format!("### {name}"),
        String::new(),
        format!("| {} |", head.join(" | ")),
        format!(
            "|{}|",
            head.iter().map(|_| "---").collect::<Vec<_>>().join("|")
        ),
    ];
    let run_col = |h: &str| {
        ["exit", "wall", "RSS", "cache"]
            .iter()
            .any(|k| h.contains(k))
    };
    for r in rows {
        let cells: Vec<String> = DETAIL
            .iter()
            .map(|c| {
                if r.from.is_some() || run_col(c.head) {
                    fmt((c.get)(r))
                } else {
                    String::new()
                }
            })
            .collect();
        let note = r
            .card_error
            .as_ref()
            .map_or(String::new(), |e| format!(" ⚠ {e}"));
        out.push(format!(
            "| {} | {} | {}{note} |",
            hop_name(r),
            cells.join(" | "),
            boot_cell(r)
        ));
    }
    let totals: Vec<String> = DETAIL
        .iter()
        .map(|c| {
            if !c.total {
                return String::new();
            }
            let all = c.head.contains("wall") || c.head.contains("cache");
            let vals: Vec<Option<f64>> = rows
                .iter()
                .filter(|r| all || r.from.is_some())
                .map(c.get)
                .collect();
            fmt(sum(&vals))
        })
        .collect();
    let ok = rows.iter().filter(|r| hop_bool(r, "ok")).count();
    out.push(format!(
        "| **total** | {} | {ok}/{} OK |",
        totals.join(" | "),
        rows.len()
    ));
    out
}

const HEADLINE: &[Column] = &[
    Column {
        head: "diffLn",
        get: |r| r.lines.as_ref().map(|l| l.0),
        total: true,
    },
    Column {
        head: "noiseLines",
        get: |r| noise_lines(r.card.as_ref()),
        total: true,
    },
    Column {
        head: "nameOnly",
        get: |r| layout(r, "nameOnlyLines"),
        total: true,
    },
    Column {
        head: "realExBuild",
        get: |r| layout(r, "realExBuild"),
        total: true,
    },
    Column {
        head: "novel",
        get: |r| num_at(r.card.as_ref(), &["churn", "statements", "novel"]),
        total: true,
    },
    Column {
        head: "wall s",
        get: |r| num_at(Some(&r.hop), &["wallSeconds"]),
        total: true,
    },
];

fn comparison_table(walks: &[(String, Vec<Row>)]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for (_, rows) in walks {
        for r in rows.iter().filter(|r| r.from.is_some()) {
            let k = hop_name(r);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    let mut head = vec!["hop".to_string()];
    for h in HEADLINE {
        for (n, _) in walks {
            head.push(format!("{} {n}", h.head));
        }
    }
    let mut out = vec![
        format!("| {} |", head.join(" | ")),
        format!(
            "|{}|",
            head.iter().map(|_| "---").collect::<Vec<_>>().join("|")
        ),
    ];
    let find = |rows: &'_ [Row], k: &str| -> Option<usize> {
        rows.iter()
            .position(|r| r.from.is_some() && hop_name(r) == k)
    };
    let common: Vec<&String> = keys
        .iter()
        .filter(|k| walks.iter().all(|(_, rows)| find(rows, k).is_some()))
        .collect();
    for k in common.iter() {
        let cells: Vec<String> = HEADLINE
            .iter()
            .flat_map(|h| {
                walks
                    .iter()
                    .map(move |(_, rows)| fmt((h.get)(&rows[find(rows, k).unwrap_or(0)])))
            })
            .collect();
        out.push(format!("| {k} | {} |", cells.join(" | ")));
    }
    let common = &common;
    let totals: Vec<String> = HEADLINE
        .iter()
        .flat_map(|h| {
            walks.iter().map(move |(_, rows)| {
                let vals: Vec<Option<f64>> = common
                    .iter()
                    .map(|k| (h.get)(&rows[find(rows, k).unwrap_or(0)]))
                    .collect();
                fmt(sum(&vals))
            })
        })
        .collect();
    out.push(format!(
        "| **total ({} common hops)** | {} |",
        common.len(),
        totals.join(" | ")
    ));
    out
}

// ── hops to look at (exp198) ────────────────────────────────────────────

fn median(mut xs: Vec<f64>) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let n = xs.len();
    if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    }
}

/// The hops whose noise or vendor structural-hash turnover is far above
/// the walk's median, each with the rule that fired.
pub fn hops_to_look_at(name: &str, rows: &[Row]) -> Vec<String> {
    let hops: Vec<&Row> = rows.iter().filter(|r| r.from.is_some()).collect();
    let noise: Vec<f64> = hops
        .iter()
        .filter_map(|r| noise_lines(r.card.as_ref()))
        .collect();
    let turn: Vec<f64> = hops
        .iter()
        .filter_map(|r| r.turnover.map(|t| t.0 as f64))
        .collect();
    let (mn, mt) = (median(noise), median(turn));
    let mut out = vec![
        format!("### {name}"),
        String::new(),
        format!(
            "Flagged when noiseLines ≥ {NOISE_FACTOR}× the walk's median ({}), or when ≥ {}% of the hop's \
             hash-named vendor files (`module-<8 hex>.js` / `lib_<8 hex>.js`) appear in only one of the two \
             trees and that count is ≥ {NOISE_FACTOR}× the median ({}). These are hops to READ, not verdicts.",
            fmt(Some(mn)),
            TURNOVER_SHARE * 100.0,
            fmt(Some(mt))
        ),
        String::new(),
    ];
    let mut flagged = Vec::new();
    for r in &hops {
        let mut why = Vec::new();
        if let Some(n) = noise_lines(r.card.as_ref())
            && mn > 0.0
            && n >= NOISE_FACTOR * mn
        {
            why.push(format!(
                "noiseLines {} = {:.1}× median",
                fmt(Some(n)),
                n / mn
            ));
        }
        if let Some((t, u)) = r.turnover {
            let share = t as f64 / u.max(1) as f64;
            if share >= TURNOVER_SHARE && t as f64 >= NOISE_FACTOR * mt {
                why.push(format!(
                    "vendor hash turnover {} of {} files ({:.1}%)",
                    fmt(Some(t as f64)),
                    fmt(Some(u as f64)),
                    share * 100.0
                ));
            }
        }
        if !why.is_empty() {
            flagged.push(format!("- **{}** — {}", hop_name(r), why.join("; ")));
        }
    }
    if flagged.is_empty() {
        out.push("_(none: every hop is within the thresholds)_".into());
    } else {
        out.extend(flagged);
    }
    out
}

// ── examples ────────────────────────────────────────────────────────────

/// Hops worth reading: the biggest real change, the noisiest, the median,
/// the calmest, the second biggest.
fn example_hops(rows: &[Row], n: usize) -> Vec<&Row> {
    let scored: Vec<&Row> = rows
        .iter()
        .filter(|r| r.from.is_some() && at(r.card.as_ref(), &["churn", "layout"]).is_some())
        .collect();
    let real = |r: &&Row| layout(r, "realExBuild").unwrap_or(0.0);
    let mut by_real = scored.clone();
    by_real.sort_by(|a, b| real(b).total_cmp(&real(a)));
    let mut by_noise = scored.clone();
    by_noise.sort_by(|a, b| {
        noise_lines(b.card.as_ref())
            .unwrap_or(0.0)
            .total_cmp(&noise_lines(a.card.as_ref()).unwrap_or(0.0))
    });
    let picks = [
        by_real.first(),
        by_noise.first(),
        by_real.get(by_real.len() / 2),
        by_real.last(),
        by_real.get(1),
    ];
    let mut out: Vec<&Row> = Vec::new();
    for p in picks.into_iter().flatten() {
        if out.len() < n && !out.iter().any(|o| o.to == p.to) {
            out.push(p);
        }
    }
    out.sort_by_key(|r| scored.iter().position(|s| s.to == r.to));
    out
}

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Real => "real change (edited code)",
        Kind::Added => "real change (new code)",
        Kind::Removed => "real change (deleted code)",
        Kind::NameOnly => "NOISE: same code, different local names",
        Kind::Moved => {
            "REORDER: identical lines in a new order (noise, or an upstream init-order change — the text alone cannot tell)"
        }
    }
}

fn examples_section(
    ctx: &Ctx,
    name: &str,
    walk: &Path,
    rows: &[Row],
    n: usize,
) -> Result<Vec<String>, String> {
    let m = manifest(walk)?;
    let mut versions = versions_of(&m);
    super::sort_versions(&mut versions);
    let pipeline = at(Some(&m), &["pipeline"])
        .and_then(JsValue::as_str)
        .unwrap_or("rust");
    let repo = super::history::build(ctx, walk, &versions, pipeline)?;
    let mut out = vec![
        format!("## Example diffs — {name} walk"),
        String::new(),
        format!(
            "From `{}` (one commit + tag per version; `.humanify/` excluded).",
            text(&repo)
        ),
        "Reproduce any hunk with `git --git-dir <repo> diff vA vB -- <file>`.".into(),
        "Hunk labels are for READING only (identifier-masked comparison, 034's".into(),
        "`maskIdentifiers`); every number above comes from the eval scorer.".into(),
        String::new(),
    ];
    for r in example_hops(rows, n) {
        let from = r.from.clone().unwrap_or_default();
        let src = hunks::parse_hunks(&super::history::diff(&repo, &from, &r.to, "src")?);
        let vend = hunks::parse_hunks(&super::history::diff(&repo, &from, &r.to, "vendor")?);
        let t = hunks::tally(&src);
        let tk = |k| fmt(Some(hunks::tally_of(&t, k) as f64));
        out.push(format!("### {from} → {}", r.to));
        out.push(String::new());
        out.push(format!(
            "Scorecard: src realExBuild {}, noise {} (naming {}, alias {}, reorder {}), nameOnly {}, vendor noise {}.",
            fmt(layout(r, "realExBuild")),
            fmt(layout(r, "noise")),
            fmt(layout(r, "naming")),
            fmt(layout(r, "alias")),
            fmt(layout(r, "reorder")),
            fmt(layout(r, "nameOnlyLines")),
            fmt(vendor(r, "noise"))
        ));
        out.push(format!(
            "src hunks by reading label (changed lines): real {}, added {}, removed {}, name-only {}, moved {}.",
            tk(Kind::Real),
            tk(Kind::Added),
            tk(Kind::Removed),
            tk(Kind::NameOnly),
            tk(Kind::Moved)
        ));
        out.push(String::new());
        let mut picks = hunks::pick_examples(&src, &[Kind::Added, Kind::Real], 2);
        picks.extend(hunks::pick_examples(
            &src,
            &[Kind::NameOnly, Kind::Moved],
            2,
        ));
        picks.extend(hunks::pick_examples(&vend, &[Kind::NameOnly], 1));
        if picks.is_empty() {
            out.push("_(no readable hunks on this hop)_".into());
            out.push(String::new());
        }
        for (kind, h) in picks {
            out.push(format!("**{}** — `{}`", kind_label(kind), h.file));
            out.push(String::new());
            out.push("```diff".into());
            out.push(h.header.clone());
            out.extend(h.lines.iter().cloned());
            out.push("```".into());
            out.push(String::new());
        }
    }
    Ok(out)
}

// ── the report ──────────────────────────────────────────────────────────

/// The KPI caveats the definitions repeat (kpis.ts `caveatLines` for
/// noiseLn, realLn, reloc, treeLn, vendorLn — kept in step with the
/// scorer's KPI table).
const CAVEATS: &[&str] = &[
    "noiseLn: scores the BUNDLE, not the split tree. It was structurally pinned at 0 for the whole 054 arc — a pass that only edits split files cannot move it, however much churn it removes. Read `layout.churnLines` for what a reviewer of the tree sees, and never conclude 'no effect' from this column alone.",
    "realLn: realLn is STATEMENT MASS — one edited line inside a 5k-line statement charges the whole statement, so it overstates edited lines several-fold",
    "reloc: reloc is NAME-keyed, so restoring a statement to its correct file MOVES the recycled name and charges for it — it rose on all three experiments that cut relocation 91%. Read relocation-churn.ts instead",
    "treeLn: total tree churn, real change INCLUDED — it falls when noise falls but also when upstream simply changed less, so read it beside novel/realLn rather than as a noise column on its own",
    "vendorLn: vendor was UNSCORED before exp046, so older references print `-`, which is not 0. This counts only reducible churn (manifest + bodies that differ solely in minifier-rerolled local names); read it next to vendorReal, never instead of it",
];

fn definitions() -> Vec<String> {
    let mut d: Vec<String> = [
        "## What each column is",
        "",
        "All per-hop numbers score (hop N-1 tree → hop N tree) of the SAME walk.",
        "",
        "- **diffLn / src / vendor** — changed lines between the two trees (`lib/diff.ts`, `diff -rN`; a modified line counts twice; the convention of `git diff --numstat --no-renames`; git rename detection would count a moved-but-identical file as 0). `.humanify/` excluded. Includes REAL change.",
        "- **noiseLines** — the headline noise: src on-disk noise (`layout.noise` = naming + alias + reorder, git lines) + `vendor.noise`. Lower is better.",
        "- **naming / alias / reorderLn** — composeDiff's src components. `naming` only sees renames in statements whose hash did NOT flip.",
        "- **nameOnly** — `layout.nameOnlyLines`: LINE-level name-only churn (identical once local names are masked), incl. renames inside edited statements. Not additive with `naming`.",
        "- **treeLn** — `layout.churnLines`, total src tree churn, real INCLUDED; **realExBuild** — src real change minus the build-constant inlining lines.",
        "- **vendorLn / vendorReal** — vendor/ reducible noise vs genuine dependency change.",
        "- **noise / noiseLn / novel / realLn / relocSt / reloc / mints** — the eval KPIs (`kpis.ts`) on the BUNDLE; noise/novel in statements.",
        "- **cache+** — LLM cache entries written; must be 0 (no cache, rule 10). **wall s / RSS MB** — from run-pipeline.ts (wall excludes the heavy-lock wait; RSS = peak of the whole process tree).",
        "",
        "Caveats (from `kpis.ts`):",
        "",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    d.extend(CAVEATS.iter().map(|c| format!("- {c}")));
    d.push("- Per-hop TS-vs-Rust deltas are NOT an A/B: each walk's hop N inherits its own hop N-1, so divergence compounds. Cold LLM draws vary run to run (rule 11); `noise-bands.json` bands are for 4-pair eval TOTALS, not walk hops. `novel`/`realLn` are statement-hash based and should nearly agree between pipelines; a large gap there means the trees differ in structure, not naming.".into());
    d.push(String::new());
    d
}

fn header_line(name: &str, dir: &Path, rows: &[Row]) -> Result<String, String> {
    let m = manifest(dir)?;
    let record = at(Some(&m), &["pipelineRecord"]).map_or("null".into(), stringify);
    let model = at(Some(&m), &["llm", "model"])
        .and_then(JsValue::as_str)
        .unwrap_or("");
    let endpoint = at(Some(&m), &["llm", "endpoint"])
        .and_then(JsValue::as_str)
        .unwrap_or("");
    Ok(format!(
        "- **{name}**: `{}` — {record}; LLM {model} @ {endpoint}; {}/{} hops finished",
        text(dir),
        rows.len(),
        versions_of(&m).len()
    ))
}

fn row_json(r: &Row) -> JsValue {
    let lines = r.lines.as_ref().map_or(JsValue::Null, |(t, by)| {
        let mut bt = humanify_ag_model::js::JsObject::new();
        for (k, n) in by {
            bt.insert(k.as_str(), JsValue::Number(*n));
        }
        super::super::records::obj(vec![
            ("total", JsValue::Number(*t)),
            ("byTop", JsValue::Object(bt)),
        ])
    });
    let mut o = humanify_ag_model::js::JsObject::new();
    o.insert("from", r.from.clone().map_or(JsValue::Null, JsValue::str));
    o.insert("to", JsValue::str(r.to.as_str()));
    o.insert("hop", r.hop.clone());
    o.insert("lines", lines);
    o.insert_opt(
        "noiseLines",
        noise_lines(r.card.as_ref()).map(JsValue::Number),
    );
    o.insert_opt("cardError", r.card_error.clone().map(JsValue::str));
    o.insert("card", r.card.clone().unwrap_or(JsValue::Null));
    if let Some((t, u)) = r.turnover {
        o.insert(
            "vendorHashTurnover",
            super::super::records::obj(vec![
                ("files", JsValue::Number(t as f64)),
                ("of", JsValue::Number(u as f64)),
            ]),
        );
    }
    JsValue::Object(o)
}

/// Write `<out>/report.md` and `<out>/report.json` over `walks`
/// ((name, dir) each); examples come from `examples` (a walk name).
pub fn write_report(
    ctx: &Ctx,
    walks: &[(String, PathBuf)],
    out: &Path,
    examples: Option<&str>,
    example_hops_n: usize,
) -> Result<(), String> {
    let mut rows: Vec<(String, Vec<Row>)> = Vec::new();
    for (name, dir) in walks {
        ctx.say(format!("scoring {name} walk ({}) …", text(dir)));
        rows.push((name.clone(), walk_rows(ctx, dir)?));
    }
    let mut md = vec!["# Version walk report".to_string(), String::new()];
    for ((name, dir), (_, r)) in walks.iter().zip(&rows) {
        md.push(header_line(name, dir, r)?);
    }
    md.push(String::new());
    md.push("## Per hop, side by side".into());
    md.push(String::new());
    md.extend(comparison_table(&rows));
    md.push(String::new());
    md.push("## Hops to look at".into());
    md.push(String::new());
    for (name, r) in &rows {
        md.extend(hops_to_look_at(name, r));
        md.push(String::new());
    }
    md.push("## Per walk, every column".into());
    md.push(String::new());
    for (name, r) in &rows {
        md.extend(detail_table(name, r));
        md.push(String::new());
    }
    md.extend(definitions());
    let ex_name = examples
        .map(str::to_string)
        .or_else(|| {
            walks
                .iter()
                .find(|(n, _)| n == "rust")
                .map(|(n, _)| n.clone())
        })
        .or_else(|| walks.first().map(|(n, _)| n.clone()));
    if let Some(ex) = ex_name
        && let (Some((_, dir)), Some((_, r))) = (
            walks.iter().find(|(n, _)| *n == ex),
            rows.iter().find(|(n, _)| *n == ex),
        )
    {
        md.extend(examples_section(ctx, &ex, dir, r, example_hops_n)?);
    }
    write(&out.join("report.md"), &md.join("\n"))?;
    let mut json = humanify_ag_model::js::JsObject::new();
    for (name, r) in &rows {
        json.insert(
            name.as_str(),
            JsValue::Array(r.iter().map(row_json).collect()),
        );
    }
    write(
        &out.join("report.json"),
        &crate::json::js_text(&JsValue::Object(json), false),
    )?;
    ctx.say(format!("report: {}", text(&out.join("report.md"))));
    for (name, r) in &rows {
        for row in r {
            if let Some(e) = &row.card_error {
                ctx.say(format!(
                    "  !! {name} {}->{}: {e}",
                    row.from.as_deref().unwrap_or(""),
                    row.to
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
