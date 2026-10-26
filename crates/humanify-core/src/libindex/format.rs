//! The index's on-disk shapes (`humanify-library-index/1`,
//! docs/design/library-index.md §4) — written by the offline builder, read
//! by the pipeline, so one set of types.
//!
//! Every fingerprint is stored ONCE per owner package with the versions it
//! appears in, as RANGES over the owner's INDEXED versions in semver order
//! ([`version_ranges`]): `["1.0.0","1.2.0"]` is every indexed version from
//! 1.0.0 to 1.2.0. A range says nothing about a version the index never
//! built; `versions.json` lists the indexed ones so a reader can expand it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::hash::forms::{LIBRARY_LITERALS_VERSION, LIBRARY_UNITS, VENDOR_CONTENT};

/// The format tag this binary writes: `/4` adds the edge tables
/// (`edges.<slice>.bin` + `edges.<slice>.jsonl`, and the flat
/// `callee-prints.<slice>.bin` — exp151 + exp155).
pub const INDEX_FORMAT: &str = "humanify-library-index/4";

/// The one older format this binary still reads: the same tables without
/// edges, so its store answers every edge read `NotRecorded`.
pub const INDEX_FORMAT_NO_EDGES: &str = "humanify-library-index/3";

/// The edge ROWS' version (exp151, unchanged in exp155): per unit row, the
/// WHOLE prints of the statement units its statement references in the
/// bundle's reference graph (`graph::references`, factory bodies cut),
/// split by the callee's owner into INSIDE (the row's own package) and
/// OUT-OF (another package); a callee with no print (a container) or no
/// owner is not recorded. In every module shard's key — the flat
/// callee-print table an `edges/2` index adds is DERIVED from these rows at
/// merge time, so a shard re-keys only when its rows do (identical rows,
/// identical key: an index rebuilt on a reused store re-merges without one
/// fingerprint).
pub const LIBRARY_EDGE_ROWS_VERSION: &str = "edges/1";

/// The readable edge data's version (exp155 bumps it to `edges/2`): what a
/// `/4` index's edge data answers IN TOTAL — the edge rows
/// ([`LIBRARY_EDGE_ROWS_VERSION`]) plus the flat callee-print table
/// (`callee-prints.<slice>.bin`, derived from the same rows at merge
/// time): every DISTINCT callee print any row records, with the owners
/// whose rows record it, so a callee that exists in the edge data WITHOUT
/// owning a unit row (under the 20-node row floor, a stop-listed helper, a
/// row dropped for evidence) is queryable by key. This is what lets the
/// call-context fit tell "the index does not know this print" (version
/// skew, print drift: silence, not evidence against the family) from "the
/// family's rows lack it but other families record it" (refutation), which
/// edges/1 could not (exp153's census: 9.1–9.2 MB of RIGHT claim bytes per
/// bun-dev set refuted through that gap). The manifest's
/// `fingerprints.edges`; a `/4` manifest stamped differently is refused
/// ([`check_manifest_stamps`]).
pub const LIBRARY_EDGES_VERSION: &str = "edges/2";

/// The build recipe's version (`humanify-index`'s `recipe` module: every
/// rule of how a slice's bundles are made). Recorded per slice in the
/// manifest and in every shard key; the loader refuses another.
pub const RECIPE_VERSION: &str = "recipe/4";

/// How a refusal says to fix it.
const REBUILD: &str = "rebuild the index with this binary's `humanify-index build` (the shard store rebuilds only what changed)";

/// A semantic version (precedence per semver 2.0: build metadata ignored,
/// a prerelease before its release, prerelease identifiers numeric < text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Semver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pre: Vec<PreId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PreId {
    Num(u64),
    Text(String),
}

impl Semver {
    /// `MAJOR.MINOR.PATCH[-pre][+build]`; None for anything else.
    pub fn parse(v: &str) -> Option<Semver> {
        let v = v.split_once('+').map_or(v, |(core, _)| core);
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        let mut parts = core.split('.');
        let mut num = || -> Option<u64> {
            let p = parts.next()?;
            (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        };
        let (major, minor, patch) = (num()?, num()?, num()?);
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(p) => p
                .split('.')
                .map(|id| match id.parse::<u64>() {
                    Ok(n) if id.bytes().all(|b| b.is_ascii_digit()) => PreId::Num(n),
                    _ => PreId::Text(id.to_string()),
                })
                .collect(),
        };
        Some(Semver {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Version strings in semver order; one that does not parse sorts after
/// every one that does, by text (deterministic either way).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (Semver::parse(a), Semver::parse(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(b),
    }
}

pub fn sort_versions(versions: &mut [String]) {
    versions.sort_by(|a, b| compare_versions(a, b));
}

/// The ranges `present` covers over `indexed` (the owner's indexed
/// versions, in semver order): maximal runs of consecutive indexed
/// versions. A present version that is not indexed is ignored.
pub fn version_ranges(indexed: &[String], present: &[String]) -> Vec<(String, String)> {
    let present: BTreeSet<&str> = present.iter().map(String::as_str).collect();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for (i, v) in indexed.iter().enumerate() {
        if present.contains(v.as_str()) {
            run = Some(run.map_or((i, i), |(s, _)| (s, i)));
        } else if let Some((s, e)) = run.take() {
            out.push((indexed[s].clone(), indexed[e].clone()));
        }
    }
    if let Some((s, e)) = run {
        out.push((indexed[s].clone(), indexed[e].clone()));
    }
    out
}

/// One row of `modules.<slice>.jsonl`: a whole-module key held by one
/// owner package (a key held by two owners is two rows — collisions are
/// lookup-time evidence, never dropped at build time).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRow {
    pub key: String,
    pub owner: String,
    /// The owner's files the module came from (sorted; one in the usual
    /// case, more when two files bundle to the same key).
    pub files: Vec<String>,
    pub versions: Vec<(String, String)>,
}

/// One row of `units.<slice>.jsonl`: a statement or function unit's hash
/// held by one owner file (a hash held by two owners is two rows). The
/// smallest bundled module the unit was seen in is kept for the lookup's
/// specificity rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitRow {
    pub hash: String,
    pub owner: String,
    pub file: String,
    pub nodes: u32,
    pub content: u32,
    pub module_bytes: u64,
    pub versions: Vec<(String, String)>,
}

/// One row of `edges.<slice>.jsonl` (format `/4`, exp151): the callees a
/// unit row records ([`LIBRARY_EDGES_VERSION`]), over every version the
/// row was seen in (a union: an index edge a bundle lacks says nothing,
/// docs/matching-architecture.md §4). Keyed like its [`UnitRow`] (hash,
/// owner, file); a row with no callee has no edge row. Provenance for
/// instruments; a run reads `edges.<slice>.bin`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRow {
    pub hash: String,
    pub owner: String,
    pub file: String,
    /// The INSIDE callees' prints (the row's own package), sorted.
    pub inside: Vec<String>,
    /// The OUT-OF callees (another package's units), sorted by (owner,
    /// hash).
    pub out_of: Vec<Callee>,
}

/// An OUT-OF callee: the package owning it and its print.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Callee {
    pub owner: String,
    pub hash: String,
}

/// One print of the index's HELPER table (exp129, `helpers.jsonl`): a
/// unit hash a helper SOURCE's own build holds (tslib, @babel/runtime,
/// @swc/helpers: the stop-listed packages whose code is the transpiler
/// helpers every compiled package inlines), with every helper package
/// holding it, over all slices. Those packages leave the unit tables at
/// merge time; their prints stay here, so a lookup can tell a package's own
/// code from the canonical helper code its dist inlined.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRow {
    pub hash: String,
    pub owners: Vec<String>,
}

/// One literal FILE row as the merge computes it: one source file's
/// literal set, per owner, with the versions in which that file had
/// exactly this set. On disk it is split (`humanify-library-index/3`): the
/// literal sets go to the lookup table (`table::LiteralTable`, file id =
/// row position) and the rest to `literals.files.jsonl`
/// ([`LiteralFileRow`]), row for row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiteralRow {
    pub owner: String,
    pub file: String,
    pub lits: Vec<String>,
    pub versions: Vec<(String, String)>,
}

/// One row of `literals.files.jsonl`: literal file `i`'s owner, path and
/// versions (provenance; its literal set is file `i` of `literals.bin`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiteralFileRow {
    pub owner: String,
    pub file: String,
    pub versions: Vec<(String, String)>,
}

/// The fingerprint versions an index was built with; the pipeline refuses
/// an index whose stamp differs from its own (a half-matching index would
/// silently half-name — the vendor record's `hashVersion` rule). EVERY
/// field is checked ([`Manifest::check_compatible`]); a stamp this binary
/// does not know, or one the manifest lacks, does not deserialize.
///
/// - `module`: the whole-module print, the vendor content key (its
///   serializer form and the known-globals set are part of that version);
/// - `literals`: the literal extractor;
/// - `units`: the statement unit model AND its serializer form (the match
///   key with private names blinded) — `units_test`'s pin fails when either
///   changes under a constant version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FingerprintVersions {
    pub module: String,
    pub literals: String,
    pub units: String,
    /// The edge rows ([`LIBRARY_EDGES_VERSION`]); absent in a
    /// [`INDEX_FORMAT_NO_EDGES`] index, which has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<String>,
}

/// This build's stamp.
pub fn fingerprint_versions() -> FingerprintVersions {
    FingerprintVersions {
        module: VENDOR_CONTENT.stamp(),
        literals: format!("literals/{LIBRARY_LITERALS_VERSION}"),
        units: LIBRARY_UNITS.stamp(),
        edges: Some(LIBRARY_EDGES_VERSION.to_string()),
    }
}

/// The stamps an index of `format` must carry to be read by this binary
/// (field → spelling), None for a format it does not read: this build's,
/// and for [`INDEX_FORMAT_NO_EDGES`] the same without `edges`.
fn readable_stamps(format: &str) -> Option<BTreeMap<&'static str, String>> {
    let f = fingerprint_versions();
    let mut out: BTreeMap<&'static str, String> = [
        ("module", f.module),
        ("literals", f.literals),
        ("units", f.units),
    ]
    .into_iter()
    .collect();
    match format {
        INDEX_FORMAT => {
            out.insert("edges", f.edges.expect("this build stamps edges"));
            Some(out)
        }
        INDEX_FORMAT_NO_EDGES => Some(out),
        _ => None,
    }
}

/// Why `theirs` (field → stamp) is not what a `format` index must carry,
/// one entry per field that differs; a format this binary does not read
/// is named, and its stamps compared with this build's.
fn stamp_mismatches(format: &str, theirs: &BTreeMap<String, String>) -> Vec<String> {
    let mut wrong = Vec::new();
    let ours = readable_stamps(format).unwrap_or_else(|| {
        wrong.push(format!(
            "format {format} (this binary reads {INDEX_FORMAT} and {INDEX_FORMAT_NO_EDGES})"
        ));
        readable_stamps(INDEX_FORMAT).expect("this build's format")
    });
    let keys: BTreeSet<&str> = ours
        .keys()
        .copied()
        .chain(theirs.keys().map(String::as_str))
        .collect();
    let show = |v: Option<&str>| v.unwrap_or("(none)").to_string();
    wrong.extend(keys.into_iter().filter_map(|k| {
        let (t, o) = (
            theirs.get(k).map(String::as_str),
            ours.get(k).map(String::as_str),
        );
        (t != o).then(|| format!("{k} {} (this binary reads {})", show(t), show(o)))
    }));
    wrong
}

/// The registry snapshot an index was planned from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySnapshot {
    pub earliest_fetch: Option<String>,
    pub latest_fetch: Option<String>,
    /// package → sha256 of its recorded packument file.
    pub packuments: BTreeMap<String, String>,
}

/// One recipe slice as built (`humanify-library-index/2`: an index is a
/// set of recipe slices, docs/library-matching-config.md §3): the recipe,
/// the bundler binary's version, and whether that is newer than the as-of
/// date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SliceRecord {
    /// The slice name ([`super::recipe::Recipe::slice_name`]): its tables
    /// are `units.<slice>.jsonl` / `modules.<slice>.jsonl`.
    pub slice: String,
    /// The recipe's spelling (`bun:target=bun,format=cjs,minify`).
    pub recipe: String,
    /// The build rules' version ([`RECIPE_VERSION`]) it was built under.
    pub recipe_version: String,
    /// The slice a bundle of this bundler reads when it does not record
    /// its target (the builder marks the first recipe of each bundler).
    pub default_for_bundler: bool,
    /// The bundler's npm package and its version.
    pub bundler: String,
    pub version: String,
    pub published: Option<String>,
    pub newest_at_as_of: Option<String>,
    pub bundler_after_as_of: bool,
}

/// One indexed root package version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRecord {
    pub name: String,
    pub version: String,
    pub role: String,
    pub published: String,
}

/// A version the plan refused, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusedRecord {
    pub name: String,
    pub version: String,
    pub reason: String,
}

/// `manifest.json`. Its sha256 is the index id a run records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub as_of: String,
    pub fingerprints: FingerprintVersions,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
    pub packages: Vec<PackageRecord>,
    pub refused: Vec<RefusedRecord>,
    /// `<kind>/<pkg>@<version>` → shard key.
    pub shards: BTreeMap<String, String>,
    /// table file → sha256.
    pub tables: BTreeMap<String, String>,
    /// data file → sha256.
    pub data: BTreeMap<String, String>,
}

/// The stamps of a manifest, read leniently: an index from another era
/// may not deserialize as this binary's [`Manifest`], and its refusal must
/// still name the stamps that differ (not a serde "missing field").
#[derive(Deserialize)]
struct Stamps {
    #[serde(default)]
    format: String,
    #[serde(default)]
    fingerprints: BTreeMap<String, String>,
}

/// The manifest TEXT's format and fingerprint stamps against this
/// binary's, before the strict parse: Err names every one that differs
/// (an unknown or missing stamp included) and how to rebuild.
pub fn check_manifest_stamps(text: &str) -> Result<(), String> {
    let theirs: Stamps = serde_json::from_str(text).map_err(|e| format!("manifest.json: {e}"))?;
    let wrong = stamp_mismatches(&theirs.format, &theirs.fingerprints);
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(format!("built with {}; {REBUILD}", wrong.join(", ")))
    }
}

impl Manifest {
    /// Can this binary read the index? Every compatibility stamp is
    /// compared — the format, each fingerprint version, every slice's
    /// recipe version and spelling — and every mismatch is named with both
    /// spellings (fail early: a half-matching index would silently
    /// half-claim). Which slice a bundle reads is [`Manifest::recipes`] +
    /// `recipe::choose_slice`, a separate question.
    pub fn check_compatible(&self) -> Result<(), String> {
        let f = &self.fingerprints;
        let stamps: BTreeMap<String, String> = [
            ("module", Some(&f.module)),
            ("literals", Some(&f.literals)),
            ("units", Some(&f.units)),
            ("edges", f.edges.as_ref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| Some((k.to_string(), v?.clone())))
        .collect();
        let mut wrong = stamp_mismatches(&self.format, &stamps);
        let mut differ = |field: &str, theirs: &str, ours: &str| {
            if theirs != ours {
                wrong.push(format!("{field} {theirs} (this binary reads {ours})"));
            }
        };
        for s in &self.slices {
            differ(
                &format!("recipe version (slice {})", s.slice),
                &s.recipe_version,
                RECIPE_VERSION,
            );
        }
        if let Err(e) = self.recipes() {
            wrong.push(e);
        }
        if wrong.is_empty() {
            Ok(())
        } else {
            Err(format!("built with {}; {REBUILD}", wrong.join(", ")))
        }
    }

    /// Does the index hold edge tables (format [`INDEX_FORMAT`])?
    pub fn has_edges(&self) -> bool {
        self.format == INDEX_FORMAT
    }

    /// The index's recipes in manifest order, each with its
    /// default-for-bundler mark; Err when a slice's recipe does not parse
    /// or its name is not its recipe's.
    pub fn recipes(&self) -> Result<Vec<(super::recipe::Recipe, bool)>, String> {
        self.slices
            .iter()
            .map(|s| {
                let r = super::recipe::Recipe::parse(&s.recipe)?;
                if r.slice_name() != s.slice {
                    return Err(format!(
                        "slice {} records recipe {} (whose slice is {})",
                        s.slice,
                        s.recipe,
                        r.slice_name()
                    ));
                }
                Ok((r, s.default_for_bundler))
            })
            .collect()
    }
}
