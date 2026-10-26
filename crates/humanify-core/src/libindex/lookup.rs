//! Looking statement units up in a library index (exp095 library
//! discovery's index seeds; docs/design/library-discovery.md §5): the
//! structural rule (the lab's Step B, S1 — whole-statement or largest
//! function hashes covering half the unit, with neighbour support), the
//! specificity rule, and agreement with the literal rule.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::format::{EdgeRow, HelperRow, LiteralRow, Manifest, UnitRow};
use super::literals::{LiteralMode, code_literals};
use super::table::{
    self, CalleePrintTable, EdgeCallee, EdgeTable, LiteralTable, Source, UnitTable,
};
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
    /// The row's position in its unit table (the edge table's key).
    pub row: u32,
    /// Which unit table: 0 the chosen slice's, `i` the `i`-th terser-built
    /// slice read beside it.
    pub table: u8,
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
    slice_reason: String,
    /// Every owner package, sorted: an owner id is its position.
    packages: Vec<String>,
    units: UnitTable,
    /// The terser-built slices read beside `units` (exp127,
    /// [`super::recipe::TerserSlices`]); empty when off.
    terser_units: Vec<UnitTable>,
    terser_reading: super::recipe::TerserSlices,
    /// `auto`'s verdict for the bundle: (read the terser slices, share),
    /// set once by [`LibraryIndex::decide_terser_slices`].
    terser_verdict: std::sync::OnceLock<(bool, f64)>,
    literals: LiteralTable,
    /// The edge tables (format `/4`), parallel to the unit tables (chosen
    /// slice first, then the terser-built ones); None for a `/3` index,
    /// which records no edges.
    edges: Option<Vec<EdgeTable>>,
    /// The flat callee-print tables (exp155, `edges/2`), parallel to
    /// `edges`: every callee print any edge row records — INSIDE entries
    /// under the row's owner, OUT-OF under the callee's own — including
    /// prints with no unit row of their own. None for a `/3` index.
    callee_prints: Option<Vec<CalleePrintTable>>,
    /// Every package name the index knows: its owners, and (loaded) the
    /// packages its manifest records registry metadata or builds for.
    known: BTreeSet<String>,
    /// The helper table (exp129): a helper source's print → the helper
    /// packages holding it; empty for an index whose manifest lists none.
    helpers: BTreeMap<String, Vec<String>>,
}

impl LibraryIndex {
    /// An index from its rows, through the same tables a run reads (the
    /// tests' constructor).
    pub fn from_rows(units: Vec<UnitRow>, literals: Vec<LiteralRow>) -> LibraryIndex {
        Self::rows(units, None, literals)
    }

    /// [`LibraryIndex::from_rows`] with an edge table (a `/4` index; the
    /// tests' constructor).
    pub fn from_rows_with_edges(
        units: Vec<UnitRow>,
        edges: Vec<EdgeRow>,
        literals: Vec<LiteralRow>,
    ) -> LibraryIndex {
        Self::rows(units, Some(edges), literals)
    }

    fn rows(
        units: Vec<UnitRow>,
        edges: Option<Vec<EdgeRow>>,
        literals: Vec<LiteralRow>,
    ) -> LibraryIndex {
        let edge_rows: &[EdgeRow] = edges.as_deref().unwrap_or(&[]);
        let packages = table::package_table(&[&units], &[edge_rows], &literals);
        // the flat callee-print table is derived from the same rows a
        // merge writes beside the edge table (`table::lookup_files`)
        let callee_prints = edges.as_ref().map(|e| {
            let bytes = CalleePrintTable::encode(e, &packages).expect("callee prints encode");
            vec![CalleePrintTable::open(Source::Memory(bytes)).expect("an encoded table opens")]
        });
        let edges = edges.map(|e| {
            let bytes = EdgeTable::encode(&units, &e, &packages).expect("edge rows encode");
            vec![EdgeTable::open(Source::Memory(bytes)).expect("an encoded edge table opens")]
        });
        let units = UnitTable::encode(&units, &packages).expect("unit hashes are unit keys");
        LibraryIndex {
            id: String::new(),
            slice: String::new(),
            slice_reason: String::new(),
            units: UnitTable::open(Source::Memory(units)).expect("an encoded unit table opens"),
            terser_units: Vec::new(),
            terser_reading: super::recipe::TerserSlices::Off,
            terser_verdict: std::sync::OnceLock::new(),
            literals: LiteralTable::open(Source::Memory(LiteralTable::encode(
                &literals, &packages,
            )))
            .expect("an encoded literal table opens"),
            edges,
            callee_prints,
            known: packages.iter().cloned().collect(),
            packages,
            helpers: BTreeMap::new(),
        }
    }

    /// This index with `rows` as its helper table (the tests' constructor;
    /// rows of one hash are joined, owners sorted).
    pub fn with_helpers(mut self, rows: Vec<HelperRow>) -> LibraryIndex {
        self.helpers = helper_table(rows);
        self
    }

    /// The helper packages whose own build holds `hash` (exp129: canonical
    /// transpiler helper code); empty when none does.
    pub fn helper_owners(&self, hash: &str) -> &[String] {
        self.helpers.get(hash).map_or(&[], Vec::as_slice)
    }

    /// Why the slice was chosen, and which slices are read beside it (with
    /// `auto`, the bundle's verdict once made).
    pub fn slice_reason(&self) -> String {
        match self.terser_verdict.get() {
            Some(&(on, share)) => format!(
                "{}; auto verdict: terser-only share {share:.4} -> {}",
                self.slice_reason,
                if on { "union" } else { "off" }
            ),
            None => self.slice_reason.clone(),
        }
    }

    /// `terser_slices = "auto"`'s verdict for the bundle `prints` are the
    /// units of: over the whole-unit prints the lookup reads (`min_nodes`,
    /// `min_content`) that some slice read holds, the share of nodes the
    /// terser-built slices ALONE hold. At or over the configured share the
    /// bundle reads the union from here on, else the chosen slice alone.
    /// Made once (a second call returns the first share); 0 when the
    /// index reads no terser-built slice. Any other mode: the share only.
    pub fn decide_terser_slices(
        &self,
        prints: &[Option<StatementPrints>],
        rules: &LookupRules,
    ) -> f64 {
        if let Some(&(_, share)) = self.terser_verdict.get() {
            return share;
        }
        if self.terser_units.is_empty() {
            return 0.0;
        }
        let (mut own, mut terser_only) = (0u64, 0u64);
        for p in prints.iter().flatten() {
            let w = &p.whole;
            if w.nodes < rules.min_nodes || w.content < rules.min_content {
                continue;
            }
            if !self.units.hits(&w.hash).is_empty() {
                own += u64::from(w.nodes);
            } else if self
                .terser_units
                .iter()
                .any(|t| !t.hits(&w.hash).is_empty())
            {
                terser_only += u64::from(w.nodes);
            }
        }
        let share = if own + terser_only == 0 {
            0.0
        } else {
            terser_only as f64 / (own + terser_only) as f64
        };
        if let super::recipe::TerserSlices::Auto { share: floor } = self.terser_reading {
            let _ = self.terser_verdict.set((share >= floor, share));
        }
        share
    }

    /// Does the index know package `name` (an owner, or a package its
    /// manifest records)?
    pub fn knows(&self, name: &str) -> bool {
        self.known.contains(name)
    }

    /// Every package name the index knows.
    pub fn known_names(&self) -> &BTreeSet<String> {
        &self.known
    }

    /// `name` as a package the index knows, in npm's spelling: itself, its
    /// lower case, or the ONE known scoped package `@s/n` it spells as
    /// `s-n` (a banner's `noble-curves` is `@noble/curves`). None when no
    /// known package, or more than one, answers to it.
    pub fn resolve_known(&self, name: &str) -> Option<String> {
        if self.known.contains(name) {
            return Some(name.to_string());
        }
        let lower = name.to_ascii_lowercase();
        if self.known.contains(&lower) {
            return Some(lower);
        }
        let mut hits = self.known.iter().filter(|k| {
            k.strip_prefix('@')
                .and_then(|rest| rest.split_once('/'))
                .is_some_and(|(scope, base)| {
                    lower.len() == scope.len() + 1 + base.len()
                        && lower.starts_with(scope)
                        && lower.as_bytes()[scope.len()] == b'-'
                        && lower.ends_with(base)
                })
        });
        let first = hits.next()?;
        hits.next().is_none().then(|| first.clone())
    }

    /// Open `dir`'s slice for the bundle's `recipe` (`units.<slice>.bin`)
    /// and the literal table, refusing an index any of whose compatibility
    /// stamps differs from this build's ([`Manifest::check_compatible`]: a
    /// half-matching index would silently half-claim). The slice is chosen
    /// in ONE place, [`super::recipe::choose_slice`]; an index with no
    /// slice for the recipe is [`LoadError::NoSlice`]. Only the tables'
    /// headers and fences are read here. `terser` names the slices read
    /// beside the chosen one ([`super::recipe::terser_slices`]).
    pub fn load(
        dir: &Path,
        recipe: &super::recipe::DetectedRecipe,
        terser: super::recipe::TerserSlices,
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
        let open_edges = |slice: &str| {
            let file = table::edges_file(slice);
            EdgeTable::open(open(&file)?).map_err(|e| format!("{file}: {e}"))
        };
        // exp155: every `/4` index this binary accepts (`edges/2`) writes
        // the flat callee-print table beside the edge table — a missing
        // file is a refusal, not an empty table: a rule that reads it
        // must never mistake a broken index for recorded silence
        let open_callee_prints = |slice: &str| {
            let file = table::callee_prints_file(slice);
            CalleePrintTable::open(open(&file)?).map_err(|e| format!("{file}: {e}"))
        };
        let mut edges = manifest
            .has_edges()
            .then(|| open_edges(&slice).map(|t| vec![t]))
            .transpose()?;
        let mut callee_prints = manifest
            .has_edges()
            .then(|| open_callee_prints(&slice).map(|t| vec![t]))
            .transpose()?;
        let mut reason = reason.to_string();
        let mut terser_units = Vec::new();
        if terser != super::recipe::TerserSlices::Off {
            let extra = super::recipe::terser_slices(&recipes, chosen);
            let names: Vec<String> = extra.iter().map(|r| r.slice_name()).collect();
            for name in &names {
                let file = table::units_file(name);
                terser_units
                    .push(UnitTable::open(open(&file)?).map_err(|e| format!("{file}: {e}"))?);
                if let Some(tables) = edges.as_mut() {
                    tables.push(open_edges(name)?);
                }
                if let Some(tables) = callee_prints.as_mut() {
                    tables.push(open_callee_prints(name)?);
                }
            }
            reason.push_str(&format!(
                "; terser_slices = {}: also {}",
                terser.name(),
                if names.is_empty() {
                    "none (the index has no terser-built slice)".to_string()
                } else {
                    names.join(", ")
                }
            ));
        }
        let literals = LiteralTable::open(open(table::LITERALS_FILE)?)
            .map_err(|e| format!("{}: {e}", table::LITERALS_FILE))?;
        let helpers = if manifest.tables.contains_key(table::HELPERS_FILE) {
            let text = read(table::HELPERS_FILE)?;
            let rows = text
                .lines()
                .map(serde_json::from_str::<HelperRow>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("{}: {e}", table::HELPERS_FILE))?;
            helper_table(rows)
        } else {
            BTreeMap::new()
        };
        let mut known: BTreeSet<String> = packages.iter().cloned().collect();
        known.extend(manifest.registry.packuments.keys().cloned());
        known.extend(manifest.packages.iter().map(|p| p.name.clone()));
        Ok(LibraryIndex {
            id,
            slice,
            slice_reason: reason,
            known,
            packages,
            units,
            terser_units,
            terser_reading: terser,
            terser_verdict: std::sync::OnceLock::new(),
            literals,
            edges,
            callee_prints,
            helpers,
        })
    }

    /// Every indexed occurrence of `hash`, in row order (the store's
    /// `candidates`).
    pub fn unit_hits(&self, hash: &str) -> Vec<UnitHit> {
        use super::recipe::TerserSlices;
        let hits = self.units.hits(hash);
        let read_terser = match self.terser_reading {
            TerserSlices::Off => false,
            TerserSlices::Union => true,
            TerserSlices::Fallback => hits.is_empty(),
            TerserSlices::Auto { .. } => self.terser_verdict.get().is_some_and(|v| v.0),
        };
        if !read_terser || self.terser_units.is_empty() {
            return hits;
        }
        // each owner once, where it is first seen, at its smallest module
        let mut merged: Vec<UnitHit> = Vec::with_capacity(hits.len());
        let terser = self.terser_units.iter().enumerate().flat_map(|(i, t)| {
            t.hits(hash).into_iter().map(move |h| UnitHit {
                table: i as u8 + 1,
                ..h
            })
        });
        for h in hits.into_iter().chain(terser) {
            match merged.iter_mut().find(|m| m.owner == h.owner) {
                Some(m) => m.module_bytes = m.module_bytes.min(h.module_bytes),
                None => merged.push(h),
            }
        }
        merged
    }

    /// Does the index record edges (format `/4`)?
    pub fn has_edges(&self) -> bool {
        self.edges.is_some()
    }

    /// Does the index hold the flat callee-print table (`edges/2`, exp155)
    /// to answer [`LibraryIndex::callee_recorders`] with? False for a `/3`
    /// index — whose silence is no evidence, so a caller gating on this
    /// must stay silent in turn.
    pub fn has_callee_prints(&self) -> bool {
        self.callee_prints.is_some()
    }

    /// The owners that RECORD `key` as a callee (exp155's flat table):
    /// INSIDE entries credit the recording row's owner, OUT-OF entries the
    /// callee's own. A print under the row floor — with no unit row of its
    /// own — still answers here; that is the whole point: this is the
    /// query that tells "the index never saw this print" from "it did,
    /// under another family". Sorted owner ids; empty when nothing
    /// records it or the index holds no table.
    pub fn callee_recorders(&self, key: u64) -> Vec<u32> {
        use super::recipe::TerserSlices;
        let Some(tables) = self.callee_prints.as_ref() else {
            return Vec::new();
        };
        let read_terser = match self.terser_reading {
            TerserSlices::Off => false,
            TerserSlices::Union => true,
            // a print the chosen slice does not record may be recorded by a
            // terser-built one — the same fallback the unit lookup reads
            TerserSlices::Fallback => tables.first().is_none_or(|t| t.recorders(key).is_empty()),
            TerserSlices::Auto { .. } => self.terser_verdict.get().is_some_and(|v| v.0),
        };
        let mut owners: Vec<u32> = tables.first().map_or_else(Vec::new, |t| t.recorders(key));
        if read_terser {
            for t in &tables[1..] {
                owners.extend(t.recorders(key));
            }
        }
        owners.sort_unstable();
        owners.dedup();
        owners
    }

    /// The callees a unit hit's row records, sorted by (owner, print key);
    /// None when the index records no edges (`/3`).
    pub fn row_callees(&self, hit: UnitHit) -> Option<Vec<EdgeCallee>> {
        let tables = self.edges.as_ref()?;
        Some(
            tables
                .get(usize::from(hit.table))
                .map_or_else(Vec::new, |t| t.callees(hit.row)),
        )
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
    /// specificity rule allows; None when there are none or too many — too
    /// many PACKAGES, or with [`LookupRules::max_packages_by_family`] too
    /// many families under `same`.
    pub(crate) fn look(
        &self,
        hash: &str,
        module_bytes: u64,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
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
        let many = if rules.max_packages_by_family {
            self.families_exceed(&owners, rules.max_packages, same)
        } else {
            owners.len() > rules.max_packages
        };
        (!owners.is_empty() && !many).then_some(owners)
    }

    /// The call-context relaxers (exp153, [`CallContextRules::relax_floor`],
    /// [`CallContextRules::relax_common`]): the owners of `q`'s WHOLE print
    /// its call context confirms, when the plain lookup refused the print
    /// for its size (at or over the relaxed floor) or for too many owners,
    /// and the confirmed ones are within the package cap. None otherwise.
    fn look_confirmed(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> Option<BTreeSet<u32>> {
        let cc = rules.call_context;
        let whole = &q.prints.whole;
        if whole.content < rules.min_content {
            return None;
        }
        let small = whole.nodes < rules.min_nodes;
        if small && (cc.relax_floor == 0 || whole.nodes < cc.relax_floor) {
            return None;
        }
        if !small && !cc.relax_common {
            return None;
        }
        let owners: BTreeSet<u32> = ReferenceStore::candidates(self, &whole.hash)
            .iter()
            .filter(|h| {
                rules
                    .specificity
                    .is_none_or(|(factor, slack)| h.module_bytes <= factor * q.module_bytes + slack)
            })
            .map(|h| h.owner)
            .collect();
        let many = |o: &BTreeSet<u32>| {
            if rules.max_packages_by_family {
                self.families_exceed(o, rules.max_packages, same)
            } else {
                o.len() > rules.max_packages
            }
        };
        if owners.is_empty() || (!small && !many(&owners)) {
            return None;
        }
        if small && many(&owners) && !cc.relax_common {
            return None;
        }
        let confirmed: BTreeSet<u32> = owners
            .into_iter()
            .filter(|&o| self.call_confirms(q, rules, &|x| x == o))
            .collect();
        (!confirmed.is_empty() && !many(&confirmed)).then_some(confirmed)
    }

    /// Do `owners` span more than `max` families under `same`? Stops at the
    /// first family past `max`, so a print every package holds costs at
    /// most `max + 1` comparisons per owner.
    fn families_exceed(
        &self,
        owners: &BTreeSet<u32>,
        max: usize,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> bool {
        let mut reps: Vec<&str> = Vec::new();
        for &o in owners {
            let name = self.package_name(o);
            if reps.iter().any(|r| same(r, name)) {
                continue;
            }
            reps.push(name);
            if reps.len() > max {
                return true;
            }
        }
        false
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
    fn literal_claim(
        &self,
        text: &str,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> Option<(u32, f64)> {
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
        let runner_up = if rules.literal_margin_by_family {
            // The best OTHER family's file: a sibling package (a fork, a
            // monorepo member, one package's two names) holding the same
            // literals is no rival — the more packages an index holds, the
            // likelier such a tie.
            let named = self.package_name(p);
            ranked
                .iter()
                .skip(1)
                .find(|r| !same(self.package_name(r.0), named))
                .map_or(0.0, |r| r.1)
        } else {
            ranked.get(1).map_or(0.0, |r| r.1)
        };
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
        same: &dyn Fn(&str, &str) -> bool,
    ) -> Option<(u32, u32, bool)> {
        let (score, relaxed) = self.structural_scores_relaxed(q, rules, same);
        let whole = &q.prints.whole;
        let best = *score.values().max()?;
        let mut tied: Vec<u32> = score
            .iter()
            .filter(|(_, v)| **v == best)
            .map(|(k, _)| *k)
            .collect();
        tied.sort_by(|a, b| self.packages[*a as usize].cmp(&self.packages[*b as usize]));
        if rules.call_context.tie_break && tied.len() > 1 {
            // exp153: the one tied package whose own rows record every
            // callee of the unit
            let confirmed: Vec<u32> = tied
                .iter()
                .copied()
                .filter(|&p| self.call_confirms(q, rules, &|o| o == p))
                .collect();
            if confirmed.len() == 1 {
                tied = confirmed;
            }
        }
        let p = previous.filter(|p| tied.contains(p)).unwrap_or(tied[0]);
        (f64::from(best) >= rules.cover * f64::from(whole.nodes.max(1)))
            .then_some((p, best, relaxed))
    }

    /// Does `q`'s call context confirm the owners `accept` takes (every
    /// counted callee of the unit is one their rows of its print record;
    /// [`super::callctx`])?
    pub(crate) fn call_confirms(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        accept: &dyn Fn(u32) -> bool,
    ) -> bool {
        self.call_fit(q.prints, q.callees, rules.call_context.callees, accept)
            .is_some_and(super::callctx::Fit::confirms)
    }

    /// Every package the structural rule would accept for `q` on its own
    /// (its covered nodes reach the cover), tie or not: the index's
    /// ambiguity about the unit.
    pub(crate) fn structural_owners(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> Vec<u32> {
        let floor = rules.cover * f64::from(q.prints.whole.nodes.max(1));
        let mut owners: Vec<u32> = self
            .structural_scores(q, rules, same)
            .into_iter()
            .filter(|&(_, v)| f64::from(v) >= floor)
            .map(|(k, _)| k)
            .collect();
        owners.sort_unstable();
        owners
    }

    /// [`Self::structural_scores`] for an instrument (the call-context
    /// census).
    pub(crate) fn structural_scores_for(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> HashMap<u32, u32> {
        self.structural_scores(q, rules, same)
    }

    /// The structural rule's scores: per package, the nodes its hits cover
    /// (the whole print, else the largest non-overlapping functions).
    fn structural_scores(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> HashMap<u32, u32> {
        self.structural_scores_relaxed(q, rules, same).0
    }

    /// [`Self::structural_scores`], and whether the scores came from a
    /// call-context relaxer ([`Self::look_confirmed`]).
    fn structural_scores_relaxed(
        &self,
        q: &UnitQuery<'_>,
        rules: &LookupRules,
        same: &dyn Fn(&str, &str) -> bool,
    ) -> (HashMap<u32, u32>, bool) {
        let usable =
            |p: &UnitPrint, content: u32| p.nodes >= rules.min_nodes && p.content >= content;
        let whole = &q.prints.whole;
        let mut score: HashMap<u32, u32> = HashMap::new();
        if usable(whole, rules.min_content)
            && let Some(owners) = self.look(&whole.hash, q.module_bytes, rules, same)
        {
            for p in owners {
                score.insert(p, whole.nodes);
            }
        }
        if score.is_empty()
            && let Some(owners) = self.look_confirmed(q, rules, same)
        {
            for p in owners {
                score.insert(p, whole.nodes);
            }
            return (score, true);
        }
        if score.is_empty() {
            let mut hits: Vec<(&UnitPrint, BTreeSet<u32>)> = q
                .prints
                .functions
                .iter()
                .filter(|f| usable(f, rules.min_function_content))
                .filter_map(|f| Some((f, self.look(&f.hash, q.module_bytes, rules, same)?)))
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
        (score, false)
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
    /// Read the literal margin against the best file of another package
    /// FAMILY, not merely another package (exp106; default off).
    pub literal_margin_by_family: bool,
    /// Count a print's owners by FAMILY, not package, against
    /// `max_packages` (exp125; default off): a scope's members, a fork and
    /// its origin are one identity, and a bigger index adds carriers of the
    /// same code until a print every one of them holds says nothing.
    pub max_packages_by_family: bool,
    /// The call-context rule (exp153; default off): what a unit's own
    /// callees, read against the index's recorded ones, may decide.
    pub call_context: CallContextRules,
}

/// How call context is used ([`super::callctx`]; exp153). Every use is
/// off by default. A unit's call context CONFIRMS a package when it has at
/// least one counted callee ([`CallContextRules::callees`]) and every one
/// is a callee the package's index rows holding the unit's print record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CallContextRules {
    /// The position window's support ([`LookupRules::support`]).
    pub support: CallSupport,
    /// The isolation veto's neighbour test (discovery).
    pub veto: CallSupport,
    /// A structural tie between packages goes to the one tied package
    /// whose call context confirms it.
    pub tie_break: bool,
    /// A whole print under `min_nodes` but at or over this floor is looked
    /// up, for the owners whose call context confirms it (0: off).
    pub relax_floor: u32,
    /// A print more than `max_packages` owners hold is narrowed to the
    /// owners whose call context confirms it.
    pub relax_common: bool,
    /// Which callees count.
    pub callees: super::callctx::CalleeFloor,
}

impl CallContextRules {
    /// The named settings the config key `library.lookup.call_context`
    /// chooses from, in this order ("off" first: the default).
    pub const NAMES: [&'static str; 11] = [
        "off",
        "tie-break",
        "support",
        "support-or",
        "veto",
        "veto-or",
        "relax-floor",
        "relax-common",
        "relax",
        "tie-break+relax",
        "tie-break+support-or+relax",
    ];

    /// The setting a name stands for (`callees` as given).
    pub fn from_name(name: &str, callees: super::callctx::CalleeFloor) -> Option<CallContextRules> {
        let off = CallContextRules {
            callees,
            ..CallContextRules::default()
        };
        let relax = CallContextRules {
            relax_floor: super::units::UNIT_ROW_MIN_NODES,
            relax_common: true,
            ..off
        };
        Some(match name {
            "off" => off,
            "tie-break" => CallContextRules {
                tie_break: true,
                ..off
            },
            "support" => CallContextRules {
                support: CallSupport::Replace,
                veto: CallSupport::Replace,
                ..off
            },
            "support-or" => CallContextRules {
                support: CallSupport::Or,
                veto: CallSupport::Or,
                ..off
            },
            "veto" => CallContextRules {
                veto: CallSupport::Replace,
                ..off
            },
            "veto-or" => CallContextRules {
                veto: CallSupport::Or,
                ..off
            },
            "relax-floor" => CallContextRules {
                relax_floor: super::units::UNIT_ROW_MIN_NODES,
                ..off
            },
            "relax-common" => CallContextRules {
                relax_common: true,
                ..off
            },
            "relax" => relax,
            "tie-break+relax" => CallContextRules {
                tie_break: true,
                ..relax
            },
            "tie-break+support-or+relax" => {
                let both = CallContextRules {
                    support: CallSupport::Or,
                    veto: CallSupport::Or,
                    ..relax
                };
                CallContextRules {
                    tie_break: true,
                    ..both
                }
            }
            _ => return None,
        })
    }

    /// This setting's name (None: not one of [`Self::NAMES`]).
    pub fn name(&self) -> Option<&'static str> {
        Self::NAMES
            .into_iter()
            .find(|n| Self::from_name(n, self.callees).as_ref() == Some(self))
    }

    /// Is any use on?
    pub fn is_on(&self) -> bool {
        self.support != CallSupport::Off
            || self.veto != CallSupport::Off
            || self.tie_break
            || self.relax_floor > 0
            || self.relax_common
    }
}

/// How call context stands beside a support test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CallSupport {
    /// Not read: the test as it is.
    #[default]
    Off,
    /// The test passes, or the call context confirms.
    Or,
    /// The call context confirms (the test is not read).
    Replace,
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
            literal_margin_by_family: false,
            max_packages_by_family: false,
            call_context: CallContextRules::default(),
        }
    }
}

impl LookupRules {
    /// The SHIPPED rules: what `[library.lookup]` defaults to (exp109, from
    /// experiments/106-matcher-hygiene's measured recommendation) — the
    /// structural rule, the literal margin read by family. `Default` stays
    /// the engine's baseline (the agreement rule, margin by package), which
    /// the unit tests compare each rule against.
    pub fn recommended() -> Self {
        LookupRules {
            agreement: false,
            literal_margin_by_family: true,
            ..LookupRules::default()
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
    /// The unit's own callees in the bundle (the call-context rule reads
    /// them; [`super::callctx::bundle_callees`]).
    pub callees: &'q [super::callctx::Callee],
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

/// The structural rule's pick for one query, before literal agreement:
/// the package, the nodes its hits cover, and whether the claim stands on
/// its own or has the support it needs ([`LookupRules::support`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    pub package: u32,
    pub covered: u32,
    pub supported: bool,
}

/// The structural rule's picks for `queries` (statement units, in
/// statement order) under `rules`: what [`claim_units`] claims from,
/// before the literal rule.
pub fn picks(
    index: &LibraryIndex,
    queries: &[UnitQuery<'_>],
    rules: &LookupRules,
    same: &dyn Fn(&str, &str) -> bool,
) -> Vec<Option<Pick>> {
    let mut previous = None;
    let structural: Vec<Option<(u32, u32, bool)>> = queries
        .iter()
        .map(|q| {
            let hit = index.structural(q, rules, previous, same);
            if let Some((p, _, _)) = hit {
                previous = Some(p);
            }
            hit
        })
        .collect();
    let supported = |i: usize, p: u32| {
        let lo = i.saturating_sub(rules.support);
        let hi = (i + rules.support + 1).min(structural.len());
        (lo..hi).any(|j| j != i && structural[j].is_some_and(|(q, _, _)| q == p))
    };
    let calls = |i: usize, p: u32| {
        let named = index.package_name(p);
        index.call_confirms(&queries[i], rules, &|o| same(index.package_name(o), named))
    };
    structural
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (package, covered, relaxed) = (*s)?;
            let window = || rules.support == 0 || supported(i, package);
            // a relaxed pick stands on its confirmation (look_confirmed)
            let supported = relaxed
                || covered >= rules.strong
                || match rules.call_context.support {
                    CallSupport::Off => window(),
                    CallSupport::Or => window() || calls(i, package),
                    CallSupport::Replace => calls(i, package),
                };
            Some(Pick {
                package,
                covered,
                supported,
            })
        })
        .collect()
}

/// The claims of `queries` (statement units, in statement order) under
/// `rules`; `same` (one package family?) decides agreement.
pub fn claim_units(
    index: &LibraryIndex,
    queries: &[UnitQuery<'_>],
    rules: LookupRules,
    same: &dyn Fn(&str, &str) -> bool,
) -> Vec<UnitClaim> {
    let mut out = Vec::new();
    for (q, pick) in queries.iter().zip(picks(index, queries, &rules, same)) {
        let Some(Pick {
            package: p,
            covered,
            supported: true,
        }) = pick
        else {
            continue;
        };
        let package = &index.packages[p as usize];
        let mut claim = UnitClaim {
            unit: q.unit,
            package: package.clone(),
            structural: covered,
            literal: None,
        };
        if rules.agreement {
            let Some((l, score)) = index.literal_claim(q.text, &rules, same) else {
                continue;
            };
            let named = &index.packages[l as usize];
            if !same(named, package) {
                continue;
            }
            claim.package = named.clone();
            claim.literal = Some((named.clone(), score));
        }
        out.push(claim);
    }
    out
}

/// Helper rows as the lookup's map: rows of one hash joined, owners sorted
/// and once each.
fn helper_table(rows: Vec<HelperRow>) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in rows {
        out.entry(r.hash).or_default().extend(r.owners);
    }
    for owners in out.values_mut() {
        owners.sort();
        owners.dedup();
    }
    out
}
