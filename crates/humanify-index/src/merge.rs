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
    INDEX_FORMAT, LiteralFileRow, LiteralRow, Manifest, ModuleRow, PackageRecord, RefusedRecord,
    RegistrySnapshot, SliceRecord, UnitRow, fingerprint_versions, sort_versions, version_ranges,
};
use serde::Serialize;

use crate::shard::{Shard, hex_sha256};

/// The stop-lists shipped with every index.
pub const STOPLISTS_JSON: &str = include_str!("../data/stoplists.json");
/// The package families shipped with every index.
pub const FAMILIES_JSON: &str = humanify_core::libindex::families::FAMILIES_JSON;

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
        for u in &s.units {
            seen.entry(u.owner.clone())
                .or_default()
                .insert(u.version.clone());
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

/// `units.<slice>.jsonl`: one row per (hash, owner, file), with the
/// smallest bundled module the unit was seen in.
fn unit_table(shards: &[&Shard], indexed: &BTreeMap<String, Vec<String>>) -> Vec<UnitRow> {
    // (hash, owner, file) → (nodes, content, smallest module, versions)
    type Seen = (u32, u32, u64, Vec<String>);
    let mut groups: BTreeMap<(String, String, String), Seen> = BTreeMap::new();
    for s in shards {
        for u in &s.units {
            let g = groups
                .entry((u.hash.clone(), u.owner.clone(), u.file.clone()))
                .or_insert((u.nodes, u.content, u.module_bytes, Vec::new()));
            g.2 = g.2.min(u.module_bytes);
            g.3.push(u.version.clone());
        }
    }
    groups
        .into_iter()
        .map(
            |((hash, owner, file), (nodes, content, module_bytes, versions))| UnitRow {
                versions: version_ranges(&indexed[&owner], &versions),
                hash,
                owner,
                file,
                nodes,
                content,
                module_bytes,
            },
        )
        .collect()
}

/// The literal file rows: one per (owner, file, literal set).
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

/// The literal table's provenance file: each literal file's owner, path
/// and versions, row for row with `literals.bin`'s files.
pub const LITERAL_FILES: &str = "literals.files.jsonl";

/// Merge `input` into the index's files: the row tables (provenance, read
/// by instruments) and the lookup tables a run reads
/// (`humanify_core::libindex::table`), derived from the same rows.
pub fn merge(input: MergeInput) -> Result<IndexFiles, String> {
    let stop: serde_json::Value = serde_json::from_str(STOPLISTS_JSON).unwrap_or_default();
    // Shards in a fixed order, helper owners' rows dropped. Taken, not
    // copied: the shards are the merge's whole working set.
    let MergeInput {
        as_of,
        registry,
        slices,
        packages,
        refused,
        mut shards,
    } = input;
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    for s in &mut shards {
        s.modules.retain(|m| !is_helper(&m.owner, &stop));
        s.literals.retain(|l| !is_helper(&l.owner, &stop));
        s.units.retain(|u| !is_helper(&u.owner, &stop));
    }
    let all: Vec<&Shard> = shards.iter().collect();
    let indexed = owner_versions(&all);

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert("versions.json".into(), pretty(&indexed));
    let kinds: BTreeSet<&str> = shards.iter().map(|s| s.kind.as_str()).collect();
    let mut literals: Vec<LiteralRow> = Vec::new();
    let mut units: BTreeMap<String, Vec<UnitRow>> = BTreeMap::new();
    for kind in kinds {
        let of_kind: Vec<&Shard> = all.iter().copied().filter(|s| s.kind == kind).collect();
        if kind == "literals" {
            literals = literal_table(&of_kind, &indexed);
        } else {
            files.insert(
                format!("modules.{kind}.jsonl"),
                jsonl(&module_table(&of_kind, &indexed)),
            );
            let rows = unit_table(&of_kind, &indexed);
            if !rows.is_empty() {
                files.insert(format!("units.{kind}.jsonl"), jsonl(&rows));
                units.insert(kind.to_string(), rows);
            }
        }
    }
    files.extend(humanify_core::libindex::table::lookup_files(
        &units, &literals,
    )?);
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
        as_of,
        fingerprints: fingerprint_versions(),
        registry,
        slices,
        packages,
        refused,
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
    Ok(IndexFiles { files, id })
}
