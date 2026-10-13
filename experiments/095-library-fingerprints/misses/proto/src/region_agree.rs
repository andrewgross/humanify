//! exp095 "misses", Task 2: literal agreement over a REGION instead of one
//! statement. A statement's literal set is small (the rule needs 6+
//! literals, 20 bits and 90% cover), so the agreement rule refuses most
//! structural claims. Here consecutive structural claims of one family
//! (gaps of up to GAP unclaimed statements) form a region; the literal rule
//! (a copy of libindex::lookup's: IDF weights, wordy cap 2 bits, cover 0.9,
//! score 20, margin 10, >= 6 literals) runs on the UNION of the region's
//! statement literal sets; a region whose literal claim names the same
//! family keeps all its claims.
//!
//! Usage: exp095-region-agree <literals.jsonl> <jobs.tsv>
//!   jobs: query rows<TAB>bundle<TAB>structural claims (variant-lookup)<TAB>out claims
//!   env GAP (default 3)
use std::collections::{BTreeSet, HashMap};

use humanify_core::libindex::format::LiteralRow;
use humanify_core::libindex::literals::{LiteralMode, code_literals};
use serde_json::{Value, json};

struct Lits {
    ids: HashMap<String, u32>,
    postings: Vec<Vec<u32>>,
    package_df: Vec<u32>,
    file_owner: Vec<String>,
    file_lits: Vec<Vec<u32>>,
    packages: u32,
}

const WORDY_CAP: f64 = 2.0;

fn wordy(l: &str) -> bool {
    let Some(rest) = l.strip_prefix("S:").or_else(|| l.strip_prefix("P:")) else {
        return false;
    };
    let mut c = rest.chars();
    c.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && c.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '-'))
}

impl Lits {
    fn load(path: &str) -> Lits {
        let mut l = Lits {
            ids: HashMap::new(),
            postings: vec![],
            package_df: vec![],
            file_owner: vec![],
            file_lits: vec![],
            packages: 0,
        };
        let mut holders: Vec<BTreeSet<String>> = vec![];
        let mut owners = BTreeSet::new();
        for line in std::fs::read_to_string(path).expect("literals").lines() {
            let r: LiteralRow = serde_json::from_str(line).unwrap();
            let file = l.file_owner.len() as u32;
            owners.insert(r.owner.clone());
            let mut ids: Vec<u32> = r
                .lits
                .iter()
                .map(|x| {
                    let n = l.ids.len() as u32;
                    *l.ids.entry(x.clone()).or_insert(n)
                })
                .collect();
            ids.sort_unstable();
            ids.dedup();
            for &id in &ids {
                if id as usize >= l.postings.len() {
                    l.postings.resize(id as usize + 1, vec![]);
                    holders.resize(id as usize + 1, BTreeSet::new());
                }
                l.postings[id as usize].push(file);
                holders[id as usize].insert(r.owner.clone());
            }
            l.file_owner.push(r.owner);
            l.file_lits.push(ids);
        }
        l.package_df = holders.iter().map(|h| h.len() as u32).collect();
        l.packages = owners.len() as u32;
        l
    }

    fn weight(&self, lit: &str) -> f64 {
        let n = f64::from(self.packages.max(1));
        let w = match self.ids.get(lit) {
            Some(&id) => (n / f64::from(self.package_df[id as usize].max(1))).log2(),
            None => n.log2() + 1.0,
        };
        if wordy(lit) { w.min(WORDY_CAP) } else { w }
    }

    /// lookup.rs `literal_claim`, on a literal set.
    fn claim(&self, lits: &BTreeSet<String>) -> Option<(String, f64)> {
        if lits.len() < 6 {
            return None;
        }
        let mut weighted: Vec<(f64, Option<u32>)> = lits
            .iter()
            .map(|l| (self.weight(l), self.ids.get(l).copied()))
            .collect();
        let total: f64 = weighted.iter().map(|w| w.0).sum();
        weighted.sort_by(|a, b| b.0.total_cmp(&a.0));
        let need = (1.0 - 0.9) * total + 10.0;
        let mut cands = BTreeSet::new();
        let mut carried = 0.0;
        for &(w, id) in &weighted {
            if let Some(id) = id {
                cands.extend(self.postings[id as usize].iter().copied());
            }
            carried += w;
            if carried > need {
                break;
            }
        }
        let query: Vec<(u32, f64)> = weighted
            .iter()
            .filter_map(|&(w, id)| Some((id?, w)))
            .collect();
        let mut best: HashMap<&str, f64> = HashMap::new();
        for f in cands {
            let held = &self.file_lits[f as usize];
            let s: f64 = query
                .iter()
                .filter(|(id, _)| held.binary_search(id).is_ok())
                .map(|&(_, w)| w)
                .sum();
            let e = best
                .entry(self.file_owner[f as usize].as_str())
                .or_insert(0.0);
            *e = e.max(s);
        }
        let mut ranked: Vec<(&str, f64)> = best.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        let (p, score) = *ranked.first()?;
        let runner = ranked.get(1).map_or(0.0, |r| r.1);
        (score >= 20.0 && score >= 0.9 * total && score - runner >= 10.0)
            .then(|| (p.to_string(), score))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let gap: usize = std::env::var("GAP")
        .ok()
        .and_then(|g| g.parse().ok())
        .unwrap_or(3);
    let lits = Lits::load(&args[1]);
    let families = humanify_core::libindex::families::PackageFamilies::shipped();
    for line in std::fs::read_to_string(&args[2]).expect("jobs").lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let code = std::fs::read_to_string(f[1]).expect("bundle");
        let rows: Vec<Value> = std::fs::read_to_string(f[0])
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .filter(|r: &Value| matches!(r["k"].as_str(), Some("Statement" | "LazyBody")))
            .collect();
        let claims: HashMap<u64, String> = std::fs::read_to_string(f[2])
            .unwrap_or_default()
            .lines()
            .map(|l| {
                let v: Value = serde_json::from_str(l).unwrap();
                (
                    v["s"].as_u64().unwrap(),
                    v["package"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        // regions: (family, [row index])
        let mut regions: Vec<(String, Vec<usize>)> = vec![];
        let mut last: Option<(String, usize)> = None;
        for (i, r) in rows.iter().enumerate() {
            let Some(p) = claims.get(&r["s"].as_u64().unwrap()) else {
                continue;
            };
            let fam = families.family(p);
            match (&last, regions.last_mut()) {
                (Some((lf, li)), Some(reg)) if *lf == fam && i - li <= gap + 1 => reg.1.push(i),
                _ => regions.push((fam.clone(), vec![i])),
            }
            last = Some((fam, i));
        }
        let mut out = String::new();
        for (fam, members) in regions {
            let mut set = BTreeSet::new();
            for &i in &members {
                let (s, e) = (
                    rows[i]["s"].as_u64().unwrap() as usize,
                    rows[i]["e"].as_u64().unwrap() as usize,
                );
                if let Some(l) = code
                    .get(s..e)
                    .and_then(|t| code_literals(t, LiteralMode::Bundled))
                {
                    set.extend(l);
                }
            }
            let Some((named, score)) = lits.claim(&set) else {
                continue;
            };
            if families.family(&named) != fam {
                continue;
            }
            for &i in &members {
                let s = rows[i]["s"].as_u64().unwrap();
                out.push_str(&json!({"s": s, "e": rows[i]["e"], "package": claims[&s], "region": members.len(), "score": score}).to_string());
                out.push('\n');
            }
        }
        std::fs::write(f[3], out).unwrap();
    }
}
