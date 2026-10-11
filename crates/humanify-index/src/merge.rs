//! The merge: the shards of the selected versions → one index directory
//! (docs/design/library-index.md §4-5).
//!
//! Each fingerprint is stored ONCE per owner with the version ranges it
//! appears in (over the owner's indexed versions, semver order); a key
//! held by two owners stays two rows (collisions are lookup evidence).
//! The stop-listed helper packages are dropped. Every table is sorted and
//! the manifest's maps are ordered, so the same shards give the same
//! bytes in any order; the INDEX ID is the manifest's sha256.

use std::collections::{BTreeMap, BTreeSet};

use humanify_core::libindex::format::{
    INDEX_FORMAT, LiteralRow, Manifest, ModuleRow, PackageRecord, RefusedRecord, RegistrySnapshot,
    SliceRecord, fingerprint_versions, sort_versions, version_ranges,
};
use serde::Serialize;

use crate::shard::{Shard, hex_sha256};

/// The stop-lists shipped with every index.
pub const STOPLISTS_JSON: &str = include_str!("../data/stoplists.json");
/// The package families shipped with every index.
pub const FAMILIES_JSON: &str = include_str!("../data/families.json");

/// Everything a merge reads (all of it already decided by the plan).
#[derive(Default)]
pub struct MergeInput {
    pub as_of: String,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
    pub packages: Vec<PackageRecord>,
    pub refused: Vec<RefusedRecord>,
    pub shards: Vec<Shard>,
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

/// The owners' indexed versions, semver order.
fn owner_versions(shards: &[&Shard]) -> BTreeMap<String, Vec<String>> {
    let mut seen: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for s in shards {
        for m in &s.modules {
            seen.entry(m.owner.clone())
                .or_default()
                .insert(m.version.clone());
        }
        for l in &s.literals {
            seen.entry(l.owner.clone())
                .or_default()
                .insert(l.version.clone());
        }
    }
    seen.into_iter()
        .map(|(owner, vs)| {
            let mut vs: Vec<String> = vs.into_iter().collect();
            sort_versions(&mut vs);
            (owner, vs)
        })
        .collect()
}

/// `modules.<slice>.jsonl` for one slice's shards.
fn module_table(shards: &[&Shard], indexed: &BTreeMap<String, Vec<String>>) -> Vec<ModuleRow> {
    // (key, owner) → (files, versions)
    type Seen = (BTreeSet<String>, Vec<String>);
    let mut groups: BTreeMap<(String, String), Seen> = BTreeMap::new();
    for s in shards {
        for m in &s.modules {
            let g = groups.entry((m.key.clone(), m.owner.clone())).or_default();
            g.0.insert(m.file.clone());
            g.1.push(m.version.clone());
        }
    }
    groups
        .into_iter()
        .map(|((key, owner), (files, versions))| ModuleRow {
            versions: version_ranges(&indexed[&owner], &versions),
            files: files.into_iter().collect(),
            key,
            owner,
        })
        .collect()
}

/// `literals.jsonl`: one row per (owner, file, literal set).
fn literal_table(shards: &[&Shard], indexed: &BTreeMap<String, Vec<String>>) -> Vec<LiteralRow> {
    let mut groups: BTreeMap<(String, String, Vec<String>), Vec<String>> = BTreeMap::new();
    for s in shards {
        for l in &s.literals {
            groups
                .entry((l.owner.clone(), l.file.clone(), l.lits.clone()))
                .or_default()
                .push(l.version.clone());
        }
    }
    groups
        .into_iter()
        .map(|((owner, file, lits), versions)| LiteralRow {
            versions: version_ranges(&indexed[&owner], &versions),
            owner,
            file,
            lits,
        })
        .collect()
}

/// Merge `input` into the index's files.
pub fn merge(input: &MergeInput) -> IndexFiles {
    let stop: serde_json::Value = serde_json::from_str(STOPLISTS_JSON).unwrap_or_default();
    // Shards in a fixed order, helper owners' rows dropped.
    let mut shards: Vec<Shard> = input.shards.clone();
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    for s in &mut shards {
        s.modules.retain(|m| !is_helper(&m.owner, &stop));
        s.literals.retain(|l| !is_helper(&l.owner, &stop));
    }
    let all: Vec<&Shard> = shards.iter().collect();
    let indexed = owner_versions(&all);

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert("versions.json".into(), pretty(&indexed));
    let kinds: BTreeSet<&str> = shards.iter().map(|s| s.kind.as_str()).collect();
    for kind in kinds {
        let of_kind: Vec<&Shard> = all.iter().copied().filter(|s| s.kind == kind).collect();
        if kind == "literals" {
            files.insert(
                "literals.jsonl".into(),
                jsonl(&literal_table(&of_kind, &indexed)),
            );
        } else {
            files.insert(
                format!("modules.{kind}.jsonl"),
                jsonl(&module_table(&of_kind, &indexed)),
            );
        }
    }
    let data: BTreeMap<String, &str> = [
        ("families.json".to_string(), FAMILIES_JSON),
        ("stoplists.json".to_string(), STOPLISTS_JSON),
    ]
    .into_iter()
    .collect();
    let manifest = Manifest {
        format: INDEX_FORMAT.to_string(),
        as_of: input.as_of.clone(),
        fingerprints: fingerprint_versions(),
        registry: input.registry.clone(),
        slices: input.slices.clone(),
        packages: input.packages.clone(),
        refused: input.refused.clone(),
        shards: shards.iter().map(|s| (s.label(), s.key.clone())).collect(),
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
    IndexFiles { files, id }
}
