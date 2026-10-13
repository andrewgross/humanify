//! Looking statement units up in a library index (exp095 library
//! discovery's index seeds; docs/design/library-discovery.md §5): the
//! structural rule (the lab's Step B, S1 — whole-statement or largest
//! function hashes covering half the unit, with neighbour support), the
//! specificity rule, and agreement with the literal rule.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::format::{LiteralRow, Manifest, UnitRow};
use super::literals::{LiteralMode, code_literals};
use super::units::{StatementPrints, UnitPrint};

/// One indexed occurrence of a unit hash.
#[derive(Clone, Copy, Debug)]
struct UnitHit {
    owner: u32,
    module_bytes: u64,
}

/// A loaded index slice: its unit table and the literal table.
#[derive(Default)]
pub struct LibraryIndex {
    /// sha256 of the manifest (the id a run records); empty when built
    /// from rows.
    pub id: String,
    /// The recipe slice read (`bun-bun-cjs-min`) and why it was chosen;
    /// empty when built from rows.
    pub slice: String,
    pub slice_reason: String,
    packages: Vec<String>,
    package_ids: HashMap<String, u32>,
    units: HashMap<String, Vec<UnitHit>>,
    literal_ids: HashMap<String, u32>,
    /// literal → files holding it.
    postings: Vec<Vec<u32>>,
    /// literal → how many packages hold it.
    package_df: Vec<u32>,
    file_owner: Vec<u32>,
    /// Per file, its literal ids, sorted.
    file_literals: Vec<Vec<u32>>,
    literal_packages: u32,
}

impl LibraryIndex {
    fn package(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.package_ids.get(name) {
            return id;
        }
        let id = self.packages.len() as u32;
        self.packages.push(name.to_string());
        self.package_ids.insert(name.to_string(), id);
        id
    }

    /// An index from its rows (the loader's and the tests' constructor).
    pub fn from_rows(units: Vec<UnitRow>, literals: Vec<LiteralRow>) -> LibraryIndex {
        let mut ix = LibraryIndex::default();
        for r in units {
            let owner = ix.package(&r.owner);
            ix.units.entry(r.hash).or_default().push(UnitHit {
                owner,
                module_bytes: r.module_bytes,
            });
        }
        let mut holders: Vec<BTreeSet<u32>> = Vec::new();
        for r in literals {
            let owner = ix.package(&r.owner);
            let file = ix.file_owner.len() as u32;
            ix.file_owner.push(owner);
            let mut ids: Vec<u32> = r
                .lits
                .iter()
                .map(|l| {
                    let next = ix.literal_ids.len() as u32;
                    *ix.literal_ids.entry(l.clone()).or_insert(next)
                })
                .collect();
            ids.sort_unstable();
            ids.dedup();
            for &id in &ids {
                if id as usize >= ix.postings.len() {
                    ix.postings.resize(id as usize + 1, Vec::new());
                    holders.resize(id as usize + 1, BTreeSet::new());
                }
                ix.postings[id as usize].push(file);
                holders[id as usize].insert(owner);
            }
            ix.file_literals.push(ids);
        }
        ix.package_df = holders.iter().map(|h| h.len() as u32).collect();
        ix.literal_packages = ix.file_owner.iter().collect::<BTreeSet<_>>().len() as u32;
        ix
    }

    /// Load `dir`'s slice for the bundle's `recipe` (`units.<slice>.jsonl`)
    /// and `literals.jsonl`, refusing an index any of whose compatibility
    /// stamps differs from this build's ([`Manifest::check_compatible`]: a
    /// half-matching index would silently half-claim). The slice is chosen
    /// in ONE place, [`super::recipe::choose_slice`]; an index with no
    /// slice for the recipe is [`LoadError::NoSlice`].
    pub fn load(
        dir: &Path,
        recipe: &super::recipe::DetectedRecipe,
    ) -> Result<LibraryIndex, LoadError> {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: {e}"))
        };
        let manifest_text = read("manifest.json")?;
        super::format::check_manifest_stamps(&manifest_text)?;
        let manifest: Manifest =
            serde_json::from_str(&manifest_text).map_err(|e| format!("manifest.json: {e}"))?;
        manifest.check_compatible()?;
        let id = hex_sha256(manifest_text.as_bytes());
        let recipes = manifest.recipes()?;
        let (chosen, reason) =
            super::recipe::choose_slice(&recipes, recipe).map_err(|e| LoadError::NoSlice {
                why: format!("library index {id} has {e}"),
                id: id.clone(),
            })?;
        let slice = chosen.slice_name();
        let rows = |name: &str| -> Result<Vec<String>, String> {
            Ok(read(name)?.lines().map(String::from).collect())
        };
        let parse = |line: &str, name: &str| format!("{name}: {line:.60}");
        let units: Vec<UnitRow> = rows(&format!("units.{slice}.jsonl"))?
            .iter()
            .map(|l| serde_json::from_str(l).map_err(|_| parse(l, "units")))
            .collect::<Result<_, _>>()?;
        let literals: Vec<LiteralRow> = rows("literals.jsonl")?
            .iter()
            .map(|l| serde_json::from_str(l).map_err(|_| parse(l, "literals")))
            .collect::<Result<_, _>>()?;
        let mut ix = LibraryIndex::from_rows(units, literals);
        ix.id = id;
        ix.slice = slice;
        ix.slice_reason = reason.to_string();
        Ok(ix)
    }

    /// Every indexed occurrence of `hash`: (owner, module bytes) — for
    /// diagnostics.
    pub fn occurrences(&self, hash: &str) -> Vec<(&str, u64)> {
        self.units.get(hash).map_or_else(Vec::new, |hits| {
            hits.iter()
                .map(|h| (self.packages[h.owner as usize].as_str(), h.module_bytes))
                .collect()
        })
    }

    /// The distinct owners holding `hash` in a module no bigger than the
    /// specificity rule allows; None when there are none or too many.
    fn look(&self, hash: &str, module_bytes: u64, rules: &LookupRules) -> Option<BTreeSet<u32>> {
        let owners: BTreeSet<u32> = self
            .units
            .get(hash)?
            .iter()
            .filter(|h| {
                rules
                    .specificity
                    .is_none_or(|(factor, slack)| h.module_bytes <= factor * module_bytes + slack)
            })
            .map(|h| h.owner)
            .collect();
        (!owners.is_empty() && owners.len() <= rules.max_packages).then_some(owners)
    }

    fn weight(&self, literal: &str) -> f64 {
        let n = f64::from(self.literal_packages.max(1));
        let w = match self.literal_ids.get(literal) {
            Some(&id) => (n / f64::from(self.package_df[id as usize].max(1))).log2(),
            None => n.log2() + 1.0,
        };
        if is_wordy(literal) {
            w.min(WORDY_CAP)
        } else {
            w
        }
    }

    /// The literal rule on one text: (package, score) when the best file's
    /// package reaches the score and cover thresholds with a margin over
    /// the next package.
    fn literal_claim(&self, text: &str, rules: &LookupRules) -> Option<(u32, f64)> {
        let lits = code_literals(text, LiteralMode::Bundled)?;
        if lits.len() < rules.min_literals {
            return None;
        }
        let mut weighted: Vec<(f64, Option<u32>)> = lits
            .iter()
            .map(|l| (self.weight(l), self.literal_ids.get(l).copied()))
            .collect();
        let total: f64 = weighted.iter().map(|w| w.0).sum();
        // A file reaching the cover, or the margin below a file that does,
        // holds one of the heaviest literals that carry more than the rest.
        weighted.sort_by(|a, b| b.0.total_cmp(&a.0));
        let need = (1.0 - rules.literal_cover) * total + rules.literal_margin;
        let mut candidates: BTreeSet<u32> = BTreeSet::new();
        let mut carried = 0.0;
        for &(w, id) in &weighted {
            if let Some(id) = id {
                candidates.extend(&self.postings[id as usize]);
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
        let mut best: HashMap<u32, f64> = HashMap::new();
        for f in candidates {
            let held = &self.file_literals[f as usize];
            let score: f64 = query
                .iter()
                .filter(|(id, _)| held.binary_search(id).is_ok())
                .map(|&(_, w)| w)
                .sum();
            let e = best.entry(self.file_owner[f as usize]).or_insert(0.0);
            *e = e.max(score);
        }
        let mut ranked: Vec<(u32, f64)> = best.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let (p, score) = *ranked.first()?;
        let runner_up = ranked.get(1).map_or(0.0, |r| r.1);
        (score >= rules.literal_score
            && score >= rules.literal_cover * total
            && score - runner_up >= rules.literal_margin)
            .then_some((p, score))
    }

    /// The structural rule on one unit: (package, nodes covered).
    fn structural(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        previous: Option<u32>,
    ) -> Option<(u32, u32)> {
        let usable =
            |p: &UnitPrint, content: u32| p.nodes >= rules.min_nodes && p.content >= content;
        let whole = &q.prints.whole;
        let mut score: HashMap<u32, u32> = HashMap::new();
        if usable(whole, rules.min_content)
            && let Some(owners) = self.look(&whole.hash, q.module_bytes, rules)
        {
            for p in owners {
                score.insert(p, whole.nodes);
            }
        }
        if score.is_empty() {
            let mut hits: Vec<(&UnitPrint, BTreeSet<u32>)> = q
                .prints
                .functions
                .iter()
                .filter(|f| usable(f, rules.min_function_content))
                .filter_map(|f| Some((f, self.look(&f.hash, q.module_bytes, rules)?)))
                .collect();
            hits.sort_by_key(|(f, _)| std::cmp::Reverse(f.nodes));
            let mut taken: Vec<(u32, u32)> = Vec::new();
            for (f, owners) in hits {
                let (s, e) = (f.span.start, f.span.end);
                if taken.iter().any(|&(a, b)| !(e <= a || s >= b)) {
                    continue;
                }
                taken.push((s, e));
                for p in owners {
                    *score.entry(p).or_default() += f.nodes;
                }
            }
        }
        let best = *score.values().max()?;
        let mut tied: Vec<u32> = score
            .iter()
            .filter(|(_, v)| **v == best)
            .map(|(k, _)| *k)
            .collect();
        tied.sort_by(|a, b| self.packages[*a as usize].cmp(&self.packages[*b as usize]));
        let p = previous.filter(|p| tied.contains(p)).unwrap_or(tied[0]);
        (f64::from(best) >= rules.cover * f64::from(whole.nodes.max(1))).then_some((p, best))
    }
}

/// Why an index was not loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Missing, unreadable or incompatible: the run must stop.
    Unusable(String),
    /// Readable and compatible, but it holds no slice for the bundle's
    /// recipe: a loud WARNING in report mode (the run loses only its
    /// report), a failure for an instrument. `id`: the index's id.
    NoSlice { id: String, why: String },
}

impl From<String> for LoadError {
    fn from(e: String) -> Self {
        LoadError::Unusable(e)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unusable(e) | LoadError::NoSlice { why: e, .. } => f.write_str(e),
        }
    }
}

/// Identifier-like strings and property names: weighed at most
/// [`WORDY_CAP`] bits (the lab's capped vocabulary).
fn is_wordy(literal: &str) -> bool {
    let Some(rest) = literal
        .strip_prefix("S:")
        .or_else(|| literal.strip_prefix("P:"))
    else {
        return false;
    };
    let mut chars = rest.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '-'))
}

const WORDY_CAP: f64 = 2.0;

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The rule's tunables (the lab's Step B and scale-lane values).
#[derive(Clone, Copy, Debug)]
pub struct LookupRules {
    pub min_nodes: u32,
    /// Content tokens a WHOLE statement needs to be looked up.
    pub min_content: u32,
    /// Content tokens a function needs (the lab's Step B: none).
    pub min_function_content: u32,
    pub cover: f64,
    pub max_packages: usize,
    /// A claim under `strong` nodes needs another claim of its package
    /// within this many statement units (0: no support needed).
    pub support: usize,
    pub strong: u32,
    /// Specificity: an index module may be at most `factor` x the query's
    /// module + `slack` bytes.
    pub specificity: Option<(u64, u64)>,
    /// Require the literal rule to agree (same family).
    pub agreement: bool,
    pub min_literals: usize,
    pub literal_score: f64,
    pub literal_cover: f64,
    pub literal_margin: f64,
}

impl Default for LookupRules {
    fn default() -> Self {
        LookupRules {
            min_nodes: 30,
            min_content: 3,
            min_function_content: 0,
            cover: 0.5,
            max_packages: 3,
            support: 10,
            strong: 100,
            specificity: Some((4, 4096)),
            agreement: true,
            min_literals: 6,
            literal_score: 20.0,
            literal_cover: 0.9,
            literal_margin: 10.0,
        }
    }
}

/// One statement unit to look up, in statement order.
pub struct UnitQuery<'q> {
    pub unit: u32,
    pub prints: &'q StatementPrints,
    /// The bytes of the module the unit sits in (specificity).
    pub module_bytes: u64,
    /// The unit's text (the literal rule reads it).
    pub text: &'q str,
}

/// One claim.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitClaim {
    pub unit: u32,
    pub package: String,
    /// Nodes the structural hits covered.
    pub structural: u32,
    /// The literal rule's package and score, when it was consulted.
    pub literal: Option<(String, f64)>,
}

/// The claims of `queries` (statement units, in statement order) under
/// `rules`; `family` decides agreement.
pub fn claim_units(
    index: &LibraryIndex,
    queries: &[UnitQuery<'_>],
    rules: LookupRules,
    family: &dyn Fn(&str) -> String,
) -> Vec<UnitClaim> {
    let mut previous = None;
    let structural: Vec<Option<(u32, u32)>> = queries
        .iter()
        .map(|q| {
            let hit = index.structural(q, &rules, previous);
            if let Some((p, _)) = hit {
                previous = Some(p);
            }
            hit
        })
        .collect();
    let supported = |i: usize, p: u32| {
        let lo = i.saturating_sub(rules.support);
        let hi = (i + rules.support + 1).min(structural.len());
        (lo..hi).any(|j| j != i && structural[j].is_some_and(|(q, _)| q == p))
    };
    let mut out = Vec::new();
    for (i, q) in queries.iter().enumerate() {
        let Some((p, covered)) = structural[i] else {
            continue;
        };
        if rules.support > 0 && covered < rules.strong && !supported(i, p) {
            continue;
        }
        let package = &index.packages[p as usize];
        let mut claim = UnitClaim {
            unit: q.unit,
            package: package.clone(),
            structural: covered,
            literal: None,
        };
        if rules.agreement {
            let Some((l, score)) = index.literal_claim(q.text, &rules) else {
                continue;
            };
            let named = &index.packages[l as usize];
            if family(named) != family(package) {
                continue;
            }
            claim.package = named.clone();
            claim.literal = Some((named.clone(), score));
        }
        out.push(claim);
    }
    out
}
