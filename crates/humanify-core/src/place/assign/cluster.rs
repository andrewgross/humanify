//! The FRESH-grouping regime — TS `src/split/cluster-assign.ts` (exp029)
//! plus the segment helpers it borrows from stable-split.ts
//! (`referenceIndices`, `segmentStem`, `segmentBindings`): no prior
//! ledger, no fossils. Whole vendored libraries (Bun CJS factories) go to
//! vendor/; the app statements are cut at their reference-graph SEAMS
//! (valleys of an IDF-weighted crossing curve) into a size-balanced nested
//! folder tree under src/, each level named after its dominant binding and
//! LLM-polished when a namer is given (files first, then sub folders, then
//! the top level as ONE joint batch, then the optional holistic reviser).
//!
//! The curve is float arithmetic, so its inputs are reproduced bit for bit:
//! the IDF weights use V8's `Math.log` (fdlibm — [`math_log`]) and every
//! reference set is iterated in the TS's insertion order (the babel
//! `traverseFast` order the references were found in), so the sums add in
//! the same order.

use std::collections::{HashMap, HashSet};

use humanify_model::js::{is_js_whitespace, math_log};
use serde_json::Value;

use super::namer::{FolderSummary, NameKind, NameLevel, SplitNameRequest, SplitNamer, TreeReviser};
use crate::detect::js_text::js_prefix;
use crate::modules::vendor_names::vendor_stem_for;
use crate::place::babel_walk::walk;
use crate::place::declared::declared_names;
use crate::place::layout::{CODE_DIR, VENDOR_DIR};
use crate::place::stems::{accept_proposed_name, is_rejected_stem, to_kebab_case};

/// `ClusterConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterConfig {
    pub window: usize,
    pub min_gap: usize,
    pub target_files: usize,
    pub max_lines: usize,
    pub max_seg: usize,
    pub min_lines: usize,
    pub min_top: usize,
    pub max_top: usize,
    pub min_sub: usize,
    pub max_sub: usize,
    pub flat_top: usize,
    pub folder_window: usize,
}

/// `DEFAULT_CLUSTER_CONFIG`.
pub const DEFAULT_CLUSTER_CONFIG: ClusterConfig = ClusterConfig {
    window: 40,
    min_gap: 4,
    target_files: 1700,
    max_lines: 2500,
    max_seg: 60,
    min_lines: 25,
    min_top: 40,
    max_top: 100,
    min_sub: 6,
    max_sub: 25,
    flat_top: 8,
    folder_window: 300,
};

fn node_type(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

/// `referenceIndices`: per statement, the indices of wrapper-body
/// declarations it references (first-seen order — a JS `Set`). Approximate
/// on purpose (no shadow analysis): every babel `Identifier` node whose
/// name is not the statement's own and is declared elsewhere.
pub fn reference_indices(body: &[Value]) -> Vec<Vec<usize>> {
    let declared: Vec<Vec<String>> = body.iter().map(declared_names).collect();
    let mut decl_index: HashMap<&str, usize> = HashMap::new();
    for (i, names) in declared.iter().enumerate() {
        for n in names {
            decl_index.entry(n.as_str()).or_insert(i);
        }
    }
    body.iter()
        .enumerate()
        .map(|(i, stmt)| {
            let own: HashSet<&str> = declared[i].iter().map(String::as_str).collect();
            let mut refs: Vec<usize> = Vec::new();
            walk(stmt, |v| {
                let Some(name) = v.identifier else { return };
                if own.contains(name) {
                    return;
                }
                if let Some(&idx) = decl_index.get(name)
                    && idx != i
                    && !refs.contains(&idx)
                {
                    refs.push(idx);
                }
            });
            refs
        })
        .collect()
}

/// `inboundCounts`: references INTO [seg_start, seg_end) from outside.
fn inbound_counts(refs: &[Vec<usize>], seg_start: usize, seg_end: usize) -> HashMap<usize, usize> {
    let mut inbound = HashMap::new();
    for (i, list) in refs.iter().enumerate() {
        if i >= seg_start && i < seg_end {
            continue;
        }
        for &r in list {
            if r >= seg_start && r < seg_end {
                *inbound.entry(r).or_insert(0) += 1;
            }
        }
    }
    inbound
}

fn is_fn_class(stmt: &Value) -> bool {
    matches!(node_type(stmt), "FunctionDeclaration" | "ClassDeclaration")
}

/// `betterStem`: prefer function/class stems over var noise for near-tied
/// counts.
fn better_stem(candidate: (usize, bool), best: Option<(usize, bool)>) -> bool {
    let Some((best_count, best_fn)) = best else {
        return true;
    };
    let (count, is_fn) = candidate;
    if is_fn == best_fn {
        return count > best_count;
    }
    if is_fn {
        count * 2 >= best_count
    } else {
        count > best_count * 2
    }
}

/// `segmentStem`: the segment's most externally-referenced
/// non-placeholder binding, "stubs" when every named candidate is banned.
pub fn segment_stem(
    body: &[Value],
    refs: &[Vec<usize>],
    seg_start: usize,
    seg_end: usize,
) -> String {
    let inbound = inbound_counts(refs, seg_start, seg_end);
    let mut best: Option<(usize, usize, bool)> = None;
    let mut any_named = false;
    for (i, stmt) in body.iter().enumerate().take(seg_end).skip(seg_start) {
        let names = declared_names(stmt);
        let Some(first) = names.first() else {
            continue;
        };
        any_named = true;
        if is_rejected_stem(first) {
            continue;
        }
        let candidate = (inbound.get(&i).copied().unwrap_or(0), is_fn_class(stmt));
        if better_stem(candidate, best.map(|(_, c, f)| (c, f))) {
            best = Some((i, candidate.0, candidate.1));
        }
    }
    if let Some((idx, _, _)) = best {
        return declared_names(&body[idx])
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("segment_{seg_start}"));
    }
    if any_named {
        "stubs".to_string()
    } else {
        format!("segment_{seg_start}")
    }
}

/// `segmentBindings`: the top declared bindings, inbound-weighted, for
/// namer prompts ("function handleMessage (12 refs)").
pub fn segment_bindings(
    body: &[Value],
    refs: &[Vec<usize>],
    seg_start: usize,
    seg_end: usize,
    limit: usize,
) -> Vec<String> {
    let inbound = inbound_counts(refs, seg_start, seg_end);
    let mut rows: Vec<(String, &str, usize)> = Vec::new();
    for (i, stmt) in body.iter().enumerate().take(seg_end).skip(seg_start) {
        let Some(name) = declared_names(stmt).into_iter().next() else {
            continue;
        };
        let kind = match node_type(stmt) {
            "FunctionDeclaration" => "function",
            "ClassDeclaration" => "class",
            _ => "var",
        };
        rows.push((name, kind, inbound.get(&i).copied().unwrap_or(0)));
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    rows.into_iter()
        .take(limit)
        .map(|(name, kind, count)| format!("{kind} {name} ({count} refs)"))
        .collect()
}

/// Line terminators babel's `loc` counts (\r\n as one).
fn line_count(text: &str) -> usize {
    let mut lines = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines += 1;
            }
            '\n' | '\u{2028}' | '\u{2029}' => lines += 1,
            _ => {}
        }
    }
    lines
}

/// `RefGraph`.
struct RefGraph {
    refs: Vec<Vec<usize>>,
    idf: Vec<f64>,
    lines: Vec<usize>,
    n: usize,
}

fn build_ref_graph(body: &[Value], texts: &[&str]) -> RefGraph {
    let refs = reference_indices(body);
    let n = body.len();
    let mut indeg = vec![0usize; n];
    for list in &refs {
        for &j in list {
            indeg[j] += 1;
        }
    }
    let idf = indeg
        .iter()
        .map(|&d| math_log(n as f64 / (1 + d) as f64))
        .collect();
    RefGraph {
        refs,
        idf,
        lines: texts.iter().map(|t| line_count(t)).collect(),
        n,
    }
}

/// `crossingCurve`: x[c] = IDF-weighted count of short edges spanning c.
fn crossing_curve(g: &RefGraph, window: usize) -> Vec<f64> {
    let mut diff = vec![0.0f64; g.n + 2];
    for (i, list) in g.refs.iter().enumerate() {
        for &j in list {
            let (a, b) = if i < j { (i, j) } else { (j, i) };
            if b - a > window {
                continue;
            }
            diff[a + 1] += g.idf[j];
            diff[b + 1] -= g.idf[j];
        }
    }
    let mut acc = 0.0;
    (0..=g.n)
        .map(|c| {
            acc += diff[c];
            acc
        })
        .collect()
}

/// `deepSeamCuts`: the globally deepest seams (at least `minGap` apart),
/// then the budget caps.
fn deep_seam_cuts(g: &RefGraph, x: &[f64], cfg: &ClusterConfig) -> Vec<usize> {
    let mut cand: Vec<usize> = (1..g.n).collect();
    cand.sort_by(|&a, &b| {
        x[a].partial_cmp(&x[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut taken: HashSet<usize> = HashSet::new();
    let mut accepted: Vec<usize> = Vec::new();
    for c in cand {
        if accepted.len() + 1 >= cfg.target_files {
            break;
        }
        let clear = (1..cfg.min_gap)
            .all(|d| !taken.contains(&(c.wrapping_sub(d))) && !taken.contains(&(c + d)));
        if clear {
            accepted.push(c);
            taken.insert(c);
        }
    }
    accepted.sort_unstable();
    enforce_budgets(g, x, &accepted, cfg)
}

/// `deepestIn`: the lowest x in [lo, hi], leftmost on ties.
fn deepest_in(x: &[f64], lo: usize, hi: usize) -> usize {
    let mut best = hi;
    let mut best_v = f64::INFINITY;
    for (c, &v) in x.iter().enumerate().take(hi + 1).skip(lo) {
        if v < best_v {
            best_v = v;
            best = c;
        }
    }
    best
}

/// `budgetReach`.
fn budget_reach(g: &RefGraph, start: usize, seg_end: usize, cfg: &ClusterConfig) -> usize {
    let mut end = start + 1;
    let mut acc = g.lines[start];
    while end < seg_end && end - start < cfg.max_seg && acc + g.lines[end] <= cfg.max_lines {
        acc += g.lines[end];
        end += 1;
    }
    end
}

/// `budgetSplit`.
fn budget_split(
    g: &RefGraph,
    x: &[f64],
    seg: (usize, usize),
    cfg: &ClusterConfig,
    out: &mut HashSet<usize>,
) {
    let (mut start, seg_end) = seg;
    while start < seg_end {
        let end = budget_reach(g, start, seg_end, cfg);
        if end >= seg_end {
            break;
        }
        let cut = deepest_in(x, (start + 1).min(end), end);
        out.insert(cut);
        start = cut;
    }
}

fn enforce_budgets(g: &RefGraph, x: &[f64], accepted: &[usize], cfg: &ClusterConfig) -> Vec<usize> {
    let mut bounds = vec![0];
    bounds.extend_from_slice(accepted);
    bounds.push(g.n);
    let mut final_cuts: HashSet<usize> = accepted.iter().copied().collect();
    for w in bounds.windows(2) {
        budget_split(g, x, (w[0], w[1]), cfg, &mut final_cuts);
    }
    let mut cuts: Vec<usize> = final_cuts.into_iter().collect();
    cuts.sort_unstable();
    drop_tiny_segments(g, &cuts, cfg)
}

/// `dropTinySegments`: merge segments under the minLines floor into a
/// neighbour unless a budget cap forbids it.
fn drop_tiny_segments(g: &RefGraph, cuts: &[usize], cfg: &ClusterConfig) -> Vec<usize> {
    let mut pre = vec![0usize; g.n + 1];
    for i in 0..g.n {
        pre[i + 1] = pre[i] + g.lines[i];
    }
    let mut kept: Vec<usize> = Vec::new();
    let mut start = 0;
    for (i, &c) in cuts.iter().enumerate() {
        let next = cuts.get(i + 1).copied().unwrap_or(g.n);
        let merged_too_big = pre[next] - pre[start] > cfg.max_lines || next - start > cfg.max_seg;
        if pre[c] - pre[start] >= cfg.min_lines || merged_too_big {
            kept.push(c);
            start = c;
        }
    }
    if let Some(&last) = kept.last() {
        let prev_start = if kept.len() > 1 {
            kept[kept.len() - 2]
        } else {
            0
        };
        let tail_tiny = pre[g.n] - pre[last] < cfg.min_lines;
        let fits = pre[g.n] - pre[prev_start] <= cfg.max_lines && g.n - prev_start <= cfg.max_seg;
        if tail_tiny && fits {
            kept.pop();
        }
    }
    kept
}

/// `pickWalls`: group sorted cuts into runs of [min, max] cuts, walling at
/// the deepest seam within each allowed window.
pub fn pick_walls(cuts: &[usize], x: &[f64], min: usize, max: usize) -> HashSet<usize> {
    let max = max.max(1);
    let min = min.min(max).max(1);
    let mut walls = HashSet::new();
    let mut start = 0;
    while cuts.len() - start > max {
        let lo = start + min;
        let mut hi = (start + max).min(cuts.len() - 1);
        if cuts.len() >= min {
            let keeping_tail = cuts.len() - min;
            if keeping_tail >= lo {
                hi = hi.min(keeping_tail);
            }
        }
        let mut best = hi;
        let mut best_d = f64::INFINITY;
        for k in lo..=hi {
            if x[cuts[k]] < best_d {
                best_d = x[cuts[k]];
                best = k;
            }
        }
        walls.insert(cuts[best]);
        start = best;
    }
    walls
}

/// `Segment`.
#[derive(Clone, Copy, Debug)]
struct Segment {
    s: usize,
    e: usize,
    top: usize,
    sub: usize,
}

/// `subWallsWithin`.
fn sub_walls_within(
    cuts: &[usize],
    top_walls: &HashSet<usize>,
    x: &[f64],
    min: usize,
    max: usize,
) -> HashSet<usize> {
    let mut sub_walls = HashSet::new();
    let mut group: Vec<usize> = Vec::new();
    for &c in cuts {
        if top_walls.contains(&c) {
            sub_walls.extend(pick_walls(&group, x, min, max));
            group.clear();
        } else {
            group.push(c);
        }
    }
    sub_walls.extend(pick_walls(&group, x, min, max));
    sub_walls
}

/// `groupSegments`.
fn group_segments(
    cuts: &[usize],
    x_folder: &[f64],
    app_n: usize,
    cfg: &ClusterConfig,
) -> Vec<Segment> {
    let top_walls = pick_walls(cuts, x_folder, cfg.min_top, cfg.max_top);
    let sub_walls = sub_walls_within(cuts, &top_walls, x_folder, cfg.min_sub, cfg.max_sub);
    let mut bounds = vec![0];
    bounds.extend_from_slice(cuts);
    bounds.push(app_n);
    let (mut top, mut sub) = (0, 0);
    let mut segments = Vec::new();
    for i in 1..bounds.len() {
        let s = bounds[i - 1];
        if i > 1 {
            if top_walls.contains(&s) {
                top += 1;
                sub = 0;
            } else if sub_walls.contains(&s) {
                sub += 1;
            }
        }
        segments.push(Segment {
            s,
            e: bounds[i],
            top,
            sub,
        });
    }
    segments
}

/// `Named`: an item to name — a span + the dedup scope it competes in.
#[derive(Clone, Debug)]
struct Named {
    key: String,
    s: usize,
    e: usize,
    scope: String,
}

/// `mergedFolderNames`: same-name same-scope groups merge (first casing
/// wins).
fn merged_folder_names(
    items: &[Named],
    polished: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut canonical: HashMap<String, String> = HashMap::new();
    items
        .iter()
        .map(|it| {
            let name = polished
                .get(&it.key)
                .cloned()
                .unwrap_or_else(|| "module".into());
            let key = format!("{}|{}", it.scope, name.to_lowercase());
            let chosen = canonical.entry(key).or_insert_with(|| name.clone()).clone();
            (it.key.clone(), chosen)
        })
        .collect()
}

/// `tokensOf`: camelCase/kebab/snake stem → lowercase word tokens
/// (`.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase()
/// .split(/[\s_-]+/).filter(Boolean)`).
fn stem_tokens(stem: &str) -> Vec<String> {
    spaced_lower_upper(stem)
        .to_lowercase()
        .split(|c: char| is_js_whitespace(c) || c == '_' || c == '-')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// `.replace(/([a-z0-9])([A-Z])/g, "$1 $2")`.
fn spaced_lower_upper(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let pair = (c.is_ascii_lowercase() || c.is_ascii_digit())
            && chars.get(i + 1).is_some_and(char::is_ascii_uppercase);
        out.push(c);
        if pair {
            out.push(' ');
            out.push(chars[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// `singular`: crude plural normalization, for token COMPARISON only.
fn singular(token: &str) -> &str {
    if token.encode_utf16().count() > 3 {
        token.strip_suffix('s').unwrap_or(token)
    } else {
        token
    }
}

/// `mergeSubIntoTop`: `None` when the sub adds nothing over its parent;
/// else the sub's final name (its residual tokens, or itself when
/// disjoint).
pub fn merge_sub_into_top(top: &str, sub: &str) -> Option<String> {
    let top_tokens: HashSet<String> = stem_tokens(top)
        .iter()
        .map(|t| singular(t).to_string())
        .collect();
    let sub_tokens = stem_tokens(sub);
    let residual: Vec<&String> = sub_tokens
        .iter()
        .filter(|t| !top_tokens.contains(singular(t)))
        .collect();
    if residual.is_empty() {
        return None;
    }
    if residual.len() == sub_tokens.len() {
        return Some(sub.to_string());
    }
    let name: String = residual
        .iter()
        .enumerate()
        .map(|(i, tok)| {
            if i == 0 {
                (*tok).clone()
            } else {
                let mut cs = tok.chars();
                cs.next()
                    .map(|c| c.to_uppercase().collect::<String>() + cs.as_str())
                    .unwrap_or_default()
            }
        })
        .collect();
    accept_proposed_name(&name)
}

const FOLDER_DECORATION: [&str; 11] = [
    "group",
    "suite",
    "engine",
    "manager",
    "hub",
    "handler",
    "factory",
    "processor",
    "service",
    "module",
    "wrapper",
];

const FOLDER_VERB: [&str; 29] = [
    "get",
    "set",
    "build",
    "filter",
    "handle",
    "create",
    "make",
    "render",
    "process",
    "register",
    "add",
    "remove",
    "update",
    "fetch",
    "load",
    "parse",
    "init",
    "initialize",
    "run",
    "send",
    "apply",
    "resolve",
    "compute",
    "generate",
    "validate",
    "check",
    "format",
    "convert",
    "transform",
];

/// `cleanFolderSegment`.
fn clean_folder_segment(name: &str) -> String {
    let mut toks = stem_tokens(name);
    while toks.len() > 1 && FOLDER_DECORATION.contains(&toks[toks.len() - 1].as_str()) {
        toks.pop();
    }
    if toks.len() > 1 && FOLDER_VERB.contains(&toks[0].as_str()) {
        toks.remove(0);
    }
    let joined = toks.join("-");
    if joined.is_empty() {
        to_kebab_case(name)
    } else {
        joined
    }
}

/// `cleanDirPath`.
fn clean_dir_path(dir: &str) -> String {
    dir.split('/')
        .filter(|s| !s.is_empty())
        .map(clean_folder_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// The distinct group-1 captures of a scanner, capped.
fn distinct_capped(found: Vec<String>, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in found {
        if v.is_empty() || out.contains(&v) {
            continue;
        }
        out.push(v);
        if out.len() >= cap {
            break;
        }
    }
    out
}

/// `/["'`]([^"'`\n\\]{4,50})["'`]/g` captures, in order.
fn quoted_strings(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let is_quote = |c: char| matches!(c, '"' | '\'' | '`');
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_quote(chars[i]) {
            let mut j = i + 1;
            let mut units = 0;
            while j < chars.len() && !is_quote(chars[j]) && chars[j] != '\n' && chars[j] != '\\' {
                units += chars[j].len_utf16();
                j += 1;
            }
            if j < chars.len() && is_quote(chars[j]) && (4..=50).contains(&units) {
                out.push(chars[i + 1..j].iter().collect());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `/\b([A-Za-z_$][\w$]*\.[A-Za-z_$][\w$]*)\(/g` captures, in order (`\b`
/// is ASCII-\w based; `$` is not a word char).
fn member_calls(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let ident_start = |c: u8| c.is_ascii_alphabetic() || c == b'_' || c == b'$';
    let ident = |c: u8| word(c) || c == b'$';
    let mut out = Vec::new();
    let mut p = 0;
    while p < b.len() {
        let prev_word = p > 0 && word(b[p - 1]);
        let boundary = prev_word != word(b[p]);
        if !(boundary && ident_start(b[p])) {
            p += 1;
            continue;
        }
        let mut q = p + 1;
        while q < b.len() && ident(b[q]) {
            q += 1;
        }
        let matched = (q + 1 < b.len() && b[q] == b'.' && ident_start(b[q + 1])).then(|| {
            let mut r = q + 2;
            while r < b.len() && ident(b[r]) {
                r += 1;
            }
            r
        });
        match matched {
            Some(r) if r < b.len() && b[r] == b'(' => {
                out.push(src[p..r].to_string());
                p = r + 1;
            }
            _ => p += 1,
        }
    }
    out
}

/// `buildSegmentEvidence`: distinctive string literals and member-call
/// targets of the segment's source, for the namer.
fn build_segment_evidence(code: &str, spans: &[(u32, u32)], s: usize, e: usize) -> String {
    let (Some(&(start, _)), Some(&(_, end))) =
        (spans.get(s), e.checked_sub(1).and_then(|l| spans.get(l)))
    else {
        return String::new();
    };
    let src = &code[start as usize..end as usize];
    let numeric_like = |s: &str| {
        !s.is_empty()
            && s.chars().all(|c| {
                c.is_ascii_digit() || is_js_whitespace(c) || matches!(c, '.' | '-' | '_' | '/')
            })
    };
    let strings: Vec<String> = distinct_capped(quoted_strings(src), 8)
        .into_iter()
        .filter(|s| s.chars().any(|c| c.is_ascii_alphabetic()) && !numeric_like(s))
        .collect();
    let calls: Vec<String> = distinct_capped(member_calls(src), 8)
        .into_iter()
        .filter(|c| {
            !["this.", "exports.", "module."]
                .iter()
                .any(|p| c.starts_with(p))
        })
        .collect();
    let mut parts: Vec<String> = Vec::new();
    if !strings.is_empty() {
        let quoted: Vec<String> = strings.iter().map(|s| format!("\"{s}\"")).collect();
        parts.push(format!("strings: {}", quoted.join(", ")));
    }
    if !calls.is_empty() {
        parts.push(format!("calls: {}", calls.join(", ")));
    }
    // `.slice(0, 500)` in UTF-16 units (a cut through a surrogate pair —
    // a lone surrogate in the TS — drops the half char here).
    js_prefix(&parts.join("; "), 500).to_string()
}

/// `acceptForItem`: a validated proposal that does not merely echo one of
/// the folder's members.
fn accept_for_item(proposal: Option<&str>, members: Option<&[String]>) -> Option<String> {
    let accepted = accept_proposed_name(proposal?)?;
    let lower = accepted.to_lowercase();
    let echo = members.is_some_and(|m| m.iter().any(|x| x.to_lowercase() == lower));
    (!echo).then_some(accepted)
}

/// The app side a naming pass reads.
struct App<'a> {
    body: &'a [Value],
    refs: &'a [Vec<usize>],
    /// For evidence: the shipped text and the app statements' spans.
    code: Option<(&'a str, &'a [(u32, u32)])>,
}

/// `PolishOpts`.
struct Polish<'a> {
    kind: NameKind,
    level: Option<NameLevel>,
    evidence: bool,
    members_of: &'a dyn Fn(&str) -> Option<Vec<String>>,
}

/// `polishLevel`: one namer call per sibling scope (groups in first-seen
/// order); the mechanical stem stands wherever the namer declines.
fn polish_level(
    items: &[Named],
    mech: &HashMap<String, String>,
    app: &App,
    opts: &Polish,
    namer: Option<&mut dyn SplitNamer>,
) -> HashMap<String, String> {
    let mech_of = |k: &str| mech.get(k).cloned().unwrap_or_else(|| "module".into());
    let mut polished: HashMap<String, String> = items
        .iter()
        .map(|it| (it.key.clone(), mech_of(&it.key)))
        .collect();
    let Some(namer) = namer else {
        return polished;
    };
    let mut groups: Vec<(&str, Vec<&Named>)> = Vec::new();
    for it in items {
        match groups.iter_mut().find(|(s, _)| *s == it.scope) {
            Some((_, list)) => list.push(it),
            None => groups.push((&it.scope, vec![it])),
        }
    }
    for (_, group) in groups {
        let requests: Vec<SplitNameRequest> = group
            .iter()
            .map(|it| SplitNameRequest {
                kind: opts.kind,
                mechanical_stem: mech_of(&it.key),
                siblings: group
                    .iter()
                    .filter(|o| o.key != it.key)
                    .map(|o| mech.get(&o.key).cloned().unwrap_or_default())
                    .collect(),
                bindings: segment_bindings(app.body, app.refs, it.s, it.e, 10),
                members: (opts.members_of)(&it.key),
                level: opts.level,
                evidence: app
                    .code
                    .filter(|_| opts.evidence)
                    .map(|(code, spans)| build_segment_evidence(code, spans, it.s, it.e)),
            })
            .collect();
        let proposals = namer.name(&requests);
        for (i, it) in group.iter().enumerate() {
            let proposal = proposals.get(i).cloned().flatten();
            if let Some(accepted) =
                accept_for_item(proposal.as_deref(), requests[i].members.as_deref())
            {
                polished.insert(it.key.clone(), accepted);
            }
        }
    }
    polished
}

/// `collectMemberFiles`: distinct polished file names (capped at 12) of
/// the segments `belongs` selects.
fn collect_member_files(
    segments: &[Segment],
    file_polished: &HashMap<String, String>,
    belongs: impl Fn(&Segment) -> bool,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (idx, seg) in segments.iter().enumerate() {
        if out.len() >= 12 {
            break;
        }
        if !belongs(seg) {
            continue;
        }
        let name = file_polished
            .get(&idx.to_string())
            .cloned()
            .unwrap_or_default();
        if name.is_empty() || !seen.insert(name.to_lowercase()) {
            continue;
        }
        out.push(name);
    }
    out
}

/// `hoistSingletonDirs`: a dir holding one file hoists it a level per
/// round (two rounds).
fn hoist_singleton_dirs(dirs: &mut [String]) {
    for _ in 0..2 {
        let mut per_dir: HashMap<String, usize> = HashMap::new();
        for d in dirs.iter() {
            *per_dir.entry(d.clone()).or_default() += 1;
        }
        for d in dirs.iter_mut() {
            if !d.is_empty() && per_dir.get(d.as_str()) == Some(&1) {
                *d = match d.rfind('/') {
                    Some(cut) => d[..cut].to_string(),
                    None => String::new(),
                };
            }
        }
    }
}

/// The per-top structure (`topGroupInfo`) and the keep-sub rule.
struct TopInfo {
    files: HashMap<usize, usize>,
    subs: HashMap<usize, HashSet<usize>>,
    flat_top: usize,
}

impl TopInfo {
    fn of(segments: &[Segment], flat_top: usize) -> Self {
        let mut files: HashMap<usize, usize> = HashMap::new();
        let mut subs: HashMap<usize, HashSet<usize>> = HashMap::new();
        for seg in segments {
            *files.entry(seg.top).or_default() += 1;
            subs.entry(seg.top).or_default().insert(seg.sub);
        }
        TopInfo {
            files,
            subs,
            flat_top,
        }
    }

    /// The sub level survives only where it adds structure.
    fn keep_sub(&self, top: usize) -> bool {
        self.files.get(&top).copied().unwrap_or(0) > self.flat_top
            && self.subs.get(&top).map_or(0, HashSet::len) > 1
    }
}

/// `widen` over a first-seen-ordered span map.
fn widen(map: &mut Vec<(String, (usize, usize))>, key: String, s: usize, e: usize) {
    match map.iter_mut().find(|(k, _)| *k == key) {
        Some((_, span)) => *span = (span.0.min(s), span.1.max(e)),
        None => map.push((key, (s, e))),
    }
}

/// The optional LLM hooks of a naming pass.
pub struct ClusterNamers<'a> {
    pub namer: Option<&'a mut dyn SplitNamer>,
    pub reviser: Option<&'a mut dyn TreeReviser>,
}

/// `reviseTopNames` (Tier 4): mutates `top_names` (key → name).
fn revise_top_names(
    top_names: &mut [(String, String)],
    segments: &[Segment],
    file_polished: &HashMap<String, String>,
    reviser: &mut dyn TreeReviser,
) {
    let mut keys_by_name: Vec<(String, Vec<String>)> = Vec::new();
    for (key, name) in top_names.iter() {
        match keys_by_name.iter_mut().find(|(n, _)| n == name) {
            Some((_, keys)) => keys.push(key.clone()),
            None => keys_by_name.push((name.clone(), vec![key.clone()])),
        }
    }
    let summaries: Vec<FolderSummary> = keys_by_name
        .iter()
        .map(|(name, keys)| FolderSummary {
            name: name.clone(),
            members: collect_member_files(segments, file_polished, |seg| {
                keys.contains(&seg.top.to_string())
            }),
        })
        .collect();
    for (old, proposed) in reviser.revise(&summaries) {
        let Some(accepted) = accept_proposed_name(&proposed) else {
            continue;
        };
        let keys = keys_by_name
            .iter()
            .find(|(n, _)| *n == old)
            .map(|(_, k)| k.clone())
            .unwrap_or_default();
        for (key, name) in top_names.iter_mut() {
            if keys.contains(key) {
                *name = accepted.clone();
            }
        }
    }
}

/// `nameSegments`: every app segment's `[<folder>[/<sub>]/]<file>.js`.
fn name_segments(
    segments: &[Segment],
    app: &App,
    cfg: &ClusterConfig,
    namers: ClusterNamers,
) -> Vec<String> {
    let ClusterNamers { mut namer, reviser } = namers;
    let mut top_span: Vec<(String, (usize, usize))> = Vec::new();
    let mut sub_span: Vec<(String, (usize, usize))> = Vec::new();
    for seg in segments {
        widen(&mut top_span, seg.top.to_string(), seg.s, seg.e);
        widen(
            &mut sub_span,
            format!("{}/{}", seg.top, seg.sub),
            seg.s,
            seg.e,
        );
    }
    let info = TopInfo::of(segments, cfg.flat_top);
    let top_of = |key: &str| {
        key.split('/')
            .next()
            .unwrap_or("")
            .parse::<usize>()
            .unwrap_or(usize::MAX)
    };
    let mech_of = |items: &[Named]| -> HashMap<String, String> {
        items
            .iter()
            .map(|it| (it.key.clone(), segment_stem(app.body, app.refs, it.s, it.e)))
            .collect()
    };
    // Files first.
    let file_items: Vec<Named> = segments
        .iter()
        .enumerate()
        .map(|(idx, seg)| Named {
            key: idx.to_string(),
            s: seg.s,
            e: seg.e,
            scope: if info.keep_sub(seg.top) {
                format!("{}/{}", seg.top, seg.sub)
            } else {
                seg.top.to_string()
            },
        })
        .collect();
    let no_members = |_: &str| None;
    let file_polished = polish_level(
        &file_items,
        &mech_of(&file_items),
        app,
        &Polish {
            kind: NameKind::File,
            level: None,
            evidence: true,
            members_of: &no_members,
        },
        namer.as_mut().map(|n| &mut **n as &mut dyn SplitNamer),
    );
    // Then sub folders.
    let mut subs_sorted = sub_span.clone();
    subs_sorted.sort_by_key(|(_, (s, _))| *s);
    let sub_items: Vec<Named> = subs_sorted
        .into_iter()
        .filter(|(k, _)| info.keep_sub(top_of(k)))
        .map(|(k, (s, e))| Named {
            scope: k.split('/').next().unwrap_or("").to_string(),
            key: k,
            s,
            e,
        })
        .collect();
    let sub_members = |key: &str| {
        Some(collect_member_files(segments, &file_polished, |seg| {
            format!("{}/{}", seg.top, seg.sub) == key
        }))
    };
    let sub_polished = polish_level(
        &sub_items,
        &mech_of(&sub_items),
        app,
        &Polish {
            kind: NameKind::Folder,
            level: Some(NameLevel::Sub),
            evidence: false,
            members_of: &sub_members,
        },
        namer.as_mut().map(|n| &mut **n as &mut dyn SplitNamer),
    );
    let sub_names = merged_folder_names(&sub_items, &sub_polished);
    // Then the top level, one joint batch.
    let mut tops_sorted = top_span.clone();
    tops_sorted.sort_by_key(|(_, (s, _))| *s);
    let top_items: Vec<Named> = tops_sorted
        .into_iter()
        .map(|(k, (s, e))| Named {
            key: k,
            s,
            e,
            scope: String::new(),
        })
        .collect();
    let top_members = |key: &str| {
        let top = top_of(key);
        if !info.keep_sub(top) {
            return Some(collect_member_files(segments, &file_polished, |seg| {
                seg.top == top
            }));
        }
        let mut under: Vec<String> = Vec::new();
        for (sub_key, name) in &sub_names {
            if top_of(sub_key) == top && !under.contains(name) {
                under.push(name.clone());
            }
        }
        under.truncate(12);
        Some(under)
    };
    let top_polished = polish_level(
        &top_items,
        &mech_of(&top_items),
        app,
        &Polish {
            kind: NameKind::Folder,
            level: Some(NameLevel::Top),
            evidence: false,
            members_of: &top_members,
        },
        namer.as_mut().map(|n| &mut **n as &mut dyn SplitNamer),
    );
    let mut top_names = merged_folder_names(&top_items, &top_polished);
    if let Some(reviser) = reviser {
        revise_top_names(&mut top_names, segments, &file_polished, reviser);
    }
    let lookup = |pairs: &[(String, String)], key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "module".into())
    };
    let mut dirs: Vec<String> = segments
        .iter()
        .map(|seg| {
            let top = lookup(&top_names, &seg.top.to_string());
            if !info.keep_sub(seg.top) {
                return top;
            }
            let sub = lookup(&sub_names, &format!("{}/{}", seg.top, seg.sub));
            match merge_sub_into_top(&top, &sub) {
                None => top,
                Some(sub_final) => format!("{top}/{sub_final}"),
            }
        })
        .collect();
    hoist_singleton_dirs(&mut dirs);
    let kebab_dirs: Vec<String> = dirs.iter().map(|d| clean_dir_path(d)).collect();
    let mut used_by_dir: HashMap<String, HashSet<String>> = HashMap::new();
    (0..segments.len())
        .map(|idx| {
            let used = used_by_dir.entry(kebab_dirs[idx].clone()).or_default();
            let stem = to_kebab_case(
                file_polished
                    .get(&idx.to_string())
                    .map_or("file", String::as_str),
            );
            let file = format!(
                "{}.js",
                unique_case_insensitive_name_ext(&stem, used, ".js")
            );
            if kebab_dirs[idx].is_empty() {
                file
            } else {
                format!("{}/{file}", kebab_dirs[idx])
            }
        })
        .collect()
}

/// `uniqueCaseInsensitiveName(stem, used, ext)` without the extension in
/// the returned stem (the caller appends it).
fn unique_case_insensitive_name_ext(stem: &str, used: &mut HashSet<String>, ext: &str) -> String {
    let mut name = stem.to_string();
    let mut k = 2;
    while used.contains(&format!("{name}{ext}").to_lowercase()) {
        name = format!("{stem}-{k}");
        k += 1;
    }
    used.insert(format!("{name}{ext}").to_lowercase());
    name
}

/// `factoryCallOf`: the `X = CALLEE(fn, ...)` declarator shape.
fn factory_call_of(decl: &Value) -> Option<(String, String, usize)> {
    fn unparen(mut v: &Value) -> &Value {
        while node_type(v) == "ParenthesizedExpression" {
            v = &v["expression"];
        }
        v
    }
    let binding = decl.get("id").filter(|id| node_type(id) == "Identifier")?["name"].as_str()?;
    let init = unparen(decl.get("init").filter(|v| !v.is_null())?);
    if node_type(init) != "CallExpression" {
        return None;
    }
    let callee = unparen(init.get("callee")?);
    if node_type(callee) != "Identifier" {
        return None;
    }
    let arg0 = unparen(init.get("arguments")?.as_array()?.first()?);
    if !matches!(
        node_type(arg0),
        "ArrowFunctionExpression" | "FunctionExpression"
    ) {
        return None;
    }
    Some((
        binding.to_string(),
        callee["name"].as_str()?.to_string(),
        arg0.get("params")?.as_array()?.len(),
    ))
}

/// `factoryCallee`: a statement that is PURELY factory declarations with
/// one shared callee → (first binding, callee, declarator count).
pub fn factory_callee(stmt: &Value) -> Option<(String, String, usize)> {
    if node_type(stmt) != "VariableDeclaration" {
        return None;
    }
    let decls = stmt.get("declarations")?.as_array()?;
    if decls.is_empty() {
        return None;
    }
    let mut calls = Vec::new();
    for d in decls {
        let call = factory_call_of(d)?;
        if call.2 < 1 {
            return None;
        }
        calls.push(call);
    }
    let callee = calls[0].1.clone();
    if calls.iter().any(|c| c.1 != callee) {
        return None;
    }
    Some((calls[0].0.clone(), callee, calls.len()))
}

/// `detectCjsHelper`: the identifier wrapping the most modules (>= 2).
pub fn detect_cjs_helper(body: &[Value]) -> Option<String> {
    let mut tally: Vec<(String, usize)> = Vec::new();
    for stmt in body {
        if let Some((_, callee, count)) = factory_callee(stmt) {
            match tally.iter_mut().find(|(c, _)| *c == callee) {
                Some((_, n)) => *n += count,
                None => tally.push((callee, count)),
            }
        }
    }
    let mut best: Option<String> = None;
    let mut best_n = 1;
    for (name, n) in tally {
        if n > best_n {
            best_n = n;
            best = Some(name);
        }
    }
    best
}

/// `assignClustered`: the per-statement file assignment of the fresh
/// grouping. `code` + `spans` are the rendered text the statements were
/// parsed from (vendor stems floor to a content hash; namer evidence).
pub fn assign_clustered(
    body: &[Value],
    code: Option<(&str, &[(u32, u32)])>,
    cfg: &ClusterConfig,
    namers: ClusterNamers,
) -> Vec<String> {
    let helper = detect_cjs_helper(body);
    let mut used_lib: HashSet<String> = HashSet::new();
    let mut assignment = vec![String::new(); body.len()];
    let mut app_idx: Vec<usize> = Vec::new();
    for (i, stmt) in body.iter().enumerate() {
        let fc = helper.as_ref().and_then(|_| factory_callee(stmt));
        match fc {
            Some((binding, callee, _)) if Some(&callee) == helper.as_ref() => {
                let stem = match code {
                    Some((text, spans)) => {
                        let (s, e) = spans[i];
                        vendor_stem_for(&binding, &text[s as usize..e as usize])
                    }
                    None => binding,
                };
                assignment[i] = format!(
                    "{VENDOR_DIR}/{}.js",
                    unique_case_insensitive_name_ext(&stem, &mut used_lib, ".js")
                );
            }
            _ => app_idx.push(i),
        }
    }
    if app_idx.is_empty() {
        return assignment;
    }
    let app_body: Vec<Value> = app_idx.iter().map(|&i| body[i].clone()).collect();
    let app_spans: Vec<(u32, u32)> = code.map_or_else(Vec::new, |(_, spans)| {
        app_idx.iter().map(|&i| spans[i]).collect()
    });
    let texts: Vec<&str> = match code {
        Some((text, spans)) => app_idx
            .iter()
            .map(|&i| &text[spans[i].0 as usize..spans[i].1 as usize])
            .collect(),
        None => vec![""; app_idx.len()],
    };
    let g = build_ref_graph(&app_body, &texts);
    let x = crossing_curve(&g, cfg.window);
    let x_folder = crossing_curve(&g, cfg.folder_window);
    let segments = group_segments(&deep_seam_cuts(&g, &x, cfg), &x_folder, g.n, cfg);
    let app = App {
        body: &app_body,
        refs: &g.refs,
        code: code.map(|(text, _)| (text, app_spans.as_slice())),
    };
    let paths = name_segments(&segments, &app, cfg, namers);
    for (idx, seg) in segments.iter().enumerate() {
        for a in seg.s..seg.e {
            assignment[app_idx[a]] = format!("{CODE_DIR}/{}", paths[idx]);
        }
    }
    assignment
}
