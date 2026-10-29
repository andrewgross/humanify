//! The merge: the shards of the selected versions → one index directory
//! (docs/design/library-index.md §4-5).
//!
//! Each fingerprint is stored ONCE per owner with the version ranges it
//! appears in (over the owner's indexed versions, semver order); a key
//! held by two owners stays two rows (collisions are lookup evidence).
//! The stop-listed helper packages are dropped; the prints of the helper
//! SOURCES among them go to the helper table instead (exp129,
//! `helpers.jsonl`). Every table is sorted and
//! the manifest's maps are ordered, so the same shards give the same
//! bytes in any order; the INDEX ID is the manifest's sha256.
//!
//! **Streaming (exp103).** A [`Merger`] takes shards ONE AT A TIME, in
//! label order, and keeps only the merged groups, every string interned
//! once (an owner, version, file, module key or literal is a `u32`; a unit
//! hash a `u64`); a shard is dropped as soon as it is added. The merge
//! used to hold every shard of the build at once, and a copy of them:
//! 112 GB on the 134,000 shards of exp101's 1.8 GB index.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use humanify_core::libindex::format::{
    Callee, EdgeRow, HelperRow, INDEX_FORMAT, LiteralFileRow, LiteralRow, Manifest, ModuleRow,
    NameRow, PackageRecord, RefusedRecord, RegistrySnapshot, SliceRecord, UnitRow,
    fingerprint_versions, sort_versions, version_ranges,
};
use humanify_core::libindex::names::{NameKind, RestoredName};
use humanify_core::libindex::table::{NameInterner, NameRowCompact, RawName, unit_key};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::shard::{Shard, hex_sha256};

/// The stop-lists shipped with every index.
pub const STOPLISTS_JSON: &str = include_str!("../data/stoplists.json");
/// The package families shipped with every index.
pub const FAMILIES_JSON: &str = humanify_core::libindex::families::FAMILIES_JSON;

/// The literal table's provenance file: each literal file's owner, path
/// and versions, row for row with `literals.bin`'s files.
pub const LITERAL_FILES: &str = "literals.files.jsonl";

/// Everything a merge records besides the shards (all of it already
/// decided by the plan).
#[derive(Default)]
pub struct MergeMeta {
    pub as_of: String,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
    pub packages: Vec<PackageRecord>,
    pub refused: Vec<RefusedRecord>,
    /// exp161: when set, the names jsonl tables are STREAMED here (they
    /// are the merge's one table too big to buffer: the full corpus's
    /// merged rows) instead of held in the files map; their sha256 is
    /// computed while streaming and recorded in the manifest either way.
    /// None — the tests' path — buffers them in RAM as every other table.
    pub staging: Option<std::path::PathBuf>,
}

/// The index directory's files and its id.
pub struct IndexFiles {
    pub files: BTreeMap<String, Vec<u8>>,
    pub id: String,
}

/// A stop-list's string list `k` (empty when absent).
fn stop_list(stop: &serde_json::Value, k: &str) -> Vec<String> {
    stop.get(k)
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .map(str::to_string)
        .collect()
}

/// Is `owner` a stop-listed helper package?
fn is_helper(owner: &str, stop: &serde_json::Value) -> bool {
    stop_list(stop, "helperPackages").iter().any(|p| p == owner)
        || stop_list(stop, "helperPackagePrefixes")
            .iter()
            .any(|p| owner.starts_with(p.as_str()))
}

/// Is `owner` a helper SOURCE (exp129): a stop-listed package whose own
/// code is canonical transpiler helper code, its prints kept in the helper
/// table?
fn is_helper_source(owner: &str, stop: &serde_json::Value) -> bool {
    stop_list(stop, "helperSources").iter().any(|p| p == owner)
}

fn jsonl<T: Serialize>(rows: &[T]) -> Vec<u8> {
    let mut out = Vec::new();
    for r in rows {
        out.extend(serde_json::to_vec(r).unwrap_or_default());
        out.push(b'\n');
    }
    out
}

fn pretty<T: Serialize>(v: &T) -> Vec<u8> {
    let mut out = serde_json::to_vec_pretty(v).unwrap_or_default();
    out.push(b'\n');
    out
}

/// Two interned names lists in the restored names' order (slot, name
/// STRING, kind): [`RestoredName`]'s derive order, elementwise, then by
/// length — the order the names jsonl and bin hold a row's lists in.
fn cmp_name_lists(a: &NameList, b: &NameList, s: &Interner) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let ord =
            x.0.cmp(&y.0)
                .then_with(|| s.cmp(x.1, y.1))
                .then_with(|| x.2.cmp(&y.2));
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// A streamed table's sha256, hex — [`hex_sha256`]'s spelling.
fn hex_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every string of the merge, once.
#[derive(Default)]
struct Interner {
    ids: HashMap<Box<str>, u32>,
    strings: Vec<Box<str>>,
}

impl Interner {
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.into());
        self.ids.insert(s.into(), id);
        id
    }

    /// An already-interned string's id (None when absent — never inserts).
    fn lookup(&self, s: &str) -> Option<u32> {
        self.ids.get(s).copied()
    }

    fn get(&self, id: u32) -> &str {
        &self.strings[id as usize]
    }

    fn cmp(&self, a: u32, b: u32) -> Ordering {
        self.get(a).cmp(self.get(b))
    }

    /// Two sequences as their strings compare (a `Vec<String>`'s order).
    fn cmp_seq(&self, a: &[u32], b: &[u32]) -> Ordering {
        a.iter()
            .map(|&x| self.get(x))
            .cmp(b.iter().map(|&x| self.get(x)))
    }

    fn owned(&self, ids: &[u32]) -> Vec<String> {
        ids.iter().map(|&i| self.get(i).to_string()).collect()
    }
}

/// A restored names list, interned (exp161): (slot, name string id, kind
/// ordinal). The full merge holds millions of distinct lists, so the
/// spellings live once in [`Merger`]'s interner, never per list; the
/// restored-name order (slot, name STRING, kind —
/// [`RestoredName`]'s derive order, and the kinds' ordinals number the
/// variants in that same order) is rebuilt from the interner at write
/// time.
type NameList = Vec<(u32, u32, u32)>;

/// A unit group: the FIRST shard's nodes and content (shards come in
/// label order), the smallest module, the versions it was seen in, and
/// the callees any of its occurrences recorded (a union: an index edge a
/// bundle lacks says nothing, docs/matching-architecture.md §4).
struct UnitGroup {
    nodes: u32,
    content: u32,
    module_bytes: u64,
    versions: Vec<u32>,
    inside: BTreeSet<u64>,
    /// (owner, print) of each OUT-OF callee.
    out_of: BTreeSet<(u32, u64)>,
    /// exp159: the restored names lists the group's versions were recovered
    /// with, each with the version ids that named it (the sources disagree
    /// across versions: several lists, an ambiguous answer).
    names: BTreeMap<NameList, Vec<u32>>,
}

/// One slice's groups.
#[derive(Default)]
struct SliceGroups {
    /// (key, owner) → (files, versions)
    modules: HashMap<(u32, u32), (BTreeSet<u32>, Vec<u32>)>,
    /// (hash, owner, file) → its group
    units: HashMap<(u64, u32, u32), UnitGroup>,
}

/// A literal file group: (owner, file, its literal set).
type LiteralGroup = (u32, u32, Box<[u32]>);

/// The streaming merge (module doc).
pub struct Merger {
    stop: serde_json::Value,
    strings: Interner,
    /// owner → the versions any of its rows was seen in
    versions: BTreeMap<u32, BTreeSet<u32>>,
    slices: BTreeMap<String, SliceGroups>,
    /// (owner, file, literal set) → versions
    literals: HashMap<LiteralGroup, Vec<u32>>,
    /// label → shard key, every shard added
    shard_keys: BTreeMap<String, String>,
    last: Option<(String, String)>,
    /// The helper table (exp129): a helper source's unit hash → the helper
    /// packages holding it, over every slice.
    helpers: BTreeMap<u64, BTreeSet<String>>,
}

impl Default for Merger {
    fn default() -> Self {
        Merger {
            stop: serde_json::from_str(STOPLISTS_JSON).unwrap_or_default(),
            strings: Interner::default(),
            versions: BTreeMap::new(),
            slices: BTreeMap::new(),
            literals: HashMap::new(),
            shard_keys: BTreeMap::new(),
            last: None,
            helpers: BTreeMap::new(),
        }
    }
}

impl Merger {
    /// Add one shard. Shards must come in (label, key) order — a unit
    /// group keeps the first shard's nodes and content — so one out of
    /// order is refused.
    pub fn add(&mut self, shard: Shard) -> Result<(), String> {
        let order = (shard.label(), shard.key.clone());
        if self.last.as_ref().is_some_and(|last| *last > order) {
            return Err(format!("shard {} added out of label order", order.0));
        }
        self.shard_keys.insert(order.0.clone(), shard.key.clone());
        self.last = Some(order);
        if shard.kind != "literals" {
            self.slices.entry(shard.kind.clone()).or_default();
        }
        self.add_modules(&shard);
        self.add_units(&shard)?;
        self.add_literals(&shard);
        Ok(())
    }

    fn seen(&mut self, owner: &str, version: &str) -> (u32, u32) {
        let (o, v) = (self.strings.id(owner), self.strings.id(version));
        self.versions.entry(o).or_default().insert(v);
        (o, v)
    }

    fn add_modules(&mut self, shard: &Shard) {
        for m in &shard.modules {
            if is_helper(&m.owner, &self.stop) {
                continue;
            }
            let (owner, version) = self.seen(&m.owner, &m.version);
            let (key, file) = (self.strings.id(&m.key), self.strings.id(&m.file));
            let g = self.slices.get_mut(&shard.kind).expect("the shard's slice");
            let e = g.modules.entry((key, owner)).or_default();
            e.0.insert(file);
            e.1.push(version);
        }
    }

    fn add_units(&mut self, shard: &Shard) -> Result<(), String> {
        for u in &shard.units {
            let hash = unit_key(&u.hash).ok_or_else(|| {
                format!(
                    "shard {}: unit hash {:?} is not 16 lowercase hex digits",
                    shard.label(),
                    u.hash
                )
            })?;
            if is_helper(&u.owner, &self.stop) {
                if is_helper_source(&u.owner, &self.stop) {
                    self.helpers
                        .entry(hash)
                        .or_default()
                        .insert(u.owner.clone());
                }
                continue;
            }
            let (owner, version) = self.seen(&u.owner, &u.version);
            let file = self.strings.id(&u.file);
            let callee = |h: &str| {
                unit_key(h).ok_or_else(|| {
                    format!(
                        "shard {}: callee hash {h:?} is not 16 lowercase hex digits",
                        shard.label()
                    )
                })
            };
            let inside = u
                .inside
                .iter()
                .map(|h| callee(h))
                .collect::<Result<Vec<u64>, String>>()?;
            // a stop-listed helper is in no table: no OUT-OF edge names it
            let mut out_of = Vec::with_capacity(u.out_of.len());
            for c in u.out_of.iter().filter(|c| !is_helper(&c.owner, &self.stop)) {
                out_of.push((self.strings.id(&c.owner), callee(&c.hash)?));
            }
            let g = self.slices.get_mut(&shard.kind).expect("the shard's slice");
            let e = g.units.entry((hash, owner, file)).or_insert(UnitGroup {
                nodes: u.nodes,
                content: u.content,
                module_bytes: u.module_bytes,
                versions: Vec::new(),
                inside: BTreeSet::new(),
                out_of: BTreeSet::new(),
                names: BTreeMap::new(),
            });
            e.module_bytes = e.module_bytes.min(u.module_bytes);
            e.versions.push(version);
            e.inside.extend(inside);
            e.out_of.extend(out_of);
            if !u.names.is_empty() {
                let list: NameList = u
                    .names
                    .iter()
                    .map(|n| (n.slot, self.strings.id(&n.name), n.kind.ordinal()))
                    .collect();
                e.names.entry(list).or_default().push(version);
            }
        }
        Ok(())
    }

    fn add_literals(&mut self, shard: &Shard) {
        for l in &shard.literals {
            if is_helper(&l.owner, &self.stop) {
                continue;
            }
            let (owner, version) = self.seen(&l.owner, &l.version);
            let file = self.strings.id(&l.file);
            let lits: Box<[u32]> = l.lits.iter().map(|s| self.strings.id(s)).collect();
            self.literals
                .entry((owner, file, lits))
                .or_default()
                .push(version);
        }
    }

    /// Each owner's indexed versions (semver order), by owner name.
    fn indexed(&self) -> BTreeMap<String, Vec<String>> {
        self.versions
            .iter()
            .map(|(&o, vs)| {
                let mut vs: Vec<String> = vs
                    .iter()
                    .map(|&v| self.strings.get(v).to_string())
                    .collect();
                sort_versions(&mut vs);
                (self.strings.get(o).to_string(), vs)
            })
            .collect()
    }

    fn ranges(
        &self,
        indexed: &BTreeMap<String, Vec<String>>,
        owner: u32,
        versions: &[u32],
    ) -> Vec<(String, String)> {
        version_ranges(
            &indexed[self.strings.get(owner)],
            &self.strings.owned(versions),
        )
    }

    /// `modules.<slice>.jsonl`, sorted by (key, owner).
    fn module_rows(
        &self,
        g: &SliceGroups,
        indexed: &BTreeMap<String, Vec<String>>,
    ) -> Vec<ModuleRow> {
        let s = &self.strings;
        let mut keys: Vec<&(u32, u32)> = g.modules.keys().collect();
        keys.sort_by(|a, b| s.cmp(a.0, b.0).then_with(|| s.cmp(a.1, b.1)));
        keys.into_iter()
            .map(|k| {
                let (files, versions) = &g.modules[k];
                let mut files: Vec<String> = files.iter().map(|&f| s.get(f).to_string()).collect();
                files.sort();
                ModuleRow {
                    key: s.get(k.0).to_string(),
                    owner: s.get(k.1).to_string(),
                    files,
                    versions: self.ranges(indexed, k.1, versions),
                }
            })
            .collect()
    }

    /// `units.<slice>.jsonl`, sorted by (hash, owner, file): a hash's
    /// fixed-width lowercase hex sorts as its number.
    fn unit_rows(&self, g: &SliceGroups, indexed: &BTreeMap<String, Vec<String>>) -> Vec<UnitRow> {
        let s = &self.strings;
        let mut keys: Vec<&(u64, u32, u32)> = g.units.keys().collect();
        keys.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| s.cmp(a.1, b.1))
                .then_with(|| s.cmp(a.2, b.2))
        });
        keys.into_iter()
            .map(|k| {
                let u = &g.units[k];
                UnitRow {
                    hash: format!("{:016x}", k.0),
                    owner: s.get(k.1).to_string(),
                    file: s.get(k.2).to_string(),
                    nodes: u.nodes,
                    content: u.content,
                    module_bytes: u.module_bytes,
                    versions: self.ranges(indexed, k.1, &u.versions),
                }
            })
            .collect()
    }

    /// `names.<slice>.jsonl` (exp159), both forms at once (exp161): the
    /// unit rows' restored names, in unit row order and, within a row, the
    /// restored-name order — one row per distinct list over the group's
    /// versions; a unit whose rows recovered nothing has none. The row's
    /// jsonl bytes go to `sink` (the provenance table: streamed, never
    /// buffered whole); the row comes back interned ([`NameRowCompact`])
    /// for the bin encode, its lists already in the restored-name order.
    /// `id_of` is the unit row id each group key holds.
    fn name_rows_into(
        &self,
        g: &SliceGroups,
        indexed: &BTreeMap<String, Vec<String>>,
        id_of: &HashMap<(u64, u32, u32), u32>,
        remap: &HashMap<u32, u32>,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<Vec<NameRowCompact>, String> {
        let s = &self.strings;
        let mut keys: Vec<&(u64, u32, u32)> = g
            .units
            .iter()
            .filter(|(_, u)| !u.names.is_empty())
            .map(|(k, _)| k)
            .collect();
        keys.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| s.cmp(a.1, b.1))
                .then_with(|| s.cmp(a.2, b.2))
        });
        let mut compact = Vec::new();
        for &k in &keys {
            let u = &g.units[k];
            let mut lists: Vec<(&NameList, &Vec<u32>)> = u.names.iter().collect();
            lists.sort_by(|a, b| cmp_name_lists(a.0, b.0, s));
            let mut compact_lists: Vec<Vec<RawName>> = Vec::with_capacity(lists.len());
            for &(l, _) in &lists {
                let mut out = Vec::with_capacity(l.len());
                for &(slot, name, kind) in l {
                    let mapped = *remap
                        .get(&name)
                        .ok_or_else(|| format!("names list: name id {name} is not interned"))?;
                    out.push(RawName {
                        slot,
                        name: mapped,
                        kind,
                    });
                }
                compact_lists.push(out);
            }
            // one provenance row per distinct list, in the restored-name
            // order
            for &(l, vs) in &lists {
                let mut versions = vs.clone();
                versions.sort_unstable();
                versions.dedup();
                let row = NameRow {
                    hash: format!("{:016x}", k.0),
                    owner: s.get(k.1).to_string(),
                    file: s.get(k.2).to_string(),
                    names: l
                        .iter()
                        .map(|&(slot, name, kind)| RestoredName {
                            slot,
                            name: s.get(name).to_string(),
                            kind: NameKind::from_ordinal(kind)
                                .expect("a kind ordinal the interner wrote"),
                        })
                        .collect(),
                    versions: self.ranges(indexed, k.1, &versions),
                };
                sink(&serde_json::to_vec(&row).map_err(|e| e.to_string())?)?;
                sink(b"\n")?;
            }
            compact.push(NameRowCompact {
                row: *id_of.get(k).ok_or_else(|| {
                    format!("names row {:016x}: no unit row of the slice holds it", k.0)
                })?,
                lists: compact_lists,
            });
        }
        Ok(compact)
    }

    /// `edges.<slice>.jsonl`: the unit rows' recorded callees, in unit row
    /// order (a row with none has no edge row); OUT-OF callees sorted by
    /// (owner name, hash).
    fn edge_rows(&self, g: &SliceGroups) -> Vec<EdgeRow> {
        let s = &self.strings;
        let mut keys: Vec<&(u64, u32, u32)> = g
            .units
            .iter()
            .filter(|(_, u)| !u.inside.is_empty() || !u.out_of.is_empty())
            .map(|(k, _)| k)
            .collect();
        keys.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| s.cmp(a.1, b.1))
                .then_with(|| s.cmp(a.2, b.2))
        });
        keys.into_iter()
            .map(|k| {
                let u = &g.units[k];
                let mut out_of: Vec<Callee> = u
                    .out_of
                    .iter()
                    .map(|&(o, h)| Callee {
                        owner: s.get(o).to_string(),
                        hash: format!("{h:016x}"),
                    })
                    .collect();
                out_of.sort();
                EdgeRow {
                    hash: format!("{:016x}", k.0),
                    owner: s.get(k.1).to_string(),
                    file: s.get(k.2).to_string(),
                    inside: u.inside.iter().map(|h| format!("{h:016x}")).collect(),
                    out_of,
                }
            })
            .collect()
    }

    /// The literal file rows, sorted by (owner, file, literal set).
    fn literal_rows(&self, indexed: &BTreeMap<String, Vec<String>>) -> Vec<LiteralRow> {
        let s = &self.strings;
        let mut keys: Vec<&LiteralGroup> = self.literals.keys().collect();
        keys.sort_by(|a, b| {
            s.cmp(a.0, b.0)
                .then_with(|| s.cmp(a.1, b.1))
                .then_with(|| s.cmp_seq(&a.2, &b.2))
        });
        keys.into_iter()
            .map(|k| LiteralRow {
                owner: s.get(k.0).to_string(),
                file: s.get(k.1).to_string(),
                lits: s.owned(&k.2),
                versions: self.ranges(indexed, k.0, &self.literals[k]),
            })
            .collect()
    }

    /// The index's files: the row tables (provenance, read by
    /// instruments) and the lookup tables a run reads
    /// (`humanify_core::libindex::table`), derived from the same rows.
    pub fn finish(self, meta: MergeMeta) -> Result<IndexFiles, String> {
        let indexed = self.indexed();
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert("versions.json".into(), pretty(&indexed));
        let mut units: BTreeMap<String, Vec<UnitRow>> = BTreeMap::new();
        let mut edges: BTreeMap<String, Vec<EdgeRow>> = BTreeMap::new();
        for (kind, g) in &self.slices {
            files.insert(
                format!("modules.{kind}.jsonl"),
                jsonl(&self.module_rows(g, &indexed)),
            );
            let rows = self.unit_rows(g, &indexed);
            if !rows.is_empty() {
                files.insert(format!("units.{kind}.jsonl"), jsonl(&rows));
                units.insert(kind.clone(), rows);
                let edge_rows = self.edge_rows(g);
                files.insert(
                    humanify_core::libindex::table::edge_rows_file(kind),
                    jsonl(&edge_rows),
                );
                edges.insert(kind.clone(), edge_rows);
            }
        }
        // The names tables (exp159, exp161): the distinct names first —
        // sorted, the strings file's ids are its positions — then the
        // rows, the jsonl streamed to `meta.staging` when it is set (the
        // one table too big to buffer on the full corpus), the bin encoded
        // from the interned compact rows.
        let mut interner = NameInterner::default();
        let mut remap: HashMap<u32, u32> = HashMap::new();
        {
            let mut distinct: BTreeSet<u32> = BTreeSet::new();
            for g in self.slices.values() {
                // The group map is a HashMap and its order is nondeterministic,
                // but the destination is a SET that is sorted by string right
                // below — iteration order cannot reach the output.
                #[allow(clippy::iter_over_hash_type)]
                for u in g.units.values() {
                    for list in u.names.keys() {
                        for &(_, name, _) in list {
                            distinct.insert(name);
                        }
                    }
                }
            }
            let mut ordered: Vec<u32> = distinct.into_iter().collect();
            ordered.sort_by(|a, b| self.strings.cmp(*a, *b));
            for id in ordered {
                remap.insert(id, interner.id(self.strings.get(id)));
            }
        }
        let mut names: BTreeMap<String, Vec<NameRowCompact>> = BTreeMap::new();
        let mut staged: BTreeMap<String, String> = BTreeMap::new();
        for (kind, g) in &self.slices {
            if g.units.values().all(|u| u.names.is_empty()) {
                continue;
            }
            let unit_rows = units.get(kind).ok_or_else(|| {
                format!("names rows for slice {kind}: the slice has no unit rows")
            })?;
            let ids = humanify_core::libindex::table::unit_row_ids(unit_rows)?;
            let mut id_of: HashMap<(u64, u32, u32), u32> = HashMap::with_capacity(ids.len());
            for (r, id) in unit_rows.iter().zip(&ids) {
                let key = unit_key(&r.hash).ok_or_else(|| {
                    format!("unit row hash {:?} is not 16 lowercase hex digits", r.hash)
                })?;
                let owner = self
                    .strings
                    .lookup(&r.owner)
                    .ok_or_else(|| format!("unit row owner {r:?}: the merge never interned it"))?;
                let file = self
                    .strings
                    .lookup(&r.file)
                    .ok_or_else(|| format!("unit row file {r:?}: the merge never interned it"))?;
                id_of.insert((key, owner, file), *id);
            }
            let compact = {
                let mut buffer: Vec<u8> = Vec::new();
                let mut hasher = Sha256::new();
                let mut file = match &meta.staging {
                    Some(dir) => {
                        std::fs::create_dir_all(dir)
                            .map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
                        Some(std::io::BufWriter::new(
                            std::fs::File::create(
                                dir.join(humanify_core::libindex::table::names_rows_file(kind)),
                            )
                            .map_err(|e| format!("create the names jsonl: {e}"))?,
                        ))
                    }
                    None => None,
                };
                let mut sink = |bytes: &[u8]| -> Result<(), String> {
                    hasher.update(bytes);
                    match &mut file {
                        Some(f) => std::io::Write::write_all(f, bytes)
                            .map_err(|e| format!("write the names jsonl: {e}")),
                        None => {
                            buffer.extend_from_slice(bytes);
                            Ok(())
                        }
                    }
                };
                let compact = self.name_rows_into(g, &indexed, &id_of, &remap, &mut sink)?;
                // the block ends the sink's borrows of hasher/file/buffer
                match file {
                    Some(mut f) => {
                        std::io::Write::flush(&mut f)
                            .map_err(|e| format!("flush the names jsonl: {e}"))?;
                        staged.insert(
                            humanify_core::libindex::table::names_rows_file(kind),
                            hex_digest(hasher),
                        );
                    }
                    None => {
                        files.insert(
                            humanify_core::libindex::table::names_rows_file(kind),
                            buffer,
                        );
                    }
                }
                compact
            };
            names.insert(kind.clone(), compact);
        }
        files.extend(humanify_core::libindex::table::names_lookup_files(
            &units, &names, &interner,
        )?);
        let literals = self.literal_rows(&indexed);
        files.extend(humanify_core::libindex::table::lookup_files(
            &units,
            Some(&edges),
            &literals,
        )?);
        drop(units);
        drop(edges);
        let provenance: Vec<LiteralFileRow> = literals
            .into_iter()
            .map(|r| LiteralFileRow {
                owner: r.owner,
                file: r.file,
                versions: r.versions,
            })
            .collect();
        files.insert(LITERAL_FILES.into(), jsonl(&provenance));
        if !self.helpers.is_empty() {
            let rows: Vec<HelperRow> = self
                .helpers
                .iter()
                .map(|(hash, owners)| HelperRow {
                    hash: format!("{hash:016x}"),
                    owners: owners.iter().cloned().collect(),
                })
                .collect();
            files.insert(
                humanify_core::libindex::table::HELPERS_FILE.into(),
                jsonl(&rows),
            );
        }
        let data: BTreeMap<String, &str> = [
            ("families.json".to_string(), FAMILIES_JSON),
            ("stoplists.json".to_string(), STOPLISTS_JSON),
        ]
        .into_iter()
        .collect();
        let manifest = Manifest {
            format: INDEX_FORMAT.to_string(),
            as_of: meta.as_of,
            fingerprints: fingerprint_versions(),
            registry: meta.registry,
            slices: meta.slices,
            packages: meta.packages,
            refused: meta.refused,
            shards: self.shard_keys,
            tables: files
                .iter()
                .map(|(name, bytes)| (name.clone(), hex_sha256(bytes)))
                .chain(staged.iter().map(|(name, sha)| (name.clone(), sha.clone())))
                .collect(),
            data: data
                .iter()
                .map(|(name, text)| (name.clone(), hex_sha256(text.as_bytes())))
                .collect(),
        };
        for (name, text) in data {
            files.insert(name, text.as_bytes().to_vec());
        }
        let manifest_bytes = pretty(&manifest);
        let id = hex_sha256(&manifest_bytes);
        files.insert("manifest.json".into(), manifest_bytes);
        Ok(IndexFiles { files, id })
    }
}

/// Merge every shard at once (sorted into label order first) — the
/// streaming merge on a list.
pub fn merge(meta: MergeMeta, mut shards: Vec<Shard>) -> Result<IndexFiles, String> {
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    let mut m = Merger::default();
    for s in shards {
        m.add(s)?;
    }
    m.finish(meta)
}
