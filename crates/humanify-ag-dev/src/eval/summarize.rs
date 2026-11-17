//! A label's cards -> `summary.json` and the console text. Every recorded
//! verdict reaches the banner, printed before the numbers and again last.

use std::cmp::Ordering;
use std::path::Path;

use crate::json::js::{JsObject, JsValue};
use humanify_ag_model::js::locale_compare;

use super::card::{at, is_scorecard, number_at, obj, text, texts};
use crate::committed::eval::SelfHopReference;
use crate::difflabel::GROUPS;

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

/// Boot, self-hop and pipeline verdicts.
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
        "   The numbers below were computed from tree(s) the pipeline itself marked invalid."
            .into(),
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

/// The pipeline line.
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

/// What produced a label's numbers: its manifests' models, endpoints and
/// efforts and naming modes.
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
    obj(vec![
        ("models", texts(&pick("config.model"))),
        ("endpoints", texts(&pick("config.endpoint"))),
        ("reasoningEfforts", texts(&pick("config.reasoningEffort"))),
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

/// A label's group lines over its pairs, outside the metadata folder.
fn group_lines(card: &JsValue, group: &str) -> f64 {
    number_at(card, &format!("groups.{group}.outsideMetadata")).unwrap_or(0.0)
}

/// The per-pair table of changed lines by group, with a total row.
fn group_table(cards: &[JsValue]) -> Vec<String> {
    let mut out = vec![format!(
        "{:<18} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "pair", "changed", "real", "noise", "wiring", "unknown"
    )];
    let row = |name: &str, of: &dyn Fn(&str) -> f64| {
        let cells: Vec<String> = GROUPS.iter().map(|g| format!("{:>10}", of(g))).collect();
        let changed: f64 = GROUPS.iter().map(|g| of(g)).sum();
        format!("{name:<18} {changed:>10} {}", cells.join(" "))
    };
    for card in cards {
        out.push(row(str_at(card, "pair").unwrap_or(""), &|g| {
            group_lines(card, g)
        }));
    }
    out.push(row("TOTAL", &|g| {
        cards.iter().map(|c| group_lines(c, g)).sum()
    }));
    out.push(
        "lines outside .humanify/; real = from changed input code (a ceiling), noise = from \
         unchanged input, wiring = require and export lines of unknown cause"
            .into(),
    );
    out
}

/// A label's cards -> `summary.json` and the console text: the run-health
/// banner, the changed lines by group, the LLM and matching diagnostics.
pub fn summarize(dir: &Path, label: &str, reference: &SelfHopReference) -> Result<String, String> {
    let cards = load_cards(dir);
    if cards.is_empty() {
        return Err(format!("no label cards in {}", dir.display()));
    }
    let statuses = run_statuses(dir);
    let v = verdicts(dir);
    let prov = provenance(dir);
    let naming: Vec<String> = match at(&prov, "namingModes") {
        Some(JsValue::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let mode = if naming == ["names"] { "names" } else { "off" };
    let mut banner_lines = vec![if naming.len() == 1 {
        format!("NAMING: {}", naming[0])
    } else {
        format!(
            "!! NAMING: MIXED ({}) — this label's pairs ran in different naming modes",
            naming.join(" + ")
        )
    }];
    banner_lines.extend(banner(&statuses, &v, reference_of(reference, mode)));
    let short: Vec<&str> = cards
        .iter()
        .filter(|c| number_at(c, "changedLines") != number_at(c, "diffSays"))
        .filter_map(|c| str_at(c, "pair"))
        .collect();
    if !short.is_empty() {
        banner_lines.push(format!(
            "!! LABELS DO NOT ADD UP to the diff's count for: {}",
            short.join(", ")
        ));
    }
    let (llm_digests, mut llm_lines) = llm_section(dir, &cards);
    let (matching_digests, matching_lines) = matching_section(dir, &cards);
    llm_lines.extend(matching_lines);
    let summary = obj(vec![
        ("model", text(label)),
        ("provenance", prov),
        ("pairs", JsValue::Array(cards.clone())),
        ("runStatuses", JsValue::Array(statuses.clone())),
        ("verdicts", v),
        ("llmDiagnostics", JsValue::Array(llm_digests)),
        ("matchingDiagnostics", JsValue::Array(matching_digests)),
    ]);
    let path = dir.join("summary.json");
    std::fs::write(&path, crate::json::js_text(&summary, false))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = vec![format!("\n=== eval: {label} ===")];
    out.extend(banner_lines.iter().cloned());
    out.push(String::new());
    out.extend(group_table(&cards));
    out.extend(llm_lines);
    out.push(format!("\nwrote {}", path.display()));
    out.push(String::new());
    out.extend(banner_lines);
    if statuses.is_empty() {
        out.push("\nNOTE: no per-pair run status was recorded, so whether the pipeline accepted its own output is UNKNOWN, not clean.".into());
    }
    Ok(out.join("\n") + "\n")
}

/// The label's LLM call diagnostics (exp206, `eval::llm_diag`): one
/// digest per pair whose scored run left a call ledger, and the per-pair
/// tables (a pair without one prints `-`).
fn llm_section(dir: &Path, cards: &[JsValue]) -> (Vec<JsValue>, Vec<String>) {
    let digests = super::llm_diag::pair_digests(&super::llm_diag::LLM_CALLS, dir, cards);
    let rows = pair_rows(cards, &digests);
    let lines = super::llm_diag::tables(
        &super::llm_diag::LLM_CALLS,
        "LLM call diagnostics (per pair)",
        &rows,
        false,
    );
    (digests, lines)
}

/// The label's matching counts (exp208, `eval::matching`), per pair, from
/// the same stats files.
fn matching_section(dir: &Path, cards: &[JsValue]) -> (Vec<JsValue>, Vec<String>) {
    let spec = &super::matching::MATCHING;
    let digests = super::llm_diag::pair_digests(spec, dir, cards);
    let rows = pair_rows(cards, &digests);
    let lines = super::llm_diag::tables(spec, "MATCHING (per pair)", &rows, false);
    (digests, lines)
}

/// One `(pair, digest)` row per card, in card order (None prints `-`).
fn pair_rows(cards: &[JsValue], digests: &[JsValue]) -> Vec<(String, Option<JsValue>)> {
    cards
        .iter()
        .map(|c| {
            let pair = str_at(c, "pair").unwrap_or("").to_string();
            let d = digests
                .iter()
                .find(|d| str_at(d, "pair") == Some(pair.as_str()))
                .cloned();
            (pair, d)
        })
        .collect()
}

#[cfg(test)]
#[path = "summarize_test.rs"]
mod summarize_test;
