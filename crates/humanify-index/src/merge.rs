//! The merge: the shards of the selected versions → one index directory
//! (docs/design/library-index.md §4-5).
//!
//! Each fingerprint is stored ONCE per owner with the version ranges it
//! appears in (over the owner's indexed versions, semver order); a key
//! held by two owners stays two rows (collisions are lookup evidence).
//! The stop-listed helper packages are dropped. Every table is sorted and
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
    INDEX_FORMAT, LiteralFileRow, LiteralRow, Manifest, ModuleRow, PackageRecord, RefusedRecord,
    RegistrySnapshot, SliceRecord, UnitRow, fingerprint_versions, sort_versions, version_ranges,
};
use humanify_core::libindex::table::unit_key;
use serde::Serialize;

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
}

/// The index directory's files and its id.
pub struct IndexFiles {
    pub files: BTreeMap<String, Vec<u8>>,
    pub id: String,
}

/// Is `owner` a stop-listed helper package?
fn is_helper(owner: &str, stop: &serde_json::Value) -> bool {
    let list = |k: &str| {
        stop.get(k)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    list("helperPackages").iter().any(|p| p == owner)
        || list("helperPackagePrefixes")
            .iter()
            .any(|p| owner.starts_with(p.as_str()))
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

/// A unit group: the FIRST shard's nodes and content (shards come in
/// label order), the smallest module, the versions it was seen in.
struct UnitGroup {
    nodes: u32,
    content: u32,
    module_bytes: u64,
    versions: Vec<u32>,
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
            if is_helper(&u.owner, &self.stop) {
                continue;
            }
            let hash = unit_key(&u.hash).ok_or_else(|| {
                format!(
                    "shard {}: unit hash {:?} is not 16 lowercase hex digits",
                    shard.label(),
                    u.hash
                )
            })?;
            let (owner, version) = self.seen(&u.owner, &u.version);
            let file = self.strings.id(&u.file);
            let g = self.slices.get_mut(&shard.kind).expect("the shard's slice");
            let e = g.units.entry((hash, owner, file)).or_insert(UnitGroup {
                nodes: u.nodes,
                content: u.content,
                module_bytes: u.module_bytes,
                versions: Vec::new(),
            });
            e.module_bytes = e.module_bytes.min(u.module_bytes);
            e.versions.push(version);
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
                    vendored: None,
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
        for (kind, g) in &self.slices {
            files.insert(
                format!("modules.{kind}.jsonl"),
                jsonl(&self.module_rows(g, &indexed)),
            );
            let rows = self.unit_rows(g, &indexed);
            if !rows.is_empty() {
                units.insert(kind.clone(), rows);
            }
        }
        let literals = self.literal_rows(&indexed);
        files.insert(
            VENDORED_FILE.into(),
            pretty(&mark_vendored_rows(&mut units, &literals)),
        );
        for (kind, rows) in &units {
            files.insert(format!("units.{kind}.jsonl"), jsonl(rows));
        }
        files.extend(humanify_core::libindex::table::lookup_files(
            &units, &literals,
        )?);
        drop(units);
        let provenance: Vec<LiteralFileRow> = literals
            .into_iter()
            .map(|r| LiteralFileRow {
                owner: r.owner,
                file: r.file,
                versions: r.versions,
            })
            .collect();
        files.insert(LITERAL_FILES.into(), jsonl(&provenance));
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

/// The merge's summary of the vendored-copy marks (exp115).
pub const VENDORED_FILE: &str = "vendored.json";

/// What [`mark_vendored_rows`] marked, per slice.
#[derive(serde::Serialize)]
struct VendoredSummary {
    about: &'static str,
    slices: BTreeMap<String, humanify_core::libindex::vendored::VendoredStats>,
}

/// Mark every slice's unit rows whose file is a vendored copy of another
/// package (`humanify_core::libindex::vendored`): the names known are the
/// index's owners, the families the shipped ones joined by the derived
/// links among them (what a run's family judgement reads).
fn mark_vendored_rows(
    units: &mut BTreeMap<String, Vec<UnitRow>>,
    literals: &[LiteralRow],
) -> VendoredSummary {
    use humanify_core::libindex::families::PackageFamilies;
    use humanify_core::libindex::vendored::mark_vendored;
    let slices: Vec<&[UnitRow]> = units.values().map(Vec::as_slice).collect();
    let names: BTreeSet<String> = humanify_core::libindex::table::package_table(&slices, literals)
        .into_iter()
        .collect();
    let families = PackageFamilies::shipped().with_shipped_links_among(&names);
    let known = |n: &str| names.contains(n);
    let same = |a: &str, b: &str| families.same(a, b);
    VendoredSummary {
        about: "Unit rows whose file is a vendored copy of another package (exp115, \
                humanify_core::libindex::vendored): per slice, the rows and files marked.",
        slices: units
            .iter_mut()
            .map(|(kind, rows)| (kind.clone(), mark_vendored(rows, &known, &same)))
            .collect(),
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
