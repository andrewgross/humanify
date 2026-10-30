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
use std::path::Path;

use humanify_core::libindex::format::{
    Callee, EdgeRow, HelperRow, INDEX_FORMAT, LiteralFileRow, LiteralRow, Manifest, ModuleRow,
    NameRow, PackageRecord, RefusedRecord, RegistrySnapshot, SliceRecord, UnitRow,
    fingerprint_versions, sort_versions, version_ranges,
};
use humanify_core::libindex::names::{NameKind, RestoredName};
use humanify_core::libindex::table::{NameInterner, NameRowCompact, RawName, unit_key};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::shard::{Shard, ShardInputs, hex_sha256, read_current, shard_path};

/// The stop-lists shipped with every index.
pub const STOPLISTS_JSON: &str = include_str!("../data/stoplists.json");
/// The package families shipped with every index.
pub const FAMILIES_JSON: &str = humanify_core::libindex::families::FAMILIES_JSON;

/// The literal table's provenance file: each literal file's owner, path
/// and versions, row for row with `literals.bin`'s files.
pub const LITERAL_FILES: &str = "literals.files.jsonl";

/// The part of the KEY space a merge retains (exp162): everything — the
/// streaming merge — or one contiguous range of it, a parallel merge's
/// worker's share. A key is a unit hash's u64 where the row has one
/// (units, edges, names, helper prints) and, for the tables without one,
/// an order-preserving prefix of the row's first sort field (a module
/// key's, a literal file owner's — [`key_prefix`]), so the segments
/// concatenate in range order into the same sorted tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyFilter {
    #[default]
    All,
    /// `[lo, hi)` of the key space, as u128 (the whole space, 2^64, does
    /// not fit a u64).
    Range { lo: u128, hi: u128 },
}

impl KeyFilter {
    pub(crate) fn holds(&self, key: u64) -> bool {
        match self {
            KeyFilter::All => true,
            KeyFilter::Range { lo, hi } => {
                let k = u128::from(key);
                k >= *lo && k < *hi
            }
        }
    }

    /// Does a row whose first sort field is the string `s` fall in range?
    pub(crate) fn holds_str(&self, s: &str) -> bool {
        self.holds(key_prefix(s))
    }
}

/// The order-preserving key prefix: a string's first 8 bytes as a
/// big-endian u64, zero-padded. `a < b` implies
/// `key_prefix(a) <= key_prefix(b)` (a differing byte decides inside the
/// prefix; a prefix string's pad is 0, below every byte — a NUL byte
/// equals it, and equal prefixes are one range), so two strings in
/// different [`KeyFilter`] ranges order as their strings do.
fn key_prefix(s: &str) -> u64 {
    let mut b = [0u8; 8];
    let n = s.len().min(8);
    b[..n].copy_from_slice(&s.as_bytes()[..n]);
    u64::from_be_bytes(b)
}

/// `n` contiguous ranges covering the WHOLE key space, equal in key
/// space (not in rows): range i is `[i*2^64/n, (i+1)*2^64/n)` — the last
/// range's end is 2^64 whatever the division rounded to, so no key falls
/// between ranges.
pub fn key_ranges(n: usize) -> Vec<KeyFilter> {
    let n = n.max(1);
    let per = (1u128 << 64) / n as u128;
    (0..n)
        .map(|i| KeyFilter::Range {
            lo: i as u128 * per,
            hi: if i + 1 == n {
                1u128 << 64
            } else {
                (i + 1) as u128 * per
            },
        })
        .collect()
}

/// Everything a merge records besides the shards (all of it already
/// decided by the plan).
#[derive(Default)]
pub struct MergeMeta {
    pub as_of: String,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
    pub packages: Vec<PackageRecord>,
    pub refused: Vec<RefusedRecord>,
    /// exp161, generalized by exp162: when set (a build's `--out` dir),
    /// EVERY table the finish produces is STREAMED into it as its rows
    /// finish, recorded by sha256 in the manifest's `tables`, and never
    /// held whole — only one table's bytes are alive at a time. None — the
    /// tests' path — buffers every table in RAM for the caller to write.
    pub staging: Option<std::path::PathBuf>,
}

/// The index directory's files and its id. `written` names the tables the
/// finish already STREAMED into the staging dir (name → sha256): they are
/// on disk, `write_index` must not write them again; the staging-less
/// finish (the tests) leaves it empty and holds every table in `files`.
pub struct IndexFiles {
    pub files: BTreeMap<String, Vec<u8>>,
    pub written: BTreeMap<String, String>,
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

/// Where the finish's tables go as it produces them: buffered into the
/// files map (staging unset — the tests' path, every table held), or
/// written into the staging dir and recorded by sha (staging set — a
/// build's out dir; only one table's bytes are ever alive at once).
enum TableSink<'a> {
    Buffer(&'a mut BTreeMap<String, Vec<u8>>),
    Stream(&'a std::path::Path, &'a mut BTreeMap<String, String>),
}

/// One streamed table's bytes: hashed as they pass, and either buffered or
/// written through a BufWriter to the staging dir's file.
struct JsonlStream {
    hasher: Sha256,
    out: StreamOut,
}

enum StreamOut {
    Buffer(Vec<u8>),
    File(std::io::BufWriter<std::fs::File>),
}

impl JsonlStream {
    fn take(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.hasher.update(bytes);
        match &mut self.out {
            StreamOut::Buffer(b) => {
                b.extend_from_slice(bytes);
                Ok(())
            }
            StreamOut::File(f) => {
                std::io::Write::write_all(f, bytes).map_err(|e| format!("write the table: {e}"))
            }
        }
    }

    /// One row: serde's writer path — the bytes [`jsonl`] would build.
    fn row<T: Serialize>(&mut self, r: &T) -> Result<(), String> {
        let mut line = Vec::new();
        serde_json::to_writer(&mut line, r).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.take(&line)
    }

    /// Flush; the table's sha256 and, buffered only, its bytes.
    fn done(self) -> Result<(String, Option<Vec<u8>>), String> {
        match self.out {
            StreamOut::Buffer(b) => Ok((hex_digest(self.hasher), Some(b))),
            StreamOut::File(mut f) => {
                std::io::Write::flush(&mut f).map_err(|e| format!("flush the table: {e}"))?;
                Ok((hex_digest(self.hasher), None))
            }
        }
    }
}

impl TableSink<'_> {
    /// A WHOLE table's bytes (the bin tables, versions.json): buffered, or
    /// written out and recorded by sha.
    fn whole(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), String> {
        match self {
            TableSink::Buffer(files) => {
                files.insert(name.to_string(), bytes);
                Ok(())
            }
            TableSink::Stream(dir, written) => {
                let sha = hex_sha256(&bytes);
                std::fs::write(dir.join(name), &bytes).map_err(|e| format!("write {name}: {e}"))?;
                written.insert(name.to_string(), sha);
                Ok(())
            }
        }
    }

    /// A streamed table opened for its rows: the sink's mode picks where
    /// the bytes land.
    fn stream(&mut self, name: &str) -> Result<JsonlStream, String> {
        let out = match self {
            TableSink::Buffer(_) => StreamOut::Buffer(Vec::new()),
            TableSink::Stream(dir, _) => StreamOut::File(std::io::BufWriter::new(
                std::fs::File::create(dir.join(name)).map_err(|e| format!("create {name}: {e}"))?,
            )),
        };
        Ok(JsonlStream {
            hasher: Sha256::new(),
            out,
        })
    }

    /// A jsonl table from its rows, recorded whole once they are through.
    fn jsonl<T: Serialize>(
        &mut self,
        name: &str,
        rows: impl Iterator<Item = T>,
    ) -> Result<(), String> {
        let mut s = self.stream(name)?;
        for r in rows {
            s.row(&r)?;
        }
        self.record(name, s)
    }

    fn record(&mut self, name: &str, s: JsonlStream) -> Result<(), String> {
        let (sha, bytes) = s.done()?;
        match self {
            TableSink::Buffer(files) => {
                files.insert(name.to_string(), bytes.expect("a buffered stream's bytes"));
            }
            TableSink::Stream(_, written) => {
                written.insert(name.to_string(), sha);
            }
        }
        Ok(())
    }
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

/// An owner's versions as the row's version ranges (the owner's indexed
/// versions with the row's own seen ones marked) — a row's and a
/// segment's (exp162: the segment's own interner resolves the ids, the
/// owner's indexed versions are the merge's global map).
/// Per slice kind: unit group key -> that group's unit rows' name lists, by
/// row. `exp161`'s names shape, carried per segment by the parallel merge.
type NamesGroups = BTreeMap<String, HashMap<(u64, u32, u32), BTreeMap<NameList, Vec<u32>>>>;

fn ranges(
    strings: &Interner,
    indexed: &BTreeMap<String, Vec<String>>,
    owner: u32,
    versions: &[u32],
) -> Vec<(String, String)> {
    version_ranges(&indexed[strings.get(owner)], &strings.owned(versions))
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

/// The streaming merge (module doc). With a [`KeyFilter::Range`] it
/// retains only its range's rows — a parallel merge's worker (exp162) —
/// while still SEEING every row: each row's owner and version reach the
/// versions map (so a worker's `indexed` — its ranges' basis — is
/// complete) and every row's unit hash and callees are still validated,
/// exactly as the streaming merge would have.
pub struct Merger {
    stop: serde_json::Value,
    range: KeyFilter,
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
        Merger::filtered(KeyFilter::All)
    }
}

impl Merger {
    /// A merge retaining only `range`'s keys (a parallel merge's worker,
    /// exp162); [`Merger::default`] retains everything.
    pub fn filtered(range: KeyFilter) -> Self {
        Merger {
            stop: serde_json::from_str(STOPLISTS_JSON).unwrap_or_default(),
            range,
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
            if !self.range.holds_str(&m.key) {
                continue;
            }
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
                if is_helper_source(&u.owner, &self.stop) && self.range.holds(hash) {
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
            // a parallel worker (exp162) keeps only its range's rows —
            // AFTER the whole row is validated (a hash or callee the
            // streaming merge refuses, it refuses too)
            if !self.range.holds(hash) {
                continue;
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
            if !self.range.holds_str(&l.owner) {
                continue;
            }
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
                    versions: ranges(s, indexed, k.1, versions),
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
                    versions: ranges(s, indexed, k.1, &u.versions),
                }
            })
            .collect()
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
                versions: ranges(s, indexed, k.0, &self.literals[k]),
            })
            .collect()
    }

    /// What this merge retained, EMITTED (exp162): its rows per slice,
    /// sorted; its literal and helper rows; and the per-merge GLOBAL state
    /// every worker of a parallel merge recomputes from the whole stream —
    /// the owners' indexed versions and the shard keys, identical in every
    /// worker. A [`KeyFilter::Range`] merge's segment concatenates with
    /// the others in range order ([`finish_segments`]); the whole-space
    /// merge's segment is the streaming merge's tables alone.
    fn segment(mut self) -> Segment {
        let indexed = self.indexed();
        let mut modules: BTreeMap<String, Vec<ModuleRow>> = BTreeMap::new();
        let mut units: BTreeMap<String, Vec<UnitRow>> = BTreeMap::new();
        let mut edges: BTreeMap<String, Vec<EdgeRow>> = BTreeMap::new();
        for (kind, g) in &self.slices {
            modules.insert(kind.clone(), self.module_rows(g, &indexed));
            units.insert(kind.clone(), self.unit_rows(g, &indexed));
            edges.insert(kind.clone(), self.edge_rows(g));
        }
        // exp161's names state, carried per segment (exp162): the drained
        // name lists of every name-bearing unit group, in the segment's
        // OWN string ids, with the interner that resolves them —
        // finish_segments interns the names globally (string order) and
        // emits the rows. Drained, not copied: the group's rest is
        // dropped here.
        let mut names_groups: NamesGroups = BTreeMap::new();
        for (kind, g) in self.slices.iter_mut() {
            let mut ng: HashMap<(u64, u32, u32), BTreeMap<NameList, Vec<u32>>> = HashMap::new();
            // The group map is a HashMap and its order is nondeterministic,
            // but nothing reads the drained map's order (the rows sort
            // their keys by string) — iteration order cannot reach the
            // output.
            #[allow(clippy::iter_over_hash_type)]
            for (k, u) in g.units.iter_mut() {
                if !u.names.is_empty() {
                    ng.insert(*k, std::mem::take(&mut u.names));
                }
            }
            if !ng.is_empty() {
                names_groups.insert(kind.clone(), ng);
            }
        }
        let literals = self.literal_rows(&indexed);
        let helpers: Vec<HelperRow> = self
            .helpers
            .iter()
            .map(|(hash, owners)| HelperRow {
                hash: format!("{hash:016x}"),
                owners: owners.iter().cloned().collect(),
            })
            .collect();
        Segment {
            found: Vec::new(),
            indexed,
            shard_keys: self.shard_keys,
            strings: self.strings,
            names_groups,
            modules,
            units,
            edges,
            literals,
            helpers,
        }
    }

    /// The index's files: the row tables (provenance, read by
    /// instruments) and the lookup tables a run reads
    /// (`humanify_core::libindex::table`), derived from the same rows —
    /// this merge's rows as one [`Segment`], finished like any worker's.
    pub fn finish(self, meta: MergeMeta) -> Result<IndexFiles, String> {
        finish_segments(meta, vec![self.segment()])
    }
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
    strings: &Interner,
    groups: &HashMap<(u64, u32, u32), BTreeMap<NameList, Vec<u32>>>,
    indexed: &BTreeMap<String, Vec<String>>,
    id_of: &HashMap<(u64, u32, u32), u32>,
    name_of: impl Fn(u32) -> Result<u32, String>,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<Vec<NameRowCompact>, String> {
    let s = strings;
    let mut keys: Vec<&(u64, u32, u32)> = groups.keys().collect();
    keys.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| s.cmp(a.1, b.1))
            .then_with(|| s.cmp(a.2, b.2))
    });
    let mut compact = Vec::new();
    for &k in &keys {
        let u = &groups[k];
        let mut lists: Vec<(&NameList, &Vec<u32>)> = u.iter().collect();
        lists.sort_by(|a, b| cmp_name_lists(a.0, b.0, s));
        let mut compact_lists: Vec<Vec<RawName>> = Vec::with_capacity(lists.len());
        for &(l, _) in &lists {
            let mut out = Vec::with_capacity(l.len());
            for &(slot, name, kind) in l {
                out.push(RawName {
                    slot,
                    name: name_of(name)?,
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
                versions: ranges(s, indexed, k.1, &versions),
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

/// One merge's (or one parallel worker's range's, exp162) rows: the
/// per-slice tables, the literal and helper rows, and the global state
/// (`indexed` versions, `shard_keys`) every worker recomputes. `found`
/// (a store stream only) records, per task, whether any of its shards
/// was read.
#[derive(Default)]
pub struct Segment {
    pub found: Vec<bool>,
    indexed: BTreeMap<String, Vec<String>>,
    shard_keys: BTreeMap<String, String>,
    /// exp161's names state, per segment: the interner that resolves the
    /// drained name lists' ids ([`name_rows_into`]).
    strings: Interner,
    /// Per slice, the name-bearing unit groups' name lists (the group key
    /// → its lists with the version ids that named each), drained from
    /// the groups — the ids are the segment's own interner's.
    names_groups: NamesGroups,
    modules: BTreeMap<String, Vec<ModuleRow>>,
    units: BTreeMap<String, Vec<UnitRow>>,
    edges: BTreeMap<String, Vec<EdgeRow>>,
    literals: Vec<LiteralRow>,
    helpers: Vec<HelperRow>,
}

/// The index's files from the workers' segments (exp162). The segments'
/// rows concatenate in range order — each segment's rows are sorted and
/// the ranges are order-preserving ([`key_prefix`]), so the concatenation
/// IS the sorted table the streaming merge built, byte for byte. The
/// interning stays here, on the concatenated rows: the edge tables'
/// shared callee lists and the names' strings are interned over the WHOLE
/// index once, whatever partitioned the rows.
/// One segment's distinct name ids, its LOCAL string order.
fn distinct_local_names(seg: &Segment) -> Vec<u32> {
    let mut ids: Vec<u32> = Vec::new();
    #[allow(clippy::iter_over_hash_type)]
    for lists in seg.names_groups.values().flat_map(|g| g.values()) {
        // The group map is a HashMap and its order is nondeterministic,
        // but the destination is sorted by string right below — iteration
        // order cannot reach the output.
        for list in lists.keys() {
            for &(_, name, _) in list {
                ids.push(name);
            }
        }
    }
    ids.sort_by(|a, b| seg.strings.cmp(*a, *b));
    ids.dedup();
    ids
}

/// The segments' distinct names k-way-merged by string into ONE interner:
/// the smallest string earns the next id, once however many segments hold
/// it — the serial merge's exact id assignment.
fn merge_distinct_names(segments: &[Segment]) -> NameInterner {
    let mut interner = NameInterner::default();
    let mut heads: Vec<Option<(u32, std::vec::IntoIter<u32>)>> = Vec::with_capacity(segments.len());
    for seg in segments {
        let mut it = distinct_local_names(seg).into_iter();
        heads.push(it.next().map(|h| (h, it)));
    }
    let mut last: Option<&str> = None;
    loop {
        let mut best: Option<(usize, &str)> = None;
        for (i, head) in heads.iter().enumerate() {
            if let Some((id, _)) = head {
                let s: &str = segments[i].strings.get(*id);
                if best.is_none_or(|(_, bs)| s < bs) {
                    best = Some((i, s));
                }
            }
        }
        let Some((i, s)) = best else { break };
        if last != Some(s) {
            interner.id(s);
            last = Some(s);
        }
        let (_, mut it) = heads[i].take().expect("the head just read");
        heads[i] = it.next().map(|n| (n, it));
    }
    interner
}

/// One slice kind's per-segment unit-row lookup: each segment's rows are a
/// contiguous run of the kind's concatenated `rows` (the segments appended
/// in range order; `counts` are the runs' lengths, in segment order), so
/// its group keys resolve to the global row ids through its own interner.
type RowIdOf = HashMap<(u64, u32, u32), u32>;

fn unit_row_id_ofs(
    segments: &[Segment],
    rows: &[UnitRow],
    counts: &[usize],
    ids: &[u32],
) -> Result<Vec<RowIdOf>, String> {
    let mut id_ofs: Vec<RowIdOf> = Vec::with_capacity(segments.len());
    let mut off = 0usize;
    for (seg, &n) in segments.iter().zip(counts) {
        let rows = &rows[off..off + n];
        let mut id_of: RowIdOf = HashMap::with_capacity(rows.len());
        for (j, r) in rows.iter().enumerate() {
            let key = unit_key(&r.hash).ok_or_else(|| {
                format!("unit row hash {:?} is not 16 lowercase hex digits", r.hash)
            })?;
            let owner = seg
                .strings
                .lookup(&r.owner)
                .ok_or_else(|| format!("unit row owner {r:?}: the merge never interned it"))?;
            let file = seg
                .strings
                .lookup(&r.file)
                .ok_or_else(|| format!("unit row file {r:?}: the merge never interned it"))?;
            id_of.insert((key, owner, file), ids[off + j]);
        }
        off += n;
        id_ofs.push(id_of);
    }
    Ok(id_ofs)
}

/// One slice kind's names rows: every name id resolved into the GLOBAL
/// interner by string, the segments appended in range order to ONE stream
/// (`out`) — the serial merge's exact byte sequence.
fn stream_names_rows(
    segments: &[Segment],
    kind: &str,
    indexed: &BTreeMap<String, Vec<String>>,
    id_ofs: &[RowIdOf],
    interner: &NameInterner,
    out: &mut JsonlStream,
) -> Result<Vec<NameRowCompact>, String> {
    let mut compact: Vec<NameRowCompact> = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let Some(groups) = seg.names_groups.get(kind) else {
            continue;
        };
        if groups.is_empty() {
            continue;
        }
        compact.extend(name_rows_into(
            &seg.strings,
            groups,
            indexed,
            &id_ofs[i],
            |local| {
                interner.lookup(seg.strings.get(local)).ok_or_else(|| {
                    format!(
                        "names list: name {:?} is not interned",
                        seg.strings.get(local)
                    )
                })
            },
            &mut |bytes| out.take(bytes),
        )?);
    }
    Ok(compact)
}

pub fn finish_segments(meta: MergeMeta, mut segments: Vec<Segment>) -> Result<IndexFiles, String> {
    let first = segments.first();
    let indexed = first.map_or(BTreeMap::new(), |s| s.indexed.clone());
    let shard_keys = first.map_or(BTreeMap::new(), |s| s.shard_keys.clone());
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut written: BTreeMap<String, String> = BTreeMap::new();
    stream_tables(&meta, &mut segments, &indexed, &mut files, &mut written)?;
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
        shards: shard_keys,
        tables: files
            .iter()
            .map(|(name, bytes)| (name.clone(), hex_sha256(bytes)))
            .chain(
                written
                    .iter()
                    .map(|(name, sha)| (name.clone(), sha.clone())),
            )
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
    Ok(IndexFiles { files, written, id })
}

/// The finish's tables, produced into the sink as they come: with
/// `meta.staging` set, every table is written to disk as its rows finish
/// and recorded by sha (`written`) — only one table's bytes are alive at a
/// time; without it, every table is buffered in `files` for the caller to
/// write (the tests' path, today's shape). The row tables leave the
/// segments as each use takes them (`remove`/`mem::take` — the concat's
/// clone is gone) and the lookup tables are encoded one at a time through
/// the same sink. Same tables, same bytes, either way.
fn stream_tables(
    meta: &MergeMeta,
    segments: &mut [Segment],
    indexed: &BTreeMap<String, Vec<String>>,
    files: &mut BTreeMap<String, Vec<u8>>,
    written: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let mut sink = match &meta.staging {
        Some(dir) => {
            std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
            TableSink::Stream(dir, written)
        }
        None => TableSink::Buffer(files),
    };
    sink.whole("versions.json", pretty(indexed))?;
    let mut kinds: Vec<String> = segments
        .iter()
        .flat_map(|s| s.units.keys().cloned())
        .collect();
    kinds.sort();
    kinds.dedup();
    let (units, edges, unit_counts) = stream_row_tables(&mut sink, segments, &kinds)?;
    stream_names_tables(&mut sink, segments, &kinds, indexed, &units, &unit_counts)?;
    let literals: Vec<LiteralRow> = segments
        .iter_mut()
        .flat_map(|s| std::mem::take(&mut s.literals))
        .collect();
    {
        let mut put = |name: String, bytes: Vec<u8>| sink.whole(&name, bytes);
        humanify_core::libindex::table::lookup_files_into(
            &units,
            Some(&edges),
            &literals,
            &mut put,
        )?;
    }
    drop(units);
    drop(edges);
    // the provenance rows are the literals' last use: consumed row by row
    let mut s = sink.stream(LITERAL_FILES)?;
    for r in literals {
        s.row(&LiteralFileRow {
            owner: r.owner,
            file: r.file,
            versions: r.versions,
        })?;
    }
    sink.record(LITERAL_FILES, s)?;
    let helpers: Vec<HelperRow> = segments
        .iter_mut()
        .flat_map(|s| std::mem::take(&mut s.helpers))
        .collect();
    if !helpers.is_empty() {
        sink.jsonl(
            humanify_core::libindex::table::HELPERS_FILE,
            helpers.into_iter(),
        )?;
    }
    Ok(())
}

/// The row tables (units, edges) and the modules jsonl, per kind, streamed
/// through the sink as each kind's rows MOVE out of the segments. Returns
/// the concatenated unit and edge rows (the bin tables' and the names
/// phase's inputs) and, per kind, how many unit rows each segment held —
/// captured as the rows move, the names phase's per-segment runs in the
/// concatenated table. Edge rows of a kind with no unit rows are not
/// carried (today's shape).
type RowTables = (
    BTreeMap<String, Vec<UnitRow>>,
    BTreeMap<String, Vec<EdgeRow>>,
    BTreeMap<String, Vec<usize>>,
);

fn stream_row_tables(
    sink: &mut TableSink<'_>,
    segments: &mut [Segment],
    kinds: &[String],
) -> Result<RowTables, String> {
    let mut units: BTreeMap<String, Vec<UnitRow>> = BTreeMap::new();
    let mut edges: BTreeMap<String, Vec<EdgeRow>> = BTreeMap::new();
    let mut unit_counts: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for kind in kinds {
        // the modules table is provenance only: streamed straight out of
        // the segments, row by row, never concatenated
        let name = format!("modules.{kind}.jsonl");
        let mut s = sink.stream(&name)?;
        for seg in segments.iter_mut() {
            for r in seg.modules.remove(kind).unwrap_or_default() {
                s.row(&r)?;
            }
        }
        sink.record(&name, s)?;
        // the unit and edge rows MOVE out of the segments (each segment's
        // run of rows is contiguous); nothing is cloned
        let mut counts: Vec<usize> = Vec::with_capacity(segments.len());
        let mut urows: Vec<UnitRow> = Vec::new();
        for seg in segments.iter_mut() {
            let n = seg.units.get_mut(kind).map_or(0, |v| v.len());
            counts.push(n);
            if n > 0 {
                urows.append(seg.units.get_mut(kind).expect("length just read"));
            }
        }
        let mut erows: Vec<EdgeRow> = Vec::new();
        for seg in segments.iter_mut() {
            if let Some(rows) = seg.edges.get_mut(kind) {
                erows.append(rows);
            }
        }
        if !urows.is_empty() {
            let name = format!("units.{kind}.jsonl");
            let mut s = sink.stream(&name)?;
            for r in &urows {
                s.row(r)?;
            }
            sink.record(&name, s)?;
            let name = humanify_core::libindex::table::edge_rows_file(kind);
            let mut s = sink.stream(&name)?;
            for r in &erows {
                s.row(r)?;
            }
            sink.record(&name, s)?;
            units.insert(kind.clone(), urows);
            edges.insert(kind.clone(), erows);
            unit_counts.insert(kind.clone(), counts);
        }
    }
    Ok((units, edges, unit_counts))
}

/// The names tables (exp159, exp161; exp162 carries them per segment):
/// the DISTINCT names first — the segments' own interners k-way-merged by
/// string, the strings file's ids its positions, the serial merge's exact
/// assignment — then each segment's rows in range order, every name id
/// resolved by string into the global interner, the jsonl streamed
/// through the sink. A kind's groups drain as its rows go. The names bin
/// tables (strings/symbols/...) stream through the sink too, one at a
/// time, off the concatenated rows.
fn stream_names_tables(
    sink: &mut TableSink<'_>,
    segments: &mut [Segment],
    kinds: &[String],
    indexed: &BTreeMap<String, Vec<String>>,
    units: &BTreeMap<String, Vec<UnitRow>>,
    unit_counts: &BTreeMap<String, Vec<usize>>,
) -> Result<(), String> {
    let interner = merge_distinct_names(segments);
    let mut names: BTreeMap<String, Vec<NameRowCompact>> = BTreeMap::new();
    for kind in kinds {
        if segments
            .iter()
            .all(|s| s.names_groups.get(kind).is_none_or(|g| g.is_empty()))
        {
            continue;
        }
        let unit_rows = units
            .get(kind)
            .ok_or_else(|| format!("names rows for slice {kind}: the slice has no unit rows"))?;
        let ids = humanify_core::libindex::table::unit_row_ids(unit_rows)?;
        let id_ofs = unit_row_id_ofs(
            segments,
            unit_rows,
            unit_counts.get(kind).map_or(&[][..], Vec::as_slice),
            &ids,
        )?;
        let name = humanify_core::libindex::table::names_rows_file(kind);
        let mut s = sink.stream(&name)?;
        let compact = stream_names_rows(segments, kind, indexed, &id_ofs, &interner, &mut s)?;
        sink.record(&name, s)?;
        names.insert(kind.clone(), compact);
        for seg in segments.iter_mut() {
            seg.names_groups.remove(kind);
        }
    }
    {
        let mut put = |name: String, bytes: Vec<u8>| sink.whole(&name, bytes);
        humanify_core::libindex::table::names_lookup_files_into(
            units, &names, &interner, &mut put,
        )?;
    }
    Ok(())
}

/// Merge every shard at once (sorted into label order first) — the
/// streaming merge on a list, one whole-space [`Segment`].
pub fn merge(meta: MergeMeta, mut shards: Vec<Shard>) -> Result<IndexFiles, String> {
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    let mut m = Merger::default();
    for s in shards {
        m.add(s)?;
    }
    m.finish(meta)
}

// ------------------------------------------------------------------ exp162

/// What one worker streamed: its [`Segment`], and (a store stream) the
/// tasks it saw a shard of — identical in every worker, the build's
/// `found` flags.
pub struct WorkerMerge {
    pub found: Vec<bool>,
    segment: Segment,
}

/// The shards a parallel merge's workers stream: [`ShardFlow::Owned`] is
/// the tests' (an in-memory list; every worker iterates its own clone),
/// [`ShardFlow::Store`] is a build's (every worker reads the store
/// itself, in the same label order — the redundant read+parse is the
/// parallel merge's accepted cost; one reader distributing parsed rows
/// would serialise the parse).
pub enum ShardFlow<'a> {
    Owned(Vec<Shard>),
    Store {
        store: &'a Path,
        /// (label, task, inputs), in label order (a build's sorted list).
        refs: &'a [(String, usize, ShardInputs)],
        n_tasks: usize,
        /// The giant-shard gate's threshold: a shard this size or bigger
        /// is read, parsed and added ONE worker at a time (a shard of
        /// [`GIANT_SHARD_BYTES`]). 0 forces the gate on for every shard
        /// (the test's); [`u64::MAX`] turns it off.
        giant: u64,
    },
}

/// A shard this size or bigger goes through [`GIANT_GATE`] one worker at
/// a time: every worker streams every shard in the same order, so on
/// exp152's store (4 react-icons shards of 7.7-7.8GB, ~25GB of parse
/// each) all N workers would parse the same giant together — an OOM at
/// any real job count. One at a time: the peak is the workers' rows plus
/// ONE giant.
pub(crate) const GIANT_SHARD_BYTES: u64 = 1024 * 1024 * 1024;

/// The giant shards' read+parse+add, one worker at a time. A scheduling
/// detail only: no output decision reads it, the rows go to the same
/// workers in the same order gated or not, and the lock's hold is the
/// parse of one shard (minutes on a giant) — the tradeoff that keeps the
/// parallel merge's memory at rows-plus-one-giant instead of rows-times-N.
static GIANT_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// One worker's shard stream: the same shards every worker sees, in the
/// same (label, key) order — the order the rows' contents depend on (a
/// unit group keeps the FIRST shard's nodes and content).
enum WorkerStream<'a> {
    Owned(std::vec::IntoIter<Shard>),
    Store {
        store: &'a Path,
        refs: std::slice::Iter<'a, (String, usize, ShardInputs)>,
        found: Vec<bool>,
        giant: u64,
    },
}

impl WorkerStream<'_> {
    /// The next shard in label order, read into `merger` (true: a shard
    /// was added; false: done). A store stream records the task of every
    /// shard it reads, and takes the [`GIANT_GATE`] across a giant
    /// shard's read+parse+add so one worker at a time holds a giant.
    fn next_into(&mut self, merger: &mut Merger) -> Result<bool, String> {
        match self {
            WorkerStream::Owned(iter) => match iter.next() {
                Some(shard) => {
                    merger.add(shard)?;
                    Ok(true)
                }
                None => Ok(false),
            },
            WorkerStream::Store {
                store,
                refs,
                found,
                giant,
            } => {
                for (_, task, inputs) in refs.by_ref() {
                    // the gate is taken on the shard's SIZE, before its
                    // read, and held across the add — the guard lives to
                    // the return, after `merger.add`
                    let gate = (*giant > 0
                        && std::fs::metadata(shard_path(
                            store,
                            &inputs.kind.name(),
                            &inputs.package,
                            &inputs.version,
                        ))
                        .is_ok_and(|m| m.len() >= *giant))
                    .then(|| {
                        GIANT_GATE
                            .lock()
                            .map_err(|_| "the giant-shard gate lock is poisoned".to_string())
                    })
                    .transpose()?;
                    let Some(shard) = read_current(store, inputs) else {
                        drop(gate);
                        continue;
                    };
                    if let Some(seen) = found.get_mut(*task) {
                        *seen = true;
                    }
                    merger.add(shard)?;
                    drop(gate);
                    return Ok(true);
                }
                Ok(false)
            }
        }
    }

    /// The tasks this stream saw a shard of (a store stream; an in-memory
    /// stream's is empty — the tests' shape, whose caller does not read
    /// it).
    fn into_found(self) -> Vec<bool> {
        match self {
            WorkerStream::Owned(_) => Vec::new(),
            WorkerStream::Store { found, .. } => found,
        }
    }
}

impl ShardFlow<'_> {
    /// A fresh stream of the same shards, for one worker.
    fn worker(&self) -> WorkerStream<'_> {
        match self {
            ShardFlow::Owned(shards) => WorkerStream::Owned(shards.clone().into_iter()),
            ShardFlow::Store {
                store,
                refs,
                n_tasks,
                giant,
            } => WorkerStream::Store {
                store,
                refs: refs.iter(),
                found: vec![false; *n_tasks],
                giant: *giant,
            },
        }
    }
}

/// Stream every shard into [`key_ranges`]`(jobs)` workers (exp162): each
/// worker streams ALL shards in label order and keeps only its range's
/// rows ([`Merger::filtered`]), then emits its [`Segment`]. The workers
/// run on `crate::par` (rule 1, docs/concurrency.md): pure per-range
/// maps whose results come back in RANGE order — the order the segments
/// concatenate in ([`finish_workers`]), which is what keeps the
/// hash-map iteration order out of every output decision the house rule
/// names.
pub fn stream_parallel(flow: &ShardFlow, jobs: usize) -> Result<Vec<WorkerMerge>, String> {
    let workers = humanify_core::par::map_ordered(&key_ranges(jobs), |range| {
        let mut merger = Merger::filtered(*range);
        let mut stream = flow.worker();
        while stream.next_into(&mut merger)? {}
        Ok(WorkerMerge {
            found: stream.into_found(),
            segment: merger.segment(),
        })
    });
    workers.into_iter().collect()
}

/// The parallel merge's index, from the workers [`stream_parallel`]
/// returned: their segments concatenated in range order.
pub fn finish_workers(meta: MergeMeta, workers: Vec<WorkerMerge>) -> Result<IndexFiles, String> {
    finish_segments(meta, workers.into_iter().map(|w| w.segment).collect())
}

/// The parallel merge on a shard list (the tests' shape): sorted into
/// label order first, one [`Segment`] per [`key_ranges`] worker,
/// concatenated in range order. Byte-identical to [`merge`] at every job
/// count — the contract, tested.
pub fn merge_parallel(
    meta: MergeMeta,
    shards: Vec<Shard>,
    jobs: usize,
) -> Result<IndexFiles, String> {
    let mut shards = shards;
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    let workers = stream_parallel(&ShardFlow::Owned(shards), jobs)?;
    finish_workers(meta, workers)
}
