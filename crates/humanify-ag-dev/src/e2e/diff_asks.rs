//! `cargo dev diff-asks <a.jsonl> <b.jsonl>` (successor of
//! scripts/diff-asks.ts): the ask-log comparator, the decision-level
//! sibling of the byte-diff neutrality gate, with zero LLM calls.
//!
//! Compares two `--dump-asks` logs (`cargo dev ask-trace` writes them) and
//! prints a human delta: asks only in A, only in B, and asks that match but
//! CHANGED. Uses: (a) two BINARIES on one input — a should-change-nothing
//! refactor must give an empty diff (exit 0); (b) two VERSIONS' runs;
//! (c) a behaviour change's blast radius.
//!
//! Matching: asks pair on (site, scope, round, identifiers, isRetry,
//! retryCause, retryCauseDetail) — the ask's identity, not its schedule.
//! reason, priorContext, wave, phase, usedNamesCount, promptVariant and
//! scopeKind are COMPARED fields. Exit 0 only when the logs are identical
//! row for row; exit 1 prints the delta. Totals first. The rows are read
//! as JS values and printed the way the TS printed them (a template
//! literal's `undefined`, JSON.stringify's spellings), so the text is the
//! same.

use std::collections::BTreeMap;
use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue, cmp_utf16, number_to_string, stringify};

use crate::HostEnv;
use crate::settings::Parsed;

/// A pair matched on identity whose compared fields differ.
#[derive(Debug)]
pub struct Changed<'a> {
    pub a: &'a JsObject,
    pub b: &'a JsObject,
    pub fields: Vec<String>,
}

/// The multiset diff of two ask logs.
#[derive(Debug)]
pub struct AskDelta<'a> {
    pub only_a: Vec<&'a JsObject>,
    pub only_b: Vec<&'a JsObject>,
    pub changed: Vec<Changed<'a>>,
    pub identical: bool,
}

/// `r.field ?? null`, as a value.
fn or_null(r: &JsObject, field: &str) -> JsValue {
    r.get(field).cloned().unwrap_or(JsValue::Null)
}

/// The value as a JS template literal interpolates it.
fn tpl(v: Option<&JsValue>) -> String {
    match v {
        None => "undefined".into(),
        Some(JsValue::Null) => "null".into(),
        Some(JsValue::Bool(b)) => b.to_string(),
        Some(JsValue::Number(x)) => number_to_string(*x),
        Some(JsValue::String(s)) => s.clone(),
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|i| match i {
                JsValue::Null => String::new(),
                other => tpl(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(JsValue::Object(_)) => "[object Object]".into(),
    }
}

/// The ask's identity — what two rows must agree on to be the same ask.
fn identity(r: &JsObject) -> String {
    let field = |f: &str| r.get(f).cloned().unwrap_or(JsValue::Null);
    stringify(&JsValue::Array(vec![
        field("site"),
        field("scope"),
        field("round"),
        field("identifiers"),
        field("isRetry"),
        or_null(r, "retryCause"),
        or_null(r, "retryCauseDetail"),
    ]))
}

/// The compared fields, in the order a CHANGED line names them.
const COMPARED: [&str; 7] = [
    "reason",
    "priorContext",
    "wave",
    "phase",
    "usedNamesCount",
    "promptVariant",
    "scopeKind",
];

/// Which compared fields two identified asks disagree on.
fn differing(a: &JsObject, b: &JsObject) -> Vec<String> {
    COMPARED
        .iter()
        .filter(|f| or_null(a, f) != or_null(b, f))
        .map(|f| f.to_string())
        .collect()
}

/// The multiset diff of two ask logs.
pub fn diff_asks<'a>(a: &'a [JsObject], b: &'a [JsObject]) -> AskDelta<'a> {
    let mut pool: Vec<(String, Vec<&'a JsObject>)> = Vec::new();
    for r in b {
        let k = identity(r);
        match pool.iter_mut().find(|(key, _)| *key == k) {
            Some((_, bucket)) => bucket.push(r),
            None => pool.push((k, vec![r])),
        }
    }
    let mut only_a = Vec::new();
    let mut changed = Vec::new();
    for ra in a {
        let k = identity(ra);
        let bucket = pool.iter_mut().find(|(key, _)| *key == k);
        let Some((_, bucket)) = bucket.filter(|(_, b)| !b.is_empty()) else {
            only_a.push(ra);
            continue;
        };
        let rb = bucket.remove(0);
        let fields = differing(ra, rb);
        if !fields.is_empty() {
            changed.push(Changed {
                a: ra,
                b: rb,
                fields,
            });
        }
    }
    let only_b: Vec<&JsObject> = pool.into_iter().flat_map(|(_, rows)| rows).collect();
    let identical = only_a.is_empty() && only_b.is_empty() && changed.is_empty();
    AskDelta {
        only_a,
        only_b,
        changed,
        identical,
    }
}

/// Parse one `--dump-asks` log: one JSON object per non-blank line.
pub fn read_asks(path: &Path) -> Result<Vec<JsObject>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match JsValue::parse(line) {
            Ok(JsValue::Object(o)) => rows.push(o),
            Ok(_) => {
                return Err(format!(
                    "{}:{}: not a JSONL row: not an object",
                    path.display(),
                    i + 1
                ));
            }
            Err(e) => {
                return Err(format!(
                    "{}:{}: not a JSONL row: {e}",
                    path.display(),
                    i + 1
                ));
            }
        }
    }
    Ok(rows)
}

fn describe(r: &JsObject) -> String {
    let cause = if r.get("isRetry") == Some(&JsValue::Bool(true)) {
        format!(
            " retry={}",
            match r.get("retryCause") {
                None | Some(JsValue::Null) => "?".to_string(),
                v => tpl(v),
            }
        )
    } else {
        String::new()
    };
    let identifiers = match r.get("identifiers") {
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|i| tpl(Some(i)))
            .collect::<Vec<_>>()
            .join(", "),
        other => tpl(other),
    };
    format!(
        "[{}] {} (round {},{cause} {}) [{identifiers}]",
        tpl(r.get("site")),
        tpl(r.get("scope")),
        tpl(r.get("round")),
        tpl(r.get("reason")),
    )
}

fn reason_of(r: &JsObject) -> String {
    tpl(r.get("reason"))
}

/// `  reason: A 3 / B 5` per reason, both logs' counts side by side.
fn reason_counts(a: &[JsObject], b: &[JsObject]) -> Vec<String> {
    let count = |rows: &[JsObject]| {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for r in rows {
            *m.entry(reason_of(r)).or_default() += 1;
        }
        m
    };
    let (ca, cb) = (count(a), count(b));
    let mut keys: Vec<&String> = ca.keys().chain(cb.keys()).collect();
    keys.sort_by(|x, y| cmp_utf16(x, y));
    keys.dedup();
    keys.iter()
        .map(|k| {
            format!(
                "  {k}: A {} / B {}",
                ca.get(*k).unwrap_or(&0),
                cb.get(*k).unwrap_or(&0)
            )
        })
        .collect()
}

/// The rows under a group heading, per scope (sorted), arrival order
/// within a scope.
fn group_lines(label: &str, rows: &[&JsObject]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut scopes: Vec<String> = Vec::new();
    for r in rows {
        let s = tpl(r.get("scope"));
        if !scopes.contains(&s) {
            scopes.push(s);
        }
    }
    scopes.sort_by(|x, y| cmp_utf16(x, y));
    let mut out = vec![String::new(), format!("— {label} ({}) —", rows.len())];
    for scope in &scopes {
        for r in rows.iter().filter(|r| tpl(r.get("scope")) == *scope) {
            out.push(format!("  {}", describe(r)));
        }
    }
    out
}

/// The human delta, totals first.
pub fn format_delta(
    label_a: &str,
    label_b: &str,
    a: &[JsObject],
    b: &[JsObject],
    d: &AskDelta,
) -> String {
    let mut lines = vec![
        format!("A: {label_a} — {} ask(s)", a.len()),
        format!("B: {label_b} — {} ask(s)", b.len()),
    ];
    lines.extend(reason_counts(a, b));
    lines.push(format!(
        "IDENTICAL: {} — only-in-A {}, only-in-B {}, changed {}",
        if d.identical { "yes" } else { "no" },
        d.only_a.len(),
        d.only_b.len(),
        d.changed.len()
    ));
    lines.extend(group_lines("ONLY IN A", &d.only_a));
    lines.extend(group_lines("ONLY IN B", &d.only_b));
    if !d.changed.is_empty() {
        lines.push(String::new());
        lines.push(format!("— CHANGED ({}) —", d.changed.len()));
        for c in &d.changed {
            let moves: Vec<String> = c
                .fields
                .iter()
                .map(|f| {
                    format!(
                        "{f} {} → {}",
                        stringify(&or_null(c.a, f)),
                        stringify(&or_null(c.b, f))
                    )
                })
                .collect();
            lines.push(format!("  {}: {}", describe(c.a), moves.join(", ")));
        }
    }
    lines.join("\n")
}

const USAGE: &str = "usage: cargo dev diff-asks <a.jsonl> <b.jsonl>\n";

/// Run the verb: exit 0 identical, 1 a delta, 2 a usage or read error.
pub fn run(p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let [a, b] = p.positionals.as_slice() else {
        return (2, String::new(), USAGE.into());
    };
    let read = |f: &str| read_asks(Path::new(f));
    let (rows_a, rows_b) = match (read(a), read(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return (2, String::new(), format!("diff-asks: {e}\n")),
    };
    let delta = diff_asks(&rows_a, &rows_b);
    let text = format!("{}\n", format_delta(a, b, &rows_a, &rows_b, &delta));
    (u8::from(!delta.identical), text, String::new())
}

#[cfg(test)]
#[path = "diff_asks_test.rs"]
mod diff_asks_test;
