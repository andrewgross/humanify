//! The per-pair evidence page (TS: `034/trail-report.ts` over
//! `034/diff-ledger.ts`): identifier accounting from the run's
//! `--diagnostics` trail (TOTAL at the top, REMAINING at the bottom), the
//! per-strategy attempt funnel, post-settle flags, the LLM endgame, and the
//! diff ledger — what drove every diff line of the bundle, statement by
//! statement (real change, naming-noise shapes, family buckets).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use super::analyze::PairInputs;
use super::stmts::{Stmt, bundle_statements};
use super::text::{EditPairer, edited_line_counts, words};

/// `n.toLocaleString("en-US")` for an integer.
pub fn fmt_int(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn pct(n: i64, total: i64) -> String {
    if total > 0 {
        format!(
            "{}%",
            humanify_ag_model::js::to_fixed(n as f64 / total as f64 * 100.0, 2)
        )
    } else {
        "-".into()
    }
}

// ---------------------------------------------------------------- the ledger

#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub shape: String,
    pub st: i64,
    pub ln: i64,
    pub edited_ln: i64,
}

/// The bundle's diff, statement by statement.
#[derive(Clone, Debug, Default)]
pub struct DiffLedger {
    pub fresh_total_ln: i64,
    pub prior_total_ln: i64,
    pub modified_pairs: i64,
    pub modified_fresh_ln: i64,
    pub modified_edited_ln: i64,
    pub modified_edited_prior_ln: i64,
    pub pure_added_ln: i64,
    pub pure_added_st: i64,
    pub pure_removed_ln: i64,
    pub pure_removed_st: i64,
    pub honest_real_ln: i64,
    pub diff_visible_fresh_ln: i64,
    pub diff_visible_prior_ln: i64,
    pub clean_visible_ln: i64,
    pub noise_edited_ln: i64,
    pub shapes: Vec<Shape>,
    pub family_st: i64,
    pub family_ln: i64,
    pub family_edited_ln: i64,
    pub moved_st: i64,
    pub clean_ln: i64,
}

/// Words with their separators (`seps.len() == words.len() + 1`).
fn tokenize(text: &str) -> (Vec<&str>, Vec<&str>) {
    let mut ws = Vec::new();
    let mut seps = Vec::new();
    let mut last = 0;
    for (at, w) in words(text) {
        seps.push(&text[last..at]);
        ws.push(w);
        last = at + w.len();
    }
    seps.push(&text[last..]);
    (ws, seps)
}

fn is_property_position(seps: &[&str], i: usize) -> bool {
    let before = seps[i].trim_end();
    if before.ends_with('.') || before.ends_with("?.") {
        return true;
    }
    let after = seps[i + 1].trim_start();
    after.starts_with(':') && !after.starts_with("::")
}

/// Names a statement's text declares (`var/let/const x`, `function x`,
/// `class x`, declarator lists, parameter lists) — a text heuristic.
fn declared_names(text: &str) -> HashSet<&str> {
    let mut out = HashSet::new();
    let ws = words(text);
    for (k, (at, w)) in ws.iter().enumerate() {
        let next = ws.get(k + 1);
        let prev_char = text[..*at].chars().next_back();
        let decl_kw = matches!(*w, "var" | "let" | "const");
        let head_kw =
            matches!(*w, "function" | "class") && prev_char.is_none_or(char::is_whitespace);
        if (decl_kw || head_kw)
            && let Some((nat, name)) = next
            && text[at + w.len()..*nat].chars().all(char::is_whitespace)
            && *nat > at + w.len()
        {
            out.insert(*name);
        }
        // `\b(var|let|const)\s+([^;{()]*)`, each comma part's leading name.
        let rest = &text[at + w.len()..];
        if decl_kw && rest.starts_with(char::is_whitespace) {
            let end = rest.find([';', '{', '(', ')']).unwrap_or(rest.len());
            for part in rest[..end].split(',') {
                if let Some(&(0, id)) = words(part.trim()).first() {
                    out.insert(id);
                }
            }
        }
    }
    // `( a, b ) =>` and `( a, b ) {` parameter lists.
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(open) = text[i..].find('(').map(|p| p + i) {
        let Some(close) = text[open + 1..].find([')', '(']).map(|p| p + open + 1) else {
            break;
        };
        if b[close] == b'(' {
            i = close;
            continue;
        }
        let after = text[close + 1..].trim_start();
        if after.starts_with("=>") || after.starts_with('{') {
            for part in text[open + 1..close].split(',') {
                let p = part.trim();
                let p = p.strip_prefix("...").unwrap_or(p);
                if let Some((0, id)) = words(p).first().copied() {
                    out.insert(id);
                }
            }
        }
        i = close;
    }
    out
}

/// One unique-twin noise statement's diff shape.
fn noise_shape(fresh: &str, prior: &str) -> &'static str {
    let (fw, fs) = tokenize(fresh);
    let (pw, ps) = tokenize(prior);
    if fw.len() != pw.len() || fs != ps {
        return "misaligned";
    }
    let declared = declared_names(fresh);
    let (mut internal, mut outer, mut props) = (0, 0, 0);
    for i in 0..fw.len() {
        if fw[i] == pw[i] {
            continue;
        }
        if is_property_position(&fs, i) || is_property_position(&ps, i) {
            props += 1;
        } else if declared.contains(fw[i]) {
            internal += 1;
        } else {
            outer += 1;
        }
    }
    if props > 0 {
        "property-drift"
    } else if internal > 0 && outer == 0 {
        "internal-rename"
    } else if outer > 0 && internal == 0 {
        "outer-echo"
    } else {
        "mixed-rename"
    }
}

/// statement hash -> file, from a ledger's `files` object (1:1 hashes
/// only; a ledger whose `files` is a list records none).
fn file_by_hash(ledger: &Path) -> HashMap<String, String> {
    let v: Value = std::fs::read_to_string(ledger)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let mut seen: BTreeMap<String, Option<String>> = BTreeMap::new();
    if let Some(files) = v["files"].as_object() {
        for (file, entry) in files {
            for h in entry["hashes"].as_array().into_iter().flatten() {
                let h = h.as_str().unwrap_or_default().to_string();
                let dup = seen.contains_key(&h);
                seen.insert(h, if dup { None } else { Some(file.clone()) });
            }
        }
    }
    seen.into_iter()
        .filter_map(|(h, f)| Some((h, f?)))
        .collect()
}

/// Modified pairs: novel fresh statements paired with removed ones by
/// masked head + best token overlap (>= 0.5); the rest is pure added /
/// removed.
fn modified_pairs(
    fresh: &[Stmt],
    prior: &[Stmt],
    by_hash: &HashMap<&str, Vec<&Stmt>>,
    fresh_count: &HashMap<&str, usize>,
    d: &mut DiffLedger,
) {
    let novel: Vec<&Stmt> = fresh
        .iter()
        .filter(|s| !by_hash.contains_key(s.hash.as_str()))
        .collect();
    let removed: Vec<&Stmt> = prior
        .iter()
        .filter(|s| !fresh_count.contains_key(s.hash.as_str()))
        .collect();
    let mut pairer = EditPairer::new(removed.iter().map(|s| s.text.as_str()).collect());
    for s in &novel {
        match pairer.take(&s.text) {
            Some(c) => {
                d.modified_pairs += 1;
                d.modified_fresh_ln += s.lines as i64;
                let (f, p) = edited_line_counts(&s.text, &removed[c].text);
                d.modified_edited_ln += f as i64;
                d.modified_edited_prior_ln += p as i64;
            }
            None => {
                d.pure_added_st += 1;
                d.pure_added_ln += s.lines as i64;
            }
        }
    }
    for (i, s) in removed.iter().enumerate() {
        if !pairer.used(i) {
            d.pure_removed_st += 1;
            d.pure_removed_ln += s.lines as i64;
        }
    }
}

pub fn diff_ledger(fresh: &[Stmt], prior: &[Stmt], ledgers: Option<(&Path, &Path)>) -> DiffLedger {
    let mut by_hash: HashMap<&str, Vec<&Stmt>> = HashMap::new();
    for s in prior {
        by_hash.entry(&s.hash).or_default().push(s);
    }
    let mut fresh_count: HashMap<&str, usize> = HashMap::new();
    for s in fresh {
        *fresh_count.entry(&s.hash).or_default() += 1;
    }
    let mut d = DiffLedger::default();
    let (mut added_ln, mut family_edited_prior) = (0i64, 0i64);
    let mut shapes: Vec<(String, Shape, i64)> = Vec::new();
    for s in fresh {
        let lines = s.lines as i64;
        let Some(twins) = by_hash.get(s.hash.as_str()) else {
            added_ln += lines;
            continue;
        };
        if twins.iter().any(|t| t.text == s.text) {
            d.clean_ln += lines;
            continue;
        }
        if twins.len() != 1 || fresh_count[s.hash.as_str()] != 1 {
            d.family_st += 1;
            d.family_ln += lines;
            let mut best: Option<(usize, usize)> = None;
            for t in twins {
                let e = edited_line_counts(&s.text, &t.text);
                if best.is_none_or(|b| e.0 < b.0) {
                    best = Some(e);
                }
            }
            let (f, p) = best.unwrap_or((s.lines, 0));
            d.family_edited_ln += f as i64;
            family_edited_prior += p as i64;
            continue;
        }
        let shape = noise_shape(&s.text, &twins[0].text);
        let (f, p) = edited_line_counts(&s.text, &twins[0].text);
        let idx = match shapes.iter().position(|(k, _, _)| k == shape) {
            Some(i) => i,
            None => {
                shapes.push((
                    shape.into(),
                    Shape {
                        shape: shape.into(),
                        ..Shape::default()
                    },
                    0,
                ));
                shapes.len() - 1
            }
        };
        let e = &mut shapes[idx];
        e.1.st += 1;
        e.1.ln += lines;
        e.1.edited_ln += f as i64;
        e.2 += p as i64;
    }
    modified_pairs(fresh, prior, &by_hash, &fresh_count, &mut d);
    if let Some((fl, pl)) = ledgers {
        let (ff, pf) = (file_by_hash(fl), file_by_hash(pl));
        d.moved_st = ff
            .iter()
            .filter(|(h, f)| pf.get(*h).is_some_and(|p| p != *f))
            .count() as i64;
    }
    let noise_ln: i64 = shapes.iter().map(|s| s.1.ln).sum::<i64>() + d.family_ln;
    d.noise_edited_ln = shapes.iter().map(|s| s.1.edited_ln).sum::<i64>() + d.family_edited_ln;
    let noise_edited_prior: i64 = shapes.iter().map(|s| s.2).sum::<i64>() + family_edited_prior;
    d.fresh_total_ln = d.clean_ln + noise_ln + added_ln;
    d.prior_total_ln = prior.iter().map(|s| s.lines as i64).sum();
    d.diff_visible_fresh_ln = d.noise_edited_ln + d.modified_edited_ln + d.pure_added_ln;
    d.diff_visible_prior_ln = noise_edited_prior + d.modified_edited_prior_ln + d.pure_removed_ln;
    d.honest_real_ln = d.modified_edited_ln + d.pure_added_ln + d.pure_removed_ln;
    d.clean_visible_ln = d.fresh_total_ln - d.diff_visible_fresh_ln;
    let mut sorted: Vec<Shape> = shapes.into_iter().map(|s| s.1).collect();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.edited_ln));
    d.shapes = sorted;
    d
}

// ------------------------------------------------------------------ the page

fn num(v: &Value) -> i64 {
    v.as_f64().unwrap_or(0.0) as i64
}

/// One strategy's outcome counts and refusal reasons, in first-seen order.
type FunnelEntry = (String, BTreeMap<String, i64>, Vec<(String, i64)>);

/// The `(strategy, applied, rejected, abstained, vote, top reasons)` rows.
fn funnel_rows(trails: &[Value]) -> Vec<(String, [i64; 4], String)> {
    let mut funnel: Vec<FunnelEntry> = Vec::new();
    for entry in trails {
        for a in entry["trail"].as_array().into_iter().flatten() {
            let strategy = a["strategy"].as_str().unwrap_or_default().to_string();
            let outcome = a["outcome"].as_str().unwrap_or_default().to_string();
            let idx = match funnel.iter().position(|f| f.0 == strategy) {
                Some(i) => i,
                None => {
                    funnel.push((strategy, BTreeMap::new(), Vec::new()));
                    funnel.len() - 1
                }
            };
            let f = &mut funnel[idx];
            *f.1.entry(outcome.clone()).or_default() += 1;
            if let Some(r) = a["reason"].as_str().filter(|r| !r.is_empty())
                && outcome != "vote"
            {
                match f.2.iter_mut().find(|(k, _)| k == r) {
                    Some(e) => e.1 += 1,
                    None => f.2.push((r.to_string(), 1)),
                }
            }
        }
    }
    let get = |m: &BTreeMap<String, i64>, k: &str| m.get(k).copied().unwrap_or(0);
    funnel.sort_by_key(|f| std::cmp::Reverse(get(&f.1, "applied")));
    funnel
        .into_iter()
        .map(|(s, counts, mut reasons)| {
            reasons.sort_by_key(|r| std::cmp::Reverse(r.1));
            let top: Vec<String> = reasons
                .iter()
                .take(3)
                .map(|(r, n)| format!("{r} {n}"))
                .collect();
            (
                s,
                [
                    get(&counts, "applied"),
                    get(&counts, "rejected"),
                    get(&counts, "abstained"),
                    get(&counts, "vote"),
                ],
                top.join(", "),
            )
        })
        .collect()
}

fn sorted_counts(v: &Value) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = v
        .as_object()
        .map(|m| m.iter().map(|(k, x)| (k.clone(), num(x))).collect())
        .unwrap_or_default();
    out.sort_by_key(|o| std::cmp::Reverse(o.1));
    out
}

fn terminal_state(ts: &Value, total: i64) -> String {
    if !ts.is_object() {
        return String::new();
    }
    let rows: String = sorted_counts(&ts["namedByTier"])
        .iter()
        .map(|(t, n)| {
            format!(
                "<tr><td>named-by {t}</td><td class=n>{}</td><td class=n>{}</td></tr>",
                fmt_int(*n),
                pct(*n, total)
            )
        })
        .collect();
    let unaccounted: Vec<&str> = ts["mintedUnaccounted"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let residue = if unaccounted.is_empty() {
        "<p>0 unaccounted — bookkeeping complete, the shape classifier is redundant.</p>"
            .to_string()
    } else {
        format!(
            "<p><b>{} unaccounted minted names</b> (no recorded decision): {}</p>",
            fmt_int(unaccounted.len() as i64),
            unaccounted.join(" ")
        )
    };
    format!(
        "<h2>Terminal state (ground truth)</h2>\n<table><tr><th>end state</th><th>bindings</th><th>of total</th></tr>\n<tr><td><b>TOTAL bindings</b></td><td class=n><b>{}</b></td><td class=n>100%</td></tr>{rows}\n<tr><td>llm</td><td class=n>{}</td><td class=n>{}</td></tr>\n<tr><td><b>minted, accounted</b></td><td class=n><b>{}</b></td><td class=n></td></tr>\n<tr><td><b>REMAINING unaccounted</b></td><td class=n><b>{}</b></td><td class=n></td></tr></table>\n{residue}",
        fmt_int(total),
        fmt_int(num(&ts["llmNamed"])),
        pct(num(&ts["llmNamed"]), total),
        fmt_int(num(&ts["mintedAccounted"])),
        fmt_int(unaccounted.len() as i64),
    )
}

fn diff_section(l: &DiffLedger) -> String {
    let share = |n: i64| pct(n, l.diff_visible_fresh_ln);
    let shape_rows: String = l
        .shapes
        .iter()
        .map(|s| {
            format!(
                "<tr><td>noise: {}</td><td class=n><b>~{}</b></td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td></tr>",
                s.shape,
                fmt_int(s.edited_ln),
                fmt_int(s.ln),
                fmt_int(s.st),
                share(s.edited_ln)
            )
        })
        .collect();
    format!(
        "<h2>Diff ledger — what a line diff actually shows</h2>
<div class=\"strip\">
  <div class=\"total\"><b>{fresh}</b>TOTAL lines (fresh)</div>
  <div><b>{cv}</b>clean / diff-invisible ({cvp})</div>
  <div><b>+{dvf} / −{dvp}</b>diff-visible lines</div>
  <div class=\"remaining\"><b>~{ne}</b>of which naming noise ({nes})</div>
</div>
<table><tr><th>bucket</th><th>diff-visible ln</th><th>in touched statements (mass)</th><th>statements</th><th>of visible +</th></tr>
<tr><td><b>TOTAL fresh output</b></td><td class=n></td><td class=n><b>{fresh}</b></td><td class=n></td><td class=n></td></tr>
<tr><td>clean — untouched statements</td><td class=n>0</td><td class=n>{clean}</td><td class=n></td><td class=n></td></tr>
<tr><td>clean — unchanged context inside touched statements</td><td class=n>0</td><td class=n>{ctx}</td><td class=n></td><td class=n></td></tr>
<tr><td>real: modified statements ({mp} pairs)</td><td class=n><b>~{me}</b></td><td class=n>{mf}</td><td class=n>{mp}</td><td class=n>{mes}</td></tr>
<tr><td>real: pure added</td><td class=n><b>{pa}</b></td><td class=n>{pa}</td><td class=n>{pas}</td><td class=n>{pash}</td></tr>
<tr><td>real: pure removed (− side)</td><td class=n><b>{pr}</b></td><td class=n>{pr}</td><td class=n>{prs}</td><td class=n></td></tr>
{shape_rows}
<tr><td>noise: family-bucket</td><td class=n><b>~{fe}</b></td><td class=n>{fl}</td><td class=n>{fs}</td><td class=n>{fes}</td></tr>
<tr><td>file moves</td><td class=n>—</td><td class=n>—</td><td class=n>{mv}</td><td class=n></td></tr></table>
<p style=\"color:#888\">\"Diff-visible\" approximates what `diff` prints (per-pair set-based line comparison); unchanged context inside touched statements counts as CLEAN. Statement mass is kept as context — it is what the hash-level eval KPIs measure. Honest real change ~{hr} ln.</p>",
        fresh = fmt_int(l.fresh_total_ln),
        cv = fmt_int(l.clean_visible_ln),
        cvp = pct(l.clean_visible_ln, l.fresh_total_ln),
        dvf = fmt_int(l.diff_visible_fresh_ln),
        dvp = fmt_int(l.diff_visible_prior_ln),
        ne = fmt_int(l.noise_edited_ln),
        nes = share(l.noise_edited_ln),
        clean = fmt_int(l.clean_ln),
        ctx = fmt_int(l.clean_visible_ln - l.clean_ln),
        mp = fmt_int(l.modified_pairs),
        me = fmt_int(l.modified_edited_ln),
        mf = fmt_int(l.modified_fresh_ln),
        mes = share(l.modified_edited_ln),
        pa = fmt_int(l.pure_added_ln),
        pas = fmt_int(l.pure_added_st),
        pash = share(l.pure_added_ln),
        pr = fmt_int(l.pure_removed_ln),
        prs = fmt_int(l.pure_removed_st),
        fe = fmt_int(l.family_edited_ln),
        fl = fmt_int(l.family_ln),
        fs = fmt_int(l.family_st),
        fes = share(l.family_edited_ln),
        mv = fmt_int(l.moved_st),
        hr = fmt_int(l.honest_real_ln),
    )
}

const STYLE: &str = "<style>
  body { font: 14px/1.5 -apple-system, sans-serif; margin: 2rem auto; max-width: 70rem; padding: 0 1rem; color: #1a1a1a; }
  @media (prefers-color-scheme: dark) { body { background: #14161a; color: #d6d6d6; } th { background: #22252b; } tr:nth-child(even) td { background: #1a1d22; } .strip { background: #22252b; } }
  h1 { font-size: 1.3rem; } h2 { font-size: 1.05rem; margin-top: 2rem; }
  table { border-collapse: collapse; width: 100%; }
  th, td { text-align: left; padding: .3rem .6rem; border-bottom: 1px solid #8884; }
  th { background: #f0f0f0; }
  td.n { text-align: right; font-variant-numeric: tabular-nums; }
  td.reasons { color: #888; font-size: .85em; }
  .strip { display: flex; gap: 2rem; background: #f5f5f5; padding: .8rem 1rem; border-radius: 8px; margin: 1rem 0; flex-wrap: wrap; }
  .strip b { display: block; font-size: 1.3rem; }
  .total b { color: #2563eb; } .remaining b { color: #dc2626; }
  pre.mermaid { overflow-x: auto; }
</style>";

/// The page from a diag record and (optionally) the bundle's diff ledger.
pub fn render_html(diag: &Value, ledger: Option<&DiffLedger>) -> Result<String, String> {
    let l = &diag["identifierLedger"];
    if !l.is_object() {
        return Err("no identifierLedger — re-run the pipeline with --diagnostics".into());
    }
    let total = num(&l["totalBindings"]);
    let settled = sorted_counts(&l["transferSettled"]);
    let transfer: i64 = settled.iter().map(|s| s.1).sum();
    let trails: Vec<Value> = diag["strategyTrails"]["trails"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let ledger_rows: String = settled
        .iter()
        .map(|(s, n)| {
            format!(
                "<tr><td>{s}</td><td class=n>{}</td><td class=n>{}</td></tr>",
                fmt_int(*n),
                pct(*n, total)
            )
        })
        .collect();
    let funnel: String = funnel_rows(&trails)
        .iter()
        .map(|(s, c, r)| {
            format!(
                "<tr><td>{s}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td><td class=reasons>{r}</td></tr>",
                fmt_int(c[0]),
                fmt_int(c[1]),
                fmt_int(c[2]),
                fmt_int(c[3])
            )
        })
        .collect();
    let clobbers: Vec<&Value> = trails
        .iter()
        .filter(|t| num(&t["postSettleAttempts"]) > 0)
        .collect();
    let clobber_rows: String = clobbers
        .iter()
        .take(20)
        .map(|c| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td class=n>{}</td></tr>",
                c["oldName"].as_str().unwrap_or_default(),
                c["loc"].as_str().unwrap_or_default(),
                c["settledBy"].as_str().unwrap_or("undefined"),
                num(&c["postSettleAttempts"])
            )
        })
        .collect();
    let llm = num(&l["llmNamed"]);
    let remaining = num(&l["remainingMinted"]);
    let len = |v: &Value| fmt_int(v.as_array().map_or(0, Vec::len) as i64);
    let un = &diag["unrenamed"];
    Ok(format!(
        "<!doctype html>
<meta charset=\"utf-8\">
<title>humanify identifier report</title>
{STYLE}
<h1>Identifier accounting <small style=\"color:#888\">{ts}</small></h1>
<div class=\"strip\">
  <div class=\"total\"><b>{total_s}</b>TOTAL bindings</div>
  <div><b>{transfer_s}</b>transfer-settled ({transfer_p})</div>
  <div><b>{llm_s}</b>LLM-named ({llm_p})</div>
  <div class=\"remaining\"><b>{rem_s}</b>REMAINING minted ({rem_p})</div>
</div>
<h2>Ledger — who settled what</h2>
<table><tr><th>strategy</th><th>settled</th><th>of total</th></tr>
<tr><td><b>TOTAL bindings</b></td><td class=n><b>{total_s}</b></td><td class=n>100%</td></tr>{ledger_rows}
<tr><td><b>transfer subtotal</b></td><td class=n><b>{transfer_s}</b></td><td class=n>{transfer_p}</td></tr>
<tr><td>llm</td><td class=n>{llm_s}</td><td class=n>{llm_p}</td></tr>
<tr><td>llm-unrenamed</td><td class=n>{unren}</td><td class=n></td></tr>
<tr><td><b>REMAINING still-minted</b></td><td class=n><b>{rem_s}</b></td><td class=n>{rem_p}</td></tr></table>
{terminal}<h2>Attempt funnel</h2>
<table><tr><th>strategy</th><th>applied</th><th>rejected</th><th>abstained</th><th>vote-routed</th><th>top refusal reasons</th></tr>{funnel}</table>
{diff}<h2>Post-settle flags ({nclob} bindings)</h2>
<table><tr><th>name</th><th>loc</th><th>settled by</th><th>later attempts</th></tr>{clobber_rows}</table>
<h2>LLM endgame</h2>
<p>renamed {renamed} · unchanged {u1} · missing {u2} · duplicate {u3} · invalid {u4}</p>
",
        ts = diag["timestamp"].as_str().unwrap_or_default(),
        total_s = fmt_int(total),
        transfer_s = fmt_int(transfer),
        transfer_p = pct(transfer, total),
        llm_s = fmt_int(llm),
        llm_p = pct(llm, total),
        rem_s = fmt_int(remaining),
        rem_p = pct(remaining, total),
        unren = fmt_int(num(&l["notRenamed"])),
        terminal = terminal_state(&l["terminalState"], total),
        diff = ledger.map(diff_section).unwrap_or_default(),
        nclob = clobbers.len(),
        renamed = len(&diag["renamed"]),
        u1 = len(&un["unchanged"]),
        u2 = len(&un["missing"]),
        u3 = len(&un["duplicate"]),
        u4 = len(&un["invalid"]),
    ))
}

/// Write one pair's page from its diag record and its two bundles.
pub fn write_report(diag: &Path, out: &Path, inputs: &PairInputs) -> Result<(), String> {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let d: Value =
        serde_json::from_str(&read(diag)?).map_err(|e| format!("{}: {e}", diag.display()))?;
    let meta = |t: &Path, f: &str| t.join(".humanify").join(f);
    let fresh = bundle_statements(
        &read(&meta(&inputs.fresh_tree, "humanified.js"))?,
        "fresh",
        false,
    )?;
    let prior = bundle_statements(
        &read(&meta(&inputs.prior_tree, "humanified.js"))?,
        "prior",
        false,
    )?;
    let fl = meta(&inputs.fresh_tree, "split-ledger.json");
    let pl = meta(&inputs.prior_tree, "split-ledger.json");
    let ledger = diff_ledger(&fresh, &prior, Some((&fl, &pl)));
    std::fs::write(out, render_html(&d, Some(&ledger))?)
        .map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
