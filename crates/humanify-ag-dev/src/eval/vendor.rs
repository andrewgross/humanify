//! Vendor churn, decomposed into real dependency change vs reducible noise
//! (TS: `046/vendor-churn.ts` + `046/relocation-pairing.ts`). THE UNIT IS
//! THE FILE: a vendored library is a few lines of minified text, so every
//! bucket counts files, with `diff` line counts alongside.
//!
//! A file's CONTENT SIGNATURE is its whole program under the pipeline's
//! byte-exact identity form (`hash::forms::IDENTITY_KEY`: bindings as slot
//! ordinals; literals, property keys, operators and free names verbatim),
//! with intra-tree `require("./...")` paths masked — humanify's own layout,
//! not library content. Two files with one signature differ only in local
//! names and in which humanify-chosen path they import. Matching is by
//! CONTENT, never by path; a file that lost its path AND changed a little
//! is paired to its predecessor by content similarity (mutual best,
//! thresholds and margin, contenders) and charged its own diff, the rest
//! of the old whole-file charge being the path draw (noise).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use humanify_ag_core::hash::forms::IDENTITY_KEY;
use humanify_ag_core::hash::serialize::{SymbolTables, canonical_tokens};
use humanify_ag_core::ingest::{Ingest, program_estree_json};
use humanify_ag_model::js::is_js_whitespace;
use oxc_allocator::Allocator;
use serde_json::Value;

use super::difftool::Differ;
use super::text::list_js;

/// Sidecar metadata file inside vendor/, scored separately.
pub const BUN_MANIFEST: &str = "_bun-modules.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bucket {
    pub files: usize,
    pub lines: usize,
}

impl Bucket {
    fn bump(&mut self, lines: usize) {
        self.files += 1;
        self.lines += lines;
    }
}

/// Which bucket a charged file landed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorBucket {
    Identical,
    NameOnly,
    FreeReroll,
    MovedPath,
    RealChange,
    TrulyAdded,
    TrulyRemoved,
    Relocated,
    Unparsed,
}

impl VendorBucket {
    /// Charged as REAL dependency change.
    pub fn is_real(self) -> bool {
        matches!(
            self,
            VendorBucket::RealChange | VendorBucket::TrulyAdded | VendorBucket::TrulyRemoved
        )
    }
}

/// One charged file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorFileRow {
    pub bucket: VendorBucket,
    pub prior: Option<String>,
    pub fresh: Option<String>,
    pub lines: usize,
}

/// One relocation pair.
#[derive(Clone, Debug, PartialEq)]
pub struct RelocatedPair {
    pub prior: String,
    pub fresh: String,
    pub score: f64,
    pub runner_up: f64,
    pub real_lines: usize,
    pub draw_lines: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VendorChurn {
    pub identical: Bucket,
    pub name_only: Bucket,
    pub free_reroll: Bucket,
    pub moved_path: Bucket,
    pub real_change: Bucket,
    pub truly_added: Bucket,
    pub truly_removed: Bucket,
    pub relocated: Bucket,
    pub unparsed: Bucket,
    pub manifest_changed_lines: usize,
    pub total_lines: usize,
    pub noise_lines: usize,
    pub real_lines: usize,
    pub relocated_pairs: Vec<RelocatedPair>,
    pub file_rows: Vec<VendorFileRow>,
}

/// One file's content analysis.
struct FileAnalysis {
    sig: String,
    declared: HashSet<String>,
    tokens: Vec<String>,
}

/// Mask every intra-tree `require("./...")` / `require("../...")` path.
fn mask_intra_tree_requires(v: &mut Value) {
    let mut stack: Vec<&mut Value> = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(map) => {
                let is_require = map.get("type").and_then(Value::as_str) == Some("CallExpression")
                    && map
                        .get("callee")
                        .is_some_and(|c| c["type"] == "Identifier" && c["name"] == "require");
                if is_require
                    && let Some(Value::Array(args)) = map.get_mut("arguments")
                    && let Some(Value::Object(arg)) = args.first_mut()
                    && arg.get("type").and_then(Value::as_str) == Some("Literal")
                    && arg
                        .get("value")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.starts_with("./") || s.starts_with("../"))
                {
                    arg.insert("value".into(), Value::String("<intra-tree-require>".into()));
                }
                stack.extend(map.values_mut());
            }
            Value::Array(items) => stack.extend(items.iter_mut()),
            _ => {}
        }
    }
}

fn analyze_file(path: &Path) -> Option<FileAnalysis> {
    let code = std::fs::read_to_string(path).ok()?;
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &code);
    if !ingest.errors.is_empty() {
        return None;
    }
    let tables = SymbolTables::build(ingest.semantic());
    let mut json = program_estree_json(ingest.program);
    mask_intra_tree_requires(&mut json);
    let (out, tokens) = canonical_tokens(&json, &tables, &IDENTITY_KEY);
    let scoping = ingest.semantic().scoping();
    let declared = scoping
        .symbol_ids()
        .map(|s| scoping.symbol_name(s).to_string())
        .collect();
    Some(FileAnalysis {
        sig: out.hash,
        declared,
        tokens,
    })
}

/// Bun/minifier token shape: short, no separators.
fn minified(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 5
        && (b[0].is_ascii_alphabetic() || b[0] == b'_' || b[0] == b'$')
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$')
}

/// Why two same-path files diverge: only a free, minified-shaped name
/// declared in NEITHER file differing is a minifier reroll.
fn free_minified_reroll(a: &FileAnalysis, b: &FileAnalysis) -> bool {
    if a.tokens.len() != b.tokens.len() {
        return false;
    }
    let mut saw = false;
    for (x, y) in a.tokens.iter().zip(&b.tokens) {
        if x == y {
            continue;
        }
        saw = true;
        let (Some(nx), Some(ny)) = (x.strip_prefix("I="), y.strip_prefix("I=")) else {
            return false;
        };
        if !minified(nx) || !minified(ny) || a.declared.contains(nx) || b.declared.contains(ny) {
            return false;
        }
    }
    saw
}

fn line_count(path: &Path) -> Result<usize, String> {
    Ok(std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .split('\n')
        .count())
}

/// `diff` of the two Bun manifests, `<`/`>` lines, by JSON field.
fn manifest_lines(prior: &Path, fresh: &Path, differ: &Differ) -> Result<usize, String> {
    let (mp, mf) = (prior.join(BUN_MANIFEST), fresh.join(BUN_MANIFEST));
    if !mp.is_file() || !mf.is_file() {
        return Ok(0);
    }
    differ.changed_lines(&mp, &mf)
}

impl VendorChurn {
    /// Charge one file to a bucket and record its row.
    fn charge(
        &mut self,
        bucket: VendorBucket,
        lines: usize,
        prior: Option<&str>,
        fresh: Option<&str>,
    ) {
        let b = match bucket {
            VendorBucket::Identical => &mut self.identical,
            VendorBucket::NameOnly => &mut self.name_only,
            VendorBucket::FreeReroll => &mut self.free_reroll,
            VendorBucket::MovedPath => &mut self.moved_path,
            VendorBucket::RealChange => &mut self.real_change,
            VendorBucket::TrulyAdded => &mut self.truly_added,
            VendorBucket::TrulyRemoved => &mut self.truly_removed,
            VendorBucket::Relocated => &mut self.relocated,
            VendorBucket::Unparsed => &mut self.unparsed,
        };
        b.bump(lines);
        self.file_rows.push(VendorFileRow {
            bucket,
            lines,
            prior: prior.map(str::to_string),
            fresh: fresh.map(str::to_string),
        });
    }
}

/// A same-path file's bucket (and, for a moved one, the prior path its
/// content came from): byte-identical; one signature (a local-name
/// reroll); a free minified-name reroll; content that exists elsewhere in
/// the prior tree (a path swap); else real change.
fn same_path_bucket<'p>(
    identical: bool,
    prior: Option<&FileAnalysis>,
    fresh: &FileAnalysis,
    moved_from: Option<&'p str>,
) -> (VendorBucket, Option<&'p str>) {
    if identical {
        (VendorBucket::Identical, None)
    } else if prior.is_some_and(|p| p.sig == fresh.sig) {
        (VendorBucket::NameOnly, None)
    } else if prior.is_some_and(|p| free_minified_reroll(p, fresh)) {
        (VendorBucket::FreeReroll, None)
    } else if moved_from.is_some() {
        (VendorBucket::MovedPath, moved_from)
    } else {
        (VendorBucket::RealChange, None)
    }
}

/// Decompose the vendor churn between two vendor trees.
pub fn decompose(
    prior_dir: &Path,
    fresh_dir: &Path,
    differ: &Differ,
) -> Result<VendorChurn, String> {
    let prior_rel = list_js(prior_dir)?;
    let fresh_rel = list_js(fresh_dir)?;
    let prior_info: Vec<Option<FileAnalysis>> =
        humanify_ag_core::par::map_ordered(&prior_rel, |r| analyze_file(&prior_dir.join(r)));
    let fresh_info: Vec<Option<FileAnalysis>> =
        humanify_ag_core::par::map_ordered(&fresh_rel, |r| analyze_file(&fresh_dir.join(r)));
    let pinfo: HashMap<&str, &FileAnalysis> = prior_rel
        .iter()
        .zip(&prior_info)
        .filter_map(|(r, a)| Some((r.as_str(), a.as_ref()?)))
        .collect();
    let finfo: HashMap<&str, &FileAnalysis> = fresh_rel
        .iter()
        .zip(&fresh_info)
        .filter_map(|(r, a)| Some((r.as_str(), a.as_ref()?)))
        .collect();
    let mut prior_by_sig: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in &prior_rel {
        if let Some(a) = pinfo.get(r.as_str()) {
            prior_by_sig.entry(a.sig.as_str()).or_default().push(r);
        }
    }
    let fresh_sigs: HashSet<&str> = finfo.values().map(|a| a.sig.as_str()).collect();

    // Every same-path diff is independent: count them on the pool.
    let changed: Vec<Option<Result<usize, String>>> =
        humanify_ag_core::par::map_ordered(&fresh_rel, |r| {
            let (p, f) = (prior_dir.join(r), fresh_dir.join(r));
            if !p.is_file() || !finfo.contains_key(r.as_str()) {
                return None;
            }
            match (std::fs::read(&p), std::fs::read(&f)) {
                (Ok(a), Ok(b)) if a == b => Some(Ok(0)),
                _ => Some(differ.changed_lines(&p, &f)),
            }
        });

    let mut v = VendorChurn::default();
    let prior_set: HashSet<&str> = prior_rel.iter().map(String::as_str).collect();
    let mut added: Vec<&str> = Vec::new();
    let mut changed_in_place: Vec<&str> = Vec::new();
    for (i, rel) in fresh_rel.iter().enumerate() {
        let rel = rel.as_str();
        let fp = fresh_dir.join(rel);
        let Some(fa) = finfo.get(rel) else {
            v.charge(VendorBucket::Unparsed, line_count(&fp)?, None, Some(rel));
            continue;
        };
        let moved_from = prior_by_sig.get(fa.sig.as_str()).map(|f| f[0]);
        if prior_set.contains(rel) {
            let lines = changed[i].clone().unwrap_or(Ok(0))?;
            let identical = std::fs::read(prior_dir.join(rel)).ok() == std::fs::read(&fp).ok();
            let (bucket, from) =
                same_path_bucket(identical, pinfo.get(rel).copied(), fa, moved_from);
            let lines = if bucket == VendorBucket::Identical {
                0
            } else {
                lines
            };
            v.charge(bucket, lines, Some(from.unwrap_or(rel)), Some(rel));
            if bucket == VendorBucket::RealChange {
                changed_in_place.push(rel);
            }
        } else if let Some(from) = moved_from {
            v.charge(
                VendorBucket::MovedPath,
                line_count(&fp)?,
                Some(from),
                Some(rel),
            );
        } else {
            added.push(rel);
        }
    }
    let fresh_set: HashSet<&str> = fresh_rel.iter().map(String::as_str).collect();
    let removed: Vec<&str> = prior_rel
        .iter()
        .map(String::as_str)
        .filter(|r| !fresh_set.contains(r))
        .filter(|r| {
            pinfo
                .get(r)
                .is_none_or(|a| !fresh_sigs.contains(a.sig.as_str()))
        })
        .collect();

    // Pair a file that lost its path with one that gained one, by content,
    // before charging either whole.
    let shingles =
        |info: &HashMap<&str, &FileAnalysis>, rels: &[&str]| -> BTreeMap<String, HashSet<String>> {
            rels.iter()
                .filter_map(|r| Some((r.to_string(), content_shingles(&info.get(r)?.tokens))))
                .collect()
        };
    let pairs = pair_relocated(
        &shingles(&pinfo, &removed),
        &shingles(&finfo, &added),
        &shingles(&pinfo, &changed_in_place),
        &shingles(&finfo, &changed_in_place),
    );
    let mut paired_prior: HashSet<String> = HashSet::new();
    let mut paired_fresh: HashSet<String> = HashSet::new();
    for pair in pairs {
        let (pp, fp) = (prior_dir.join(&pair.prior), fresh_dir.join(&pair.fresh));
        let lines = differ.changed_lines(&pp, &fp)?;
        let reroll = match (
            pinfo.get(pair.prior.as_str()),
            finfo.get(pair.fresh.as_str()),
        ) {
            (Some(a), Some(b)) => free_minified_reroll(a, b),
            _ => false,
        };
        let bucket = if reroll {
            VendorBucket::FreeReroll
        } else {
            VendorBucket::RealChange
        };
        v.charge(bucket, lines, Some(&pair.prior), Some(&pair.fresh));
        let draw = line_count(&pp)? + line_count(&fp)? - lines;
        v.charge(
            VendorBucket::Relocated,
            draw,
            Some(&pair.prior),
            Some(&pair.fresh),
        );
        paired_prior.insert(pair.prior.clone());
        paired_fresh.insert(pair.fresh.clone());
        v.relocated_pairs.push(RelocatedPair {
            real_lines: lines,
            draw_lines: draw,
            ..pair
        });
    }
    for rel in added {
        if paired_fresh.contains(rel) {
            continue;
        }
        v.charge(
            VendorBucket::TrulyAdded,
            line_count(&fresh_dir.join(rel))?,
            None,
            Some(rel),
        );
    }
    for rel in removed {
        if paired_prior.contains(rel) {
            continue;
        }
        v.charge(
            VendorBucket::TrulyRemoved,
            line_count(&prior_dir.join(rel))?,
            Some(rel),
            None,
        );
    }
    v.manifest_changed_lines = manifest_lines(prior_dir, fresh_dir, differ)?;
    let body = v.name_only.lines
        + v.free_reroll.lines
        + v.real_change.lines
        + v.moved_path.lines
        + v.unparsed.lines;
    v.total_lines = body
        + v.manifest_changed_lines
        + v.truly_added.lines
        + v.truly_removed.lines
        + v.relocated.lines;
    v.noise_lines = v.name_only.lines
        + v.free_reroll.lines
        + v.moved_path.lines
        + v.relocated.lines
        + v.manifest_changed_lines;
    v.real_lines = v.real_change.lines + v.truly_added.lines + v.truly_removed.lines;
    Ok(v)
}

// ------------------------------------------------------- relocation pairing

/// Pair only above this similarity...
const MIN_SCORE: f64 = 0.5;
/// ...and only when it beats every competing candidate by this much...
const MIN_MARGIN: f64 = 0.2;
/// ...and only files with at least this many content shingles.
const MIN_SHINGLES: usize = 20;
const SHINGLE: usize = 5;
/// A pair whose size bound is under this is not intersected; the bound
/// stands in for its score (an over-estimate: it can only shrink a margin).
const SKIP_BOUND: f64 = 0.1;

fn is_ordinal(tok: &str, prefix: char) -> bool {
    tok.strip_prefix(prefix)
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// The content-bearing part of one token: slots collapse to one token,
/// string / template text splits into words, identifiers / numbers /
/// regexps / node types / string scalars (operators, kinds) stay, and
/// punctuation, field labels and flag values drop.
fn content_of(tok: &str, out: &mut Vec<String>) {
    if is_ordinal(tok, '$') {
        out.push("$".into());
    } else if is_ordinal(tok, 'L') {
        out.push("L".into());
    } else if tok.starts_with("S=") || tok.starts_with("Q=") {
        out.push(tok[..2].to_string());
        let raw = &tok[2..];
        let text = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string());
        for w in text.split(is_js_whitespace).filter(|w| !w.is_empty()) {
            out.push(format!("w={w}"));
        }
    } else if ["I=", "N=", "B=", "R="].iter().any(|p| tok.starts_with(p))
        || tok.ends_with('{')
        || tok.starts_with('"')
    {
        out.push(tok.to_string());
    }
}

/// 5-token shingles of a file's content tokens.
pub fn content_shingles(tokens: &[String]) -> HashSet<String> {
    let mut c = Vec::new();
    for t in tokens {
        content_of(t, &mut c);
    }
    if c.len() < SHINGLE {
        return std::iter::once(c.join("\u{0}")).collect();
    }
    c.windows(SHINGLE).map(|w| w.join("\u{0}")).collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let inter = small.iter().filter(|x| large.contains(*x)).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

fn similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let bound = a.len().min(b.len()) as f64 / a.len().max(b.len()).max(1) as f64;
    if bound < SKIP_BOUND {
        bound
    } else {
        jaccard(a, b)
    }
}

/// Best and second-best score over `others` (a tie makes second = best).
fn top2(others: &[String], score: impl Fn(&str) -> f64) -> (f64, &str, f64) {
    let (mut best, mut best_other, mut second) = (-1.0f64, "", 0.0f64);
    for o in others {
        let s = score(o);
        if s > best {
            second = second.max(best);
            best = s;
            best_other = o;
        } else {
            second = second.max(s);
        }
    }
    (best, best_other, second)
}

/// Mutual-best pairing of removed (prior) to added (fresh) files; the
/// same-path changed files compete without being pairable. Sorted by
/// fresh path; ties never pair.
fn pair_relocated(
    removed: &BTreeMap<String, HashSet<String>>,
    added: &BTreeMap<String, HashSet<String>>,
    contenders_prior: &BTreeMap<String, HashSet<String>>,
    contenders_fresh: &BTreeMap<String, HashSet<String>>,
) -> Vec<RelocatedPair> {
    let side = |pairable: &'_ BTreeMap<String, HashSet<String>>,
                others: &'_ BTreeMap<String, HashSet<String>>| {
        let mut out: BTreeMap<String, (HashSet<String>, bool)> = BTreeMap::new();
        for (k, sh) in pairable {
            out.insert(format!("+{k}"), (sh.clone(), sh.len() >= MIN_SHINGLES));
        }
        for (k, sh) in others {
            out.insert(format!("={k}"), (sh.clone(), false));
        }
        out
    };
    let prior = side(removed, contenders_prior);
    let fresh = side(added, contenders_fresh);
    let prior_keys: Vec<String> = prior.keys().cloned().collect();
    let fresh_keys: Vec<String> = fresh.keys().cloned().collect();
    let memo = std::cell::RefCell::new(HashMap::<(String, String), f64>::new());
    let score = |p: &str, f: &str| -> f64 {
        let key = (p.to_string(), f.to_string());
        if let Some(s) = memo.borrow().get(&key) {
            return *s;
        }
        let s = similarity(&prior[p].0, &fresh[f].0);
        memo.borrow_mut().insert(key, s);
        s
    };
    let mut out = Vec::new();
    for f in &fresh_keys {
        if !fresh[f].1 {
            continue;
        }
        let (best, p, second_f) = top2(&prior_keys, |p| score(p, f));
        if best < MIN_SCORE || !prior.get(p).is_some_and(|e| e.1) {
            continue;
        }
        let (_, back, second_p) = top2(&fresh_keys, |g| score(p, g));
        if back != f {
            continue;
        }
        let runner_up = second_f.max(second_p);
        if best - runner_up < MIN_MARGIN {
            continue;
        }
        out.push(RelocatedPair {
            prior: p[1..].to_string(),
            fresh: f[1..].to_string(),
            score: best,
            runner_up,
            real_lines: 0,
            draw_lines: 0,
        });
    }
    out
}

#[cfg(test)]
#[path = "vendor_test.rs"]
mod vendor_test;
