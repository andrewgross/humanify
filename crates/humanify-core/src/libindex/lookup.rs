//! Looking statement units up in a library index (exp095 library
//! discovery's index seeds; docs/design/library-discovery.md §5): the
//! structural rule (the lab's Step B, S1 — whole-statement or largest
//! function hashes covering half the unit, with neighbour support), the
//! specificity rule, and agreement with the literal rule.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::format::{LiteralRow, Manifest, UnitRow};
use super::literals::{LiteralMode, code_literals};
use super::table::{self, LiteralTable, Source, UnitTable};
use super::units::{StatementPrints, UnitPrint};
use crate::matching::store::ReferenceStore;

/// One indexed occurrence of a unit hash: the index store's piece
/// (`matching::store`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitHit {
    /// The owner package's id (`LibraryIndex::package_name`).
    pub owner: u32,
    /// The smallest bundled module the unit was seen in (specificity).
    pub module_bytes: u64,
}

/// An opened index slice: its unit table and the literal table, read on
/// demand ([`super::table`]: nothing is parsed into a map on load).
pub struct LibraryIndex {
    /// sha256 of the manifest (the id a run records); empty when built
    /// from rows.
    pub id: String,
    /// The recipe slice read (`bun-bun-cjs-min`) and why it was chosen;
    /// empty when built from rows.
    pub slice: String,
    pub slice_reason: String,
    /// Every owner package, sorted: an owner id is its position.
    packages: Vec<String>,
    units: UnitTable,
    literals: LiteralTable,
}

impl LibraryIndex {
    /// An index from its rows, through the same tables a run reads (the
    /// tests' constructor).
    pub fn from_rows(units: Vec<UnitRow>, literals: Vec<LiteralRow>) -> LibraryIndex {
        let packages = table::package_table(&[&units], &literals);
        let units = UnitTable::encode(&units, &packages).expect("unit hashes are unit keys");
        LibraryIndex {
            id: String::new(),
            slice: String::new(),
            slice_reason: String::new(),
            units: UnitTable::open(Source::Memory(units)).expect("an encoded unit table opens"),
            literals: LiteralTable::open(Source::Memory(LiteralTable::encode(
                &literals, &packages,
            )))
            .expect("an encoded literal table opens"),
            packages,
        }
    }

    /// Open `dir`'s slice for the bundle's `recipe` (`units.<slice>.bin`)
    /// and the literal table, refusing an index any of whose compatibility
    /// stamps differs from this build's ([`Manifest::check_compatible`]: a
    /// half-matching index would silently half-claim). The slice is chosen
    /// in ONE place, [`super::recipe::choose_slice`]; an index with no
    /// slice for the recipe is [`LoadError::NoSlice`]. Only the tables'
    /// headers and fences are read here.
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
        let packages: Vec<String> = serde_json::from_str(&read(table::PACKAGES_FILE)?)
            .map_err(|e| format!("{}: {e}", table::PACKAGES_FILE))?;
        let open = |name: &str| Source::open(&dir.join(name)).map_err(|e| format!("{name}: {e}"));
        let units_name = table::units_file(&slice);
        let units =
            UnitTable::open(open(&units_name)?).map_err(|e| format!("{units_name}: {e}"))?;
        let literals = LiteralTable::open(open(table::LITERALS_FILE)?)
            .map_err(|e| format!("{}: {e}", table::LITERALS_FILE))?;
        Ok(LibraryIndex {
            id,
            slice,
            slice_reason: reason.to_string(),
            packages,
            units,
            literals,
        })
    }

    /// Every indexed occurrence of `hash`, in row order (the store's
    /// `candidates`).
    pub fn unit_hits(&self, hash: &str) -> Vec<UnitHit> {
        self.units.hits(hash)
    }

    /// An owner id's package name.
    pub fn package_name(&self, owner: u32) -> &str {
        &self.packages[owner as usize]
    }

    /// Every owner package, by owner id.
    pub fn package_names(&self) -> &[String] {
        &self.packages
    }

    /// Every indexed occurrence of `hash`: (owner, module bytes) — for
    /// diagnostics.
    pub fn occurrences(&self, hash: &str) -> Vec<(&str, u64)> {
        self.unit_hits(hash)
            .iter()
            .map(|h| (self.packages[h.owner as usize].as_str(), h.module_bytes))
            .collect()
    }

    /// The distinct owners holding `hash` in a module no bigger than the
    /// specificity rule allows; None when there are none or too many.
    pub(crate) fn look(
        &self,
        hash: &str,
        module_bytes: u64,
        rules: &LookupRules,
    ) -> Option<BTreeSet<u32>> {
        let owners: BTreeSet<u32> = ReferenceStore::candidates(self, hash)
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

    /// A query literal's weight in bits, by its id when the index holds
    /// it.
    fn weight(&self, literal: &str, id: Option<u32>) -> f64 {
        let n = f64::from(self.literals.literal_packages().max(1));
        let w = match id {
            Some(id) => (n / f64::from(self.literals.package_df(id).max(1))).log2(),
            None => n.log2() + 1.0,
        };
        if is_wordy(literal) {
            w.min(WORDY_CAP)
        } else {
            w
        }
    }

    /// Per LONG literal of `text` (a value over the extractor's length
    /// cap, kept as a hash: `S#`, `P#`, `R#`) that the literal table holds,
    /// the owners of the files holding it. A long literal is identity
    /// evidence: it does not recur by chance (a generated table, a long
    /// message). None when the text does not parse.
    pub(crate) fn long_literal_owners(&self, text: &str) -> Option<Vec<BTreeSet<u32>>> {
        let lits = code_literals(text, LiteralMode::Bundled)?;
        Some(
            lits.iter()
                .filter(|l| l.as_bytes().get(1) == Some(&b'#'))
                .filter_map(|l| self.literals.resolve(l))
                .map(|id| {
                    self.literals
                        .postings(id)
                        .into_iter()
                        .map(|f| self.literals.file_owner(f))
                        .collect()
                })
                .collect(),
        )
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
            .map(|l| {
                let id = self.literals.resolve(l);
                (self.weight(l, id), id)
            })
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
                candidates.extend(self.literals.postings(id));
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
            let held = self.literals.file_literals(f);
            let score: f64 = query
                .iter()
                .filter(|(id, _)| held.binary_search(id).is_ok())
                .map(|&(_, w)| w)
                .sum();
            let e = best.entry(self.literals.file_owner(f)).or_insert(0.0);
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
        let score = self.structural_scores(q, rules);
        let whole = &q.prints.whole;
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

    /// Every package the structural rule would accept for `q` on its own
    /// (its covered nodes reach the cover), tie or not: the index's
    /// ambiguity about the unit.
    pub(crate) fn structural_owners(&self, q: &UnitQuery<'_>, rules: &LookupRules) -> Vec<u32> {
        let floor = rules.cover * f64::from(q.prints.whole.nodes.max(1));
        let mut owners: Vec<u32> = self
            .structural_scores(q, rules)
            .into_iter()
            .filter(|&(_, v)| f64::from(v) >= floor)
            .map(|(k, _)| k)
            .collect();
        owners.sort_unstable();
        owners
    }

    /// The structural rule's scores: per package, the nodes its hits cover
    /// (the whole print, else the largest non-overlapping functions).
    fn structural_scores(&self, q: &UnitQuery<'_>, rules: &LookupRules) -> HashMap<u32, u32> {
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
        score
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
