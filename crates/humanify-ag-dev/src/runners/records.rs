//! The verdict files a scored run writes beside its cards, in the shapes
//! (and the JS spelling) every reader on record parses: the run manifest
//! `<v>-run.json` (run-manifest.ts: what actually happened), the run
//! status `<v>-run-status.json` (invariants.ts: did the pipeline itself
//! say the run was valid, and is the card refused), the boot verdict
//! `<v>-boot.json` and the self-hop verdict `<v>-self-hop.json` (run.sh).
//!
//! The rules these records exist for are kept with them:
//! - a recorded failure must be impossible to read as a clean run, and an
//!   absent recording is never a pass (invariants.ts);
//! - `manifest_warnings` turns the dangerous COMBINATIONS into sentences
//!   the runner prints — a fact nobody reads is worth what a fact nobody
//!   recorded is worth (run-manifest.ts).

use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue};

use super::write;
use crate::bin_build::same_commit;

/// `JSON.stringify(x, null, 2)`, no trailing newline (writeManifest,
/// writeRunStatus).
pub fn js_pretty(v: &JsValue) -> String {
    crate::json::js_text(v, false)
}

/// `JSON.stringify(x)` + newline (run.sh's `jq -c` records).
pub fn js_compact_line(v: &JsValue) -> String {
    format!("{}\n", humanify_ag_model::js::stringify(v))
}

pub fn num(n: impl Into<f64>) -> JsValue {
    JsValue::Number(n.into())
}

pub fn obj(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut o = JsObject::new();
    for (k, v) in entries {
        o.insert(k, v);
    }
    JsValue::Object(o)
}

fn read_js(path: &Path) -> Option<JsValue> {
    JsValue::parse(&std::fs::read_to_string(path).ok()?).ok()
}

fn field<'a>(v: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    v.as_object()?.get(key)
}

// ─── run status (invariants.ts) ──────────────────────────────────────────

/// Cap on recorded ERROR lines (with their detail beneath).
const MAX_RECORDED_LINES: usize = 60;

/// A line that belongs to the ERROR block above it: indented detail, or
/// the failing line of a code frame (`>  2 | const a = 2;`).
fn is_block_continuation(line: &str) -> bool {
    let indented = line.starts_with(char::is_whitespace) && !line.trim().is_empty();
    let frame = line.strip_prefix('>').is_some_and(|rest| {
        let rest = rest.trim_start();
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        !digits.is_empty() && rest[digits.len()..].starts_with(" |")
    });
    indented || frame
}

/// `ERROR:` lines AND the indented detail beneath each one.
pub fn extract_error_blocks(stdout: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in stdout.split('\n') {
        if line.starts_with("ERROR:") {
            in_block = true;
            out.push(line.to_string());
        } else if in_block && is_block_continuation(line) {
            out.push(line.to_string());
        } else {
            in_block = false;
        }
        if out.len() >= MAX_RECORDED_LINES {
            break;
        }
    }
    out
}

const COUNT_KEYS: &[&str] = &[
    "total",
    "naming",
    "modules",
    "vendor",
    "split",
    "treeReviser",
];

/// The scored leg's `providerErrors` block, or None when the stats carry
/// none (or cannot be read).
fn read_provider_errors(stats: &Path) -> Option<JsValue> {
    let block = field(&read_js(stats)?, "providerErrors")?.clone();
    let mut o = JsObject::new();
    for k in COUNT_KEYS {
        match field(&block, k) {
            Some(n @ JsValue::Number(_)) => o.insert(*k, n.clone()),
            _ => return None,
        }
    }
    Some(JsValue::Object(o))
}

/// The binary's one failed-call WARNING: `WARNING: <n> LLM call(s) failed
/// after every retry`. No line = no failed call; no file = unknown.
fn rebase_provider_errors(stdout: &Path) -> Option<f64> {
    let text = std::fs::read_to_string(stdout).ok()?;
    let n = text.lines().find_map(|l| {
        let rest = l.strip_prefix("WARNING: ")?;
        let (n, tail) = rest.split_once(' ')?;
        tail.starts_with("LLM call(s) failed after every retry")
            .then(|| {
                n.bytes()
                    .all(|b| b.is_ascii_digit())
                    .then(|| n.parse::<f64>().ok())
                    .flatten()
            })
            .flatten()
    });
    Some(n.unwrap_or(0.0))
}

fn read_library_matching(stats: &Path) -> Option<JsValue> {
    let block = field(&read_js(stats)?, "libraryMatching")?.clone();
    matches!(field(&block, "outcome"), Some(JsValue::String(_))).then_some(block)
}

/// The binary's skip line (`no library index for <kind> bundles in ...
/// skipping library name matching (...`).
fn rebase_skip_line(stdout: &Path) -> Option<String> {
    let text = std::fs::read_to_string(stdout).ok()?;
    text.lines()
        .find(|l| {
            l.strip_prefix("no library index for ").is_some_and(|rest| {
                rest.split_once(' ')
                    .is_some_and(|(_, t)| t.starts_with("bundles in "))
                    && l.contains(" skipping library name matching (")
            })
        })
        .map(str::to_string)
}

/// Where a pair's run status reads its sources.
#[derive(Default)]
pub struct StatusSources<'a> {
    pub stats: Option<&'a Path>,
    pub rebase_stdout: Option<&'a Path>,
    pub pair: Option<&'a str>,
}

fn opt_or_null(v: Option<JsValue>) -> JsValue {
    v.unwrap_or(JsValue::Null)
}

fn provider_errors_record(src: &StatusSources) -> JsValue {
    let scored = src.stats.and_then(read_provider_errors);
    let rebase = src.rebase_stdout.and_then(rebase_provider_errors);
    let total = scored
        .as_ref()
        .and_then(|s| match field(s, "total") {
            Some(JsValue::Number(n)) => Some(*n),
            _ => None,
        })
        .unwrap_or(0.0);
    let refused = total > 0.0 || rebase.unwrap_or(0.0) > 0.0;
    let mut e = vec![
        ("scored", opt_or_null(scored)),
        ("rebase", rebase.map_or(JsValue::Null, num)),
        ("refused", JsValue::Bool(refused)),
    ];
    if let Some(p) = src.pair {
        e.push(("pair", JsValue::str(p)));
    }
    obj(e)
}

fn library_matching_record(src: &StatusSources) -> JsValue {
    let scored = src.stats.and_then(read_library_matching);
    let skipped = src.rebase_stdout.and_then(rebase_skip_line);
    let refused = scored
        .as_ref()
        .and_then(|s| field(s, "outcome"))
        .and_then(JsValue::as_str)
        == Some("skipped")
        || skipped.is_some();
    let mut e = vec![
        ("scored", opt_or_null(scored)),
        ("rebaseSkipped", skipped.map_or(JsValue::Null, JsValue::str)),
        ("refused", JsValue::Bool(refused)),
    ];
    if let Some(p) = src.pair {
        e.push(("pair", JsValue::str(p)));
    }
    obj(e)
}

/// Record how the pipeline exited for one pair (`<v>-run-status.json`)
/// with the `ERROR:` blocks that explain it; returns the status.
pub fn write_run_status(
    results: &Path,
    version: &str,
    exit_code: i32,
    sources: &StatusSources,
) -> Result<JsValue, String> {
    let stdout = results.join(format!("{version}.stdout"));
    let errors = if exit_code != 0 {
        std::fs::read_to_string(&stdout)
            .map(|t| extract_error_blocks(&t))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let status = obj(vec![
        ("version", JsValue::str(version)),
        ("exitCode", num(exit_code)),
        ("errors", JsValue::str_array(&errors)),
        ("providerErrors", provider_errors_record(sources)),
        ("libraryMatching", library_matching_record(sources)),
    ]);
    write(
        &results.join(format!("{version}-run-status.json")),
        &js_pretty(&status),
    )?;
    Ok(status)
}

fn count_of(v: &JsValue, key: &str) -> f64 {
    match field(v, key) {
        Some(JsValue::Number(n)) => *n,
        _ => 0.0,
    }
}

/// The command that re-runs one pair of a label.
fn rerun(label: &str, pair: Option<&str>) -> String {
    match pair {
        Some(p) => format!("     cargo dev eval score {label} --pairs {p}"),
        None => format!("     cargo dev eval score {label}"),
    }
}

/// Why a pair's card is refused for lost LLM answers — empty when not.
pub fn provider_error_refusal(status: &JsValue, label: &str) -> Vec<String> {
    let Some(pe) = field(status, "providerErrors") else {
        return Vec::new();
    };
    if field(pe, "refused") != Some(&JsValue::Bool(true)) {
        return Vec::new();
    }
    let pair = field(pe, "pair").and_then(JsValue::as_str);
    let version = field(status, "version")
        .and_then(JsValue::as_str)
        .unwrap_or("?");
    let mut lines = vec![format!(
        "!! REFUSED to score {}: the run lost LLM answers — every failed call is a hole in the names, and the KPIs cannot tell a lost name from noise (rule 11).",
        pair.unwrap_or(version)
    )];
    if let Some(s) = field(pe, "scored").filter(|s| count_of(s, "total") > 0.0) {
        let parts: Vec<String> = [
            ("naming", "naming"),
            ("modules", "modules"),
            ("vendor", "vendor"),
            ("split", "split"),
            ("tree reviser", "treeReviser"),
        ]
        .iter()
        .filter(|(_, k)| count_of(s, k) > 0.0)
        .map(|(name, k)| format!("{name} {}", count_of(s, k)))
        .collect();
        lines.push(format!(
            "   scored leg: {} LLM call(s) failed after every retry ({}).",
            count_of(s, "total"),
            parts.join(", ")
        ));
    }
    let rebase = count_of(pe, "rebase");
    if rebase > 0.0 {
        lines.push(format!(
            "   rebuilt base: {rebase} LLM call(s) failed after every retry — the base the pair is diffed against has holes."
        ));
    }
    lines.push(
        "   No card was written. Check the endpoint (.humanify.local.json or --endpoint), then re-run the pair:"
            .into(),
    );
    lines.push(rerun(label, pair));
    lines
}

/// Why a pair's card is refused for a SKIPPED library decision — empty
/// when not.
pub fn library_refusal(status: &JsValue, label: &str) -> Vec<String> {
    let Some(lm) = field(status, "libraryMatching") else {
        return Vec::new();
    };
    if field(lm, "refused") != Some(&JsValue::Bool(true)) {
        return Vec::new();
    }
    let pair = field(lm, "pair").and_then(JsValue::as_str);
    let version = field(status, "version")
        .and_then(JsValue::as_str)
        .unwrap_or("?");
    let mut lines = vec![format!(
        "!! REFUSED to score {}: a run SKIPPED library name matching (no library index for the bundle's kind) — its tree is off mode's, and scoring it under this label would pass it off as a names run.",
        pair.unwrap_or(version)
    )];
    if let Some(s) = field(lm, "scored")
        .filter(|s| field(s, "outcome").and_then(JsValue::as_str) == Some("skipped"))
    {
        let kind = field(s, "kind").and_then(JsValue::as_str).unwrap_or("?");
        let why = field(s, "why")
            .and_then(JsValue::as_str)
            .unwrap_or("skipped");
        lines.push(format!("   scored leg ({kind} bundle): {why}"));
    }
    if let Some(skip) = field(lm, "rebaseSkipped").and_then(JsValue::as_str) {
        lines.push(format!("   rebuilt base: {skip}"));
    }
    lines.push(
        "   No card was written. Pass the library index explicitly (pairs.json libraryIndex, or --library-index), then re-run the pair:"
            .into(),
    );
    lines.push(rerun(label, pair));
    lines
}

// ─── run manifest (run-manifest.ts) ──────────────────────────────────────

/// Whether a prior path is a REBASED tree or the committed archive.
pub fn prior_kind_of(prior: &str) -> &'static str {
    if prior.is_empty() {
        "unknown"
    } else if prior.contains("-rebased/") || prior.ends_with("-rebased") {
        "rebased"
    } else {
        "archive"
    }
}

fn str_at<'a>(v: &'a JsValue, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for k in path {
        cur = field(cur, k)?;
    }
    cur.as_str()
}

fn at<'a>(v: &'a JsValue, path: &[&str]) -> Option<&'a JsValue> {
    let mut cur = v;
    for k in path {
        cur = field(cur, k)?;
    }
    Some(cur)
}

/// The base mode one manifest was scored on (`baseModeOf`).
pub fn base_mode_of(m: &JsValue) -> &str {
    if let Some(b) = str_at(m, &["inputs", "baseMode"]) {
        return b;
    }
    match str_at(m, &["inputs", "priorKind"]) {
        Some("rebased") => "seeded",
        Some("archive") => "archive",
        _ => "unknown",
    }
}

/// The run's kill switches (the binary's record), sorted; None = unknown.
fn active_switches(m: &JsValue) -> Option<Vec<String>> {
    let sw = at(m, &["config", "resolved", "switches"]).or(None);
    if let Some(sw) = sw {
        let mut out = Vec::new();
        for k in ["disabled", "probes"] {
            if let Some(JsValue::Array(items)) = field(sw, k) {
                out.extend(items.iter().filter_map(|x| x.as_str().map(str::to_string)));
            }
        }
        out.sort();
        return Some(out);
    }
    match at(m, &["config", "killSwitches"]) {
        Some(JsValue::Array(items)) => Some(
            items
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
        ),
        _ => None,
    }
}

fn is_true(v: Option<&JsValue>) -> bool {
    v == Some(&JsValue::Bool(true))
}

/// The sentences a reader must see: empty for a clean, cold, default run.
/// One entry per way a run can be quietly untrustworthy (run-manifest.ts
/// `WARNING_CHECKS`, each with the incident that earned it).
pub fn manifest_warnings(m: &JsValue) -> Vec<String> {
    let pair = str_at(m, &["pair"]).unwrap_or("?");
    let mut out = Vec::new();
    let cache_written = at(m, &["config", "cache", "written"]) == Some(&num(0));
    if is_true(at(m, &["config", "cache", "enabled"])) && cache_written {
        out.push("LLM CACHE ON and 0 entries written — every prompt was REPLAYED, not asked. This run cannot settle anything about naming (measurement-pitfalls rule 10, born from exactly this).".to_string());
    }
    if str_at(m, &["inputs", "priorKind"]) == Some("archive") {
        out.push("scored against the ARCHIVE prior, not a rebased one. Expect KPIs to read far worse than a like-for-like base would give.".to_string());
    }
    if base_mode_of(m) == "seeded" {
        out.push("base SEEDED by the archive (--base-mode seeded): v-1 inherits the archive's names, so this pair is comparable only to other seeded runs — every reference scored before 2026-10-03 — not to the scratch default.".to_string());
    }
    match active_switches(m) {
        Some(on) if !on.is_empty() => out.push(format!(
            "{} kill switch(es) ACTIVE — this run is not the default pipeline: {}",
            on.len(),
            on.join(", ")
        )),
        Some(_) => {}
        None => out.push("the binary wrote no configuration record (no stats file): this run's settings, kill switches included, are UNKNOWN.".to_string()),
    }
    if str_at(m, &["library", "outcome"]) == Some("skipped") {
        out.push(format!(
            "this run SKIPPED library name matching (no library index for {} bundles): its tree is off mode's, whatever its config says. {}",
            str_at(m, &["library", "kind"]).unwrap_or("undefined"),
            str_at(m, &["library", "why"]).unwrap_or("")
        ));
    }
    let commit = str_at(m, &["provenance", "commit"]).unwrap_or("");
    if is_true(at(m, &["provenance", "dirty"])) {
        out.push(format!("the working tree was DIRTY at {commit} — this run corresponds to no commit and cannot be reproduced from the sha."));
    }
    out.extend(bin_warnings(m, commit));
    if let Some(JsValue::Number(code)) = at(m, &["outcome", "exitCode"])
        && *code != 0.0
    {
        let first = match at(m, &["outcome", "errors"]) {
            Some(JsValue::Array(e)) if !e.is_empty() => {
                format!(" First error: {}", e[0].as_str().unwrap_or(""))
            }
            _ => String::new(),
        };
        out.push(format!("pipeline EXIT {code} — it declared its own output invalid. Any KPI below is computed from a rejected tree.{first}"));
    }
    out.into_iter().map(|w| format!("{pair}: {w}")).collect()
}

fn bin_warnings(m: &JsValue, commit: &str) -> Vec<String> {
    let Some(bin) = at(m, &["pipeline", "bin"]) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let bin_commit = str_at(bin, &["commit"]).unwrap_or("");
    let bin_dirty = is_true(field(bin, "dirty"));
    if bin_dirty || !same_commit(bin_commit, commit) {
        let built = if bin_commit.is_empty() {
            "an UNKNOWN commit".to_string()
        } else {
            bin_commit.chars().take(12).collect()
        };
        out.push(format!(
            "scored by a Rust binary built from {built}{}, not this run's commit {commit} — the label's commit does not describe the code that ran.",
            if bin_dirty { " (DIRTY tree)" } else { "" }
        ));
    }
    if field(bin, "sha256") != field(bin, "buildSha256") {
        out.push("the Rust binary changed on disk after the harness built it for this label — this pair ran a different file than the label records.".to_string());
    }
    out
}

// ─── boot and self-hop verdicts (run.sh) ─────────────────────────────────

/// `<v>-boot.json` (run.sh's printf record).
pub fn boot_verdict_line(version: &str, prompt: &str, model: &str, ok: bool) -> String {
    js_compact_line(&obj(vec![(
        "boot",
        obj(vec![
            ("version", JsValue::str(version)),
            ("prompt", JsValue::str(prompt)),
            ("model", JsValue::str(model)),
            ("ok", JsValue::Bool(ok)),
        ]),
    )]))
}
