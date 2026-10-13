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

/// The format tag in every manifest.
pub const INDEX_FORMAT: &str = "humanify-library-index/1";

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

/// One row of `literals.jsonl`: one source file's literal set, per owner,
/// with the versions in which that file had exactly this set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiteralRow {
    pub owner: String,
    pub file: String,
    pub lits: Vec<String>,
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
}

/// This build's stamp.
pub fn fingerprint_versions() -> FingerprintVersions {
    FingerprintVersions {
        module: VENDOR_CONTENT.stamp(),
        literals: format!("literals/{LIBRARY_LITERALS_VERSION}"),
        units: LIBRARY_UNITS.stamp(),
    }
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

/// A bundler slice as built: which bundler binary, and whether it is newer
/// than the as-of date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceRecord {
    pub slice: String,
    pub bundler: String,
    pub version: String,
    /// The build recipe's version ([`RECIPE_VERSION`]) this slice was
    /// built under.
    pub recipe: String,
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

impl Manifest {
    /// Can this binary read the index? Every compatibility stamp is
    /// compared — the format, each fingerprint version, every slice's
    /// recipe version — and every mismatch is named with both spellings
    /// (fail early: a half-matching index would silently half-claim).
    /// Whether the index HAS a bundle's slice is the loader's question.
    pub fn check_compatible(&self) -> Result<(), String> {
        let ours = fingerprint_versions();
        let mut wrong: Vec<String> = Vec::new();
        let mut differ = |field: &str, theirs: &str, ours: &str| {
            if theirs != ours {
                wrong.push(format!("{field} {theirs} (this binary reads {ours})"));
            }
        };
        differ("format", &self.format, INDEX_FORMAT);
        differ("module", &self.fingerprints.module, &ours.module);
        differ("literals", &self.fingerprints.literals, &ours.literals);
        differ("units", &self.fingerprints.units, &ours.units);
        for s in &self.slices {
            differ(
                &format!("recipe (slice {})", s.slice),
                &s.recipe,
                RECIPE_VERSION,
            );
        }
        if wrong.is_empty() {
            Ok(())
        } else {
            Err(format!("built with {}; {REBUILD}", wrong.join(", ")))
        }
    }
}
