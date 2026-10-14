//! Index-time origin de-duplication (exp110; docs/design/library-index.md
//! §4, "Copies"): a package that INLINES a dependency into its own `dist/`
//! (rspack's pre-bundled tinypool, mermaid's chunks of cytoscape and elk,
//! ora's yoctocolors) has unit rows for that code under its OWN name, and a
//! bundle holding the real dependency is then claimed as the inliner. The
//! merge keeps such a print only under its ORIGIN.
//!
//! **The origin test.** A print held by owners `A` and `B` is `A`'s, not
//! `B`'s, when `B` depends on `A` and `A` does not depend on `B`, and the
//! two are not one package family. "Depends" is read from the build store
//! itself, two ways:
//!
//! - **installed**: `B`'s own builds bundled `A` from `node_modules` — a
//!   shard of `B` (any slice, or its literal shard, which reads every file
//!   the builds contain) has rows OWNED by `A` (the shard owns rows by
//!   their nearest package.json, so a dependency's rows are the
//!   dependency's);
//! - **declared**: `B`'s published package.json names `A` in any
//!   dependency field, devDependencies included (a pre-bundled copy is
//!   usually of a dev dependency that is never installed). The builder
//!   records each version's names once, from its install, in
//!   `store/declared/` ([`read_or_record_declared`]).
//!
//! A dependency's code can appear in its dependent's dist; the reverse
//! needs a cycle. When EVERY owner of a print depends on another one (a
//! cycle), none is the origin and every row stays.
//!
//! **What it never touches.** A print held by more owners than the lookup
//! reads ([`MAX_OWNERS`], `LookupRules::max_packages`) is generic code the
//! lookup refuses as ambiguous; removing an owner must not make it
//! claimable, so it is left alone. One family's members (one scope, a
//! listed family, a metadata-derived link) both own their shared source.
//! Only the unit tables are de-duplicated: the module tables are
//! provenance, and literal sets are per file.
//!
//! The decision is recorded in `dedup.json` (rows dropped per slice, and
//! every (inliner, origin) pair by rows), hashed into the manifest.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use humanify_core::libindex::families::PackageFamilies;
use serde::{Deserialize, Serialize};

/// The rule's name, recorded in `dedup.json`.
pub const ORIGIN_RULE: &str = "origin/1";

/// The index file recording what the rule dropped.
pub const DEDUP_FILE: &str = "dedup.json";

/// A print held by more owners than this is left alone (the lookup's
/// default `max_packages`).
pub const MAX_OWNERS: usize = 3;

/// The origin's copy is the SMALLER context: an owner is an origin only
/// when its smallest module holding the print is at most `factor` x the
/// inliner's + `slack` bytes — the lookup's specificity bound
/// (`LookupRules::specificity`). A tool a package merely develops with
/// (micromark devDepends on prettier, whose 300 KB markdown plugin inlines
/// micromark) holds the print in a far bigger module and is no origin.
pub const ORIGIN_SPECIFICITY: (u64, u64) = (4, 4096);

/// The package.json fields whose names are a dependency declaration.
const DEPENDENCY_FIELDS: [&str; 6] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
    "bundleDependencies",
    "bundledDependencies",
];

/// Every package name `package_json` declares in a dependency field, in
/// field order, each once.
pub fn declared_names(package_json: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for field in DEPENDENCY_FIELDS {
        let names: Vec<&str> = match package_json.get(field) {
            Some(serde_json::Value::Object(m)) => m.keys().map(String::as_str).collect(),
            Some(serde_json::Value::Array(a)) => a.iter().filter_map(|v| v.as_str()).collect(),
            _ => Vec::new(),
        };
        for n in names {
            if !out.iter().any(|o| o == n) {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// One version's declared names, as the store keeps them.
#[derive(Serialize, Deserialize)]
struct DeclaredRecord {
    package: String,
    version: String,
    names: Vec<String>,
}

fn declared_path(store: &Path, package: &str, version: &str) -> PathBuf {
    store
        .join("declared")
        .join(format!("{}@{version}.json", package.replace('/', "__")))
}

/// `package@version`'s declared dependency names: the store's record, else
/// read from its install in `work` (its published package.json) and
/// recorded in the store. None when neither exists. A published version
/// never changes, so the record never goes stale.
pub fn read_or_record_declared(
    store: &Path,
    work: &Path,
    package: &str,
    version: &str,
) -> Option<Vec<String>> {
    let path = declared_path(store, package, version);
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(r) = serde_json::from_str::<DeclaredRecord>(&text)
    {
        return Some(r.names);
    }
    let installed = work
        .join(format!("{}@{version}", package.replace('/', "__")))
        .join("node_modules")
        .join(package)
        .join("package.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(installed).ok()?).ok()?;
    let names = declared_names(&json);
    let record = DeclaredRecord {
        package: package.to_string(),
        version: version.to_string(),
        names: names.clone(),
    };
    // best effort: an unwritable store only costs the next merge a re-read
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_ok()
        && let Ok(mut bytes) = serde_json::to_vec(&record)
    {
        bytes.push(b'\n');
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    Some(names)
}

/// Who depends on whom, by interned package id.
#[derive(Default)]
pub struct Dependencies {
    of: HashMap<u32, HashSet<u32>>,
}

impl Dependencies {
    /// `package` depends on `dependency`.
    pub fn add(&mut self, package: u32, dependency: u32) {
        if package != dependency {
            self.of.entry(package).or_default().insert(dependency);
        }
    }

    fn depends(&self, package: u32, dependency: u32) -> bool {
        self.of
            .get(&package)
            .is_some_and(|d| d.contains(&dependency))
    }

    /// The INLINERS among one print's distinct `owners` (each with the
    /// smallest module, in bytes, it holds the print in): each owner that
    /// depends on another owner which does not depend on it, outside its
    /// family, whose module is no far bigger ([`ORIGIN_SPECIFICITY`]); with
    /// the origin that decided it (the first in `owners` order). Empty when
    /// the print is left alone: one owner, more than [`MAX_OWNERS`], or
    /// every owner an inliner.
    pub fn inliners(
        &self,
        owners: &[(u32, u64)],
        same_family: &dyn Fn(u32, u32) -> bool,
    ) -> Vec<(u32, u32)> {
        let (factor, slack) = ORIGIN_SPECIFICITY;
        if owners.len() < 2 || owners.len() > MAX_OWNERS {
            return Vec::new();
        }
        let out: Vec<(u32, u32)> = owners
            .iter()
            .filter_map(|&(b, b_bytes)| {
                owners
                    .iter()
                    .find(|&&(a, a_bytes)| {
                        a != b
                            && self.depends(b, a)
                            && !self.depends(a, b)
                            && a_bytes <= factor * b_bytes + slack
                            && !same_family(a, b)
                    })
                    .map(|&(a, _)| (b, a))
            })
            .collect();
        if out.len() == owners.len() {
            return Vec::new();
        }
        out
    }
}

/// The families the rule reads: the shipped ones joined by the shipped
/// metadata-derived links (the data the lookup and the scorer read).
pub fn families() -> PackageFamilies {
    PackageFamilies::shipped().with_shipped_links()
}

/// What the rule dropped (`dedup.json`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupRecord {
    pub rule: &'static str,
    pub max_owners: usize,
    /// Versions whose declared names the merge read.
    pub declared_versions: usize,
    /// slice → prints and rows dropped.
    pub slices: BTreeMap<String, SliceDrops>,
    /// Every (inliner, origin) pair over all slices, by rows dropped (most
    /// first, then by name).
    pub pairs: Vec<PairDrops>,
}

#[derive(Default, Serialize)]
pub struct SliceDrops {
    pub hashes: usize,
    pub rows: usize,
}

#[derive(Serialize)]
pub struct PairDrops {
    pub inliner: String,
    pub origin: String,
    pub rows: usize,
}
