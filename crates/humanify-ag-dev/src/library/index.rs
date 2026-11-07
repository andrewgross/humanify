//! The library index as the scorer reads it (exp095 `mmlib.py`): the unit
//! table of one slice (`units.<slice>.jsonl`: whole-unit and function
//! print hashes -> the packages and files that hold them), an emulation of
//! the in-run structural rule (`structural`, `claim_all`), and the miss
//! classifier (exp095 `decompose.py` `classify`), all unchanged.
//!
//! Python cached the table as a pickle beside the index; this reads the
//! JSONL itself, the file the pickle was made from.

use std::collections::{BTreeSet, HashMap};
use std::io::BufRead;
use std::path::Path;

use serde::Deserialize;

use super::families::Libix;

/// One index row: a print hash held by (owner, file).
pub struct Row {
    pub owner: u32,
    pub file: u32,
    pub nodes: i64,
    pub module_bytes: i64,
    /// Version ranges `[lo, hi]` (interned version strings).
    pub versions: Vec<(u32, u32)>,
}

/// mmlib `Index`: one slice's unit table and the indexed versions.
pub struct Index {
    strings: Vec<String>,
    string_id: HashMap<String, u32>,
    pub rows: Vec<Row>,
    by_hash: HashMap<u64, Vec<u32>>,
    /// owner name -> its indexed versions, in versions.json order.
    versions: HashMap<String, Vec<String>>,
    /// (owner id, file id) -> rows; (owner id, basename) -> rows.
    by_file: HashMap<(u32, u32), Vec<u32>>,
    by_base: HashMap<(u32, String), Vec<u32>>,
}

#[derive(Deserialize)]
struct UnitLine<'a> {
    #[serde(borrow)]
    hash: &'a str,
    #[serde(borrow)]
    owner: std::borrow::Cow<'a, str>,
    #[serde(borrow)]
    file: std::borrow::Cow<'a, str>,
    nodes: i64,
    #[serde(rename = "moduleBytes")]
    module_bytes: i64,
    #[serde(borrow)]
    versions: Vec<(std::borrow::Cow<'a, str>, std::borrow::Cow<'a, str>)>,
}

/// A print hash (16 hex digits) as a key.
pub fn hash_key(h: &str) -> Result<u64, String> {
    u64::from_str_radix(h, 16).map_err(|_| format!("not a print hash: {h:?}"))
}

impl Index {
    /// Load `units.<slice>.jsonl` and `versions.json` from an index dir.
    pub fn load(dir: &Path, slice: &str) -> Result<Index, String> {
        let vpath = dir.join("versions.json");
        let vtext =
            std::fs::read_to_string(&vpath).map_err(|e| format!("{}: {e}", vpath.display()))?;
        let versions: HashMap<String, Vec<String>> =
            serde_json::from_str(&vtext).map_err(|e| format!("{}: {e}", vpath.display()))?;
        let mut ix = Index {
            strings: Vec::new(),
            string_id: HashMap::new(),
            rows: Vec::new(),
            by_hash: HashMap::new(),
            versions,
            by_file: HashMap::new(),
            by_base: HashMap::new(),
        };
        let upath = dir.join(format!("units.{slice}.jsonl"));
        let fh = std::fs::File::open(&upath).map_err(|e| format!("{}: {e}", upath.display()))?;
        let mut reader = std::io::BufReader::with_capacity(1 << 22, fh);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .map_err(|e| format!("{}: {e}", upath.display()))?;
            if n == 0 {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            let r: UnitLine =
                serde_json::from_str(&line).map_err(|e| format!("{}: {e}", upath.display()))?;
            let owner = ix.intern(&r.owner);
            let file = ix.intern(&r.file);
            let versions = r
                .versions
                .iter()
                .map(|(lo, hi)| (ix.intern(lo), ix.intern(hi)))
                .collect();
            let id = ix.rows.len() as u32;
            ix.rows.push(Row {
                owner,
                file,
                nodes: r.nodes,
                module_bytes: r.module_bytes,
                versions,
            });
            ix.by_hash.entry(hash_key(r.hash)?).or_default().push(id);
            ix.by_file.entry((owner, file)).or_default().push(id);
            let base = r.file.rsplit('/').next().unwrap_or(&r.file).to_string();
            ix.by_base.entry((owner, base)).or_default().push(id);
        }
        Ok(ix)
    }

    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.string_id.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.string_id.insert(s.to_string(), i);
        i
    }

    pub fn name(&self, id: u32) -> &str {
        &self.strings[id as usize]
    }

    /// The rows that hold print hash `h` (in file order).
    pub fn rows_of(&self, h: u64) -> impl Iterator<Item = &Row> {
        self.by_hash
            .get(&h)
            .into_iter()
            .flatten()
            .map(|&i| &self.rows[i as usize])
    }

    /// Is `v` in mmlib `expand(owner, ranges)`?
    pub fn expands_to(&self, owner: &str, ranges: &[(u32, u32)], v: &str) -> bool {
        let vs = self.versions.get(owner).map(Vec::as_slice).unwrap_or(&[]);
        let pos = |x: &str| vs.iter().rposition(|y| y == x);
        ranges.iter().any(|&(lo, hi)| {
            let (lo, hi) = (self.name(lo), self.name(hi));
            match (pos(lo), pos(hi)) {
                (Some(a), Some(b)) => a <= b && vs[a..=b].iter().any(|y| y == v),
                _ => lo == v || hi == v,
            }
        })
    }

    pub fn has_package(&self, p: &str) -> bool {
        self.versions.contains_key(p)
    }

    pub fn has_version(&self, p: &str, v: Option<&str>) -> bool {
        v.is_some_and(|v| {
            self.versions
                .get(p)
                .is_some_and(|vs| vs.iter().any(|x| x == v))
        })
    }

    fn id_of(&self, s: &str) -> Option<u32> {
        self.string_id.get(s).copied()
    }

    /// mmlib `file_versions(p, f)` contains `v`.
    pub fn file_has_version(&self, p: &str, f: &str, v: Option<&str>) -> bool {
        let (Some(v), Some(o), Some(fi)) = (v, self.id_of(p), self.id_of(f)) else {
            return false;
        };
        self.by_file.get(&(o, fi)).is_some_and(|rows| {
            rows.iter()
                .any(|&r| self.expands_to(p, &self.rows[r as usize].versions, v))
        })
    }

    /// decompose `basenames(ix)[(p, base)]` contains `v`.
    pub fn basename_has_version(&self, p: &str, base: &str, v: Option<&str>) -> bool {
        let (Some(v), Some(o)) = (v, self.id_of(p)) else {
            return false;
        };
        self.by_base
            .get(&(o, base.to_string()))
            .is_some_and(|rows| {
                rows.iter()
                    .any(|&r| self.expands_to(p, &self.rows[r as usize].versions, v))
            })
    }

    /// mmlib `owners(h, module_bytes, spec)`.
    pub fn owners(
        &self,
        h: u64,
        module_bytes: Option<i64>,
        spec: Option<(i64, i64)>,
    ) -> BTreeSet<&str> {
        self.rows_of(h)
            .filter(|r| match (spec, module_bytes) {
                (Some((a, b)), Some(mb)) => r.module_bytes <= a * mb + b,
                _ => true,
            })
            .map(|r| self.name(r.owner))
            .collect()
    }
}

/// One print: (start, end, hash, nodes, content).
#[derive(Clone, Copy, Debug)]
pub struct Print {
    pub s: i64,
    pub e: i64,
    pub h: u64,
    pub nodes: i64,
    pub content: i64,
}

/// A unit as the misses lane sees it (score.py `run_units`).
#[derive(Clone, Debug)]
pub struct Unit {
    pub i: usize,
    pub s: i64,
    pub e: i64,
    pub b: i64,
    pub k: String,
    pub rt: bool,
    pub mb: Option<i64>,
    pub bindings: i64,
    pub m: Option<i64>,
    /// (whole, functions) when the unit has a print.
    pub w: Option<(Print, Vec<Print>)>,
}

/// mmlib `RULES`.
#[derive(Clone, Copy)]
pub struct Rules {
    pub min_nodes: i64,
    pub min_content: i64,
    pub min_function_content: i64,
    pub cover: f64,
    pub max_packages: usize,
    pub support: usize,
    pub strong: i64,
    pub spec: Option<(i64, i64)>,
}

pub const RULES: Rules = Rules {
    min_nodes: 30,
    min_content: 3,
    min_function_content: 0,
    cover: 0.5,
    max_packages: 3,
    support: 10,
    strong: 100,
    spec: Some((4, 4096)),
};

fn eligible(u: &Unit, rules: &Rules) -> (Print, bool, Vec<Print>) {
    let (whole, fns) = u.w.as_ref().expect("a printed unit");
    let ok_w = whole.nodes >= rules.min_nodes && whole.content >= rules.min_content;
    let ok_f = fns
        .iter()
        .filter(|f| f.nodes >= rules.min_nodes && f.content >= rules.min_function_content)
        .copied()
        .collect();
    (*whole, ok_w, ok_f)
}

/// Python's `sorted(xs, key=lambda f: -f.nodes)` (stable).
fn by_nodes_desc<T>(mut xs: Vec<T>, nodes: impl Fn(&T) -> i64) -> Vec<T> {
    xs.sort_by_key(|x| std::cmp::Reverse(nodes(x)));
    xs
}

fn overlaps(taken: &[(i64, i64)], f: &Print) -> bool {
    taken.iter().any(|&(a, b)| !(f.e <= a || f.s >= b))
}

/// Largest non-overlapping functions first: their total nodes.
pub fn greedy_cover(fns: Vec<Print>) -> i64 {
    let mut taken = Vec::new();
    let mut tot = 0;
    for f in by_nodes_desc(fns, |f| f.nodes) {
        if overlaps(&taken, &f) {
            continue;
        }
        taken.push((f.s, f.e));
        tot += f.nodes;
    }
    tot
}

/// Nodes the structural rule could cover if every eligible print hit.
pub fn max_cover(u: &Unit) -> i64 {
    let (whole, ok_w, ok_f) = eligible(u, &RULES);
    if ok_w {
        whole.nodes
    } else {
        greedy_cover(ok_f)
    }
}

/// Nodes covered when a print counts only if `hit(print)`.
pub fn cover_by(u: &Unit, hit: impl Fn(&Print) -> bool) -> i64 {
    let (whole, ok_w, ok_f) = eligible(u, &RULES);
    if ok_w && hit(&whole) {
        return whole.nodes;
    }
    greedy_cover(ok_f.into_iter().filter(|f| hit(f)).collect())
}

/// The rule's structural step on one unit: (package, covered) or None.
pub fn structural(
    ix: &Index,
    u: &Unit,
    rules: &Rules,
    previous: Option<&str>,
) -> Option<(String, i64)> {
    let (whole, ok_w, ok_f) = eligible(u, rules);
    let mut score: HashMap<String, i64> = HashMap::new();
    if ok_w {
        let o = ix.owners(whole.h, u.mb, rules.spec);
        if !o.is_empty() && o.len() <= rules.max_packages {
            for p in o {
                score.insert(p.to_string(), whole.nodes);
            }
        }
    }
    if score.is_empty() {
        let mut hits = Vec::new();
        for f in ok_f {
            let o = ix.owners(f.h, u.mb, rules.spec);
            if !o.is_empty() && o.len() <= rules.max_packages {
                hits.push((f, o));
            }
        }
        let hits = by_nodes_desc(hits, |x| x.0.nodes);
        let mut taken = Vec::new();
        for (f, o) in hits {
            if overlaps(&taken, &f) {
                continue;
            }
            taken.push((f.s, f.e));
            for p in o {
                *score.entry(p.to_string()).or_insert(0) += f.nodes;
            }
        }
    }
    let best = *score.values().max()?;
    let mut tied: Vec<&String> = score
        .iter()
        .filter(|(_, v)| **v == best)
        .map(|(p, _)| p)
        .collect();
    tied.sort();
    let p = match previous {
        Some(prev) if tied.iter().any(|t| *t == prev) => prev.to_string(),
        _ => tied[0].clone(),
    };
    if (best as f64) < rules.cover * (whole.nodes.max(1) as f64) {
        return None;
    }
    Some((p, best))
}

/// `claim_units` over statement units in order (structural + support).
pub fn claim_all(ix: &Index, units: &[&Unit]) -> HashMap<usize, String> {
    let q: Vec<&Unit> = units
        .iter()
        .copied()
        .filter(|u| (u.k == "Statement" || u.k == "LazyBody") && !u.rt && u.w.is_some())
        .collect();
    let mut prev: Option<String> = None;
    let mut st: Vec<Option<(String, i64)>> = Vec::new();
    for u in &q {
        let h = structural(ix, u, &RULES, prev.as_deref());
        if let Some((p, _)) = &h {
            prev = Some(p.clone());
        }
        st.push(h);
    }
    let mut out = HashMap::new();
    for (i, u) in q.iter().enumerate() {
        let Some((p, cov)) = &st[i] else { continue };
        if RULES.support > 0 && *cov < RULES.strong {
            let lo = i.saturating_sub(RULES.support);
            let hi = (i + RULES.support + 1).min(q.len());
            if !(lo..hi).any(|j| j != i && st[j].as_ref().is_some_and(|x| &x.0 == p)) {
                continue;
            }
        }
        out.insert(u.i, p.clone());
    }
    out
}

/// The truth facts `classify` reads for a library unit.
pub struct TruthFacts<'a> {
    pub pkg: &'a str,
    pub version: Option<&'a str>,
    pub file: Option<&'a str>,
}

/// decompose.py `classify`: one missed library unit -> its class.
#[allow(clippy::too_many_arguments)]
pub fn classify(
    ix: &Index,
    fam: &Libix,
    u: &Unit,
    t: &TruthFacts,
    claim: Option<&str>,
    vetoed: &HashMap<usize, String>,
    struct_claims: &HashMap<usize, String>,
    rule: &str,
) -> &'static str {
    let fam_p = fam.family(t.pkg);
    let (whole, _) = u.w.as_ref().expect("a printed unit");
    let wn = whole.nodes.max(1) as f64;
    let fam_hit = |p: &Print| {
        ix.rows_of(p.h)
            .any(|r| fam.family(ix.name(r.owner)) == fam_p)
    };
    let other_hit = |p: &Print| {
        ix.rows_of(p.h)
            .any(|r| fam.family(ix.name(r.owner)) != fam_p)
    };
    let cov_any = cover_by(u, fam_hit) as f64;
    if !ix.has_package(t.pkg) {
        return "a";
    }
    if !ix.has_version(t.pkg, t.version) && cov_any < 0.5 * wn {
        return "b";
    }
    if (max_cover(u) as f64) < 0.5 * wn {
        return "c";
    }
    let other = |c: Option<&str>| c.is_some_and(|c| fam.family(c) != fam_p);
    if cov_any >= 0.5 * wn {
        if vetoed.get(&u.i).is_some_and(|v| fam.family(v) == fam_p) {
            return "e-veto";
        }
        if rule == "agree"
            && struct_claims
                .get(&u.i)
                .is_some_and(|v| fam.family(v) == fam_p)
        {
            return "e-agree";
        }
        if other(claim) {
            return "f";
        }
        let r0 = RULES;
        match structural(ix, u, &r0, None) {
            Some((p, cov)) if fam.family(&p) == fam_p => {
                return if cov < r0.strong {
                    "e-support"
                } else if rule == "agree" {
                    "e-agree"
                } else {
                    "e-other"
                };
            }
            Some(_) => return "e-ambig",
            None => {}
        }
        let r1 = Rules { spec: None, ..r0 };
        if structural(ix, u, &r1, None).is_some_and(|(p, _)| fam.family(&p) == fam_p) {
            return "e-spec";
        }
        let r2 = Rules {
            spec: None,
            max_packages: 1_000_000,
            ..r0
        };
        if structural(ix, u, &r2, None).is_some_and(|(p, _)| fam.family(&p) == fam_p) {
            return "e-ambig";
        }
        return "e-other";
    }
    if other(claim) {
        return "f";
    }
    if (cover_by(u, other_hit) as f64) >= 0.5 * wn {
        return "f";
    }
    let Some(f) = t.file else {
        return "d-nofile";
    };
    if ix.file_has_version(t.pkg, f, t.version) {
        return "d-file";
    }
    let base = f.rsplit('/').next().unwrap_or(f);
    if ix.basename_has_version(t.pkg, base, t.version) {
        return "d-variant";
    }
    "d-absent"
}
