//! Shards: one file per `package@version` per kind — the module rows one
//! bundler slice produced, or the literal rows of the npm source
//! (docs/design/library-index.md §2 step 6, §5).
//!
//! A shard's KEY is the sha256 of everything that decides its content:
//! the shard format, the kind (slice), the package and version, the
//! bundler and its version, the build recipe's version, and the
//! fingerprint version. Nothing else is in it (no time, no path), so a
//! shard is reused for as long as its key matches — a package version
//! never changes on npm — and an update builds only what is new.
//!
//! Rows are labelled with their OWNER `package@version`: a dependency's
//! modules are the dependency's, not the package that pulled it in.

use std::path::{Path, PathBuf};

use humanify_core::libindex::IndexSlice;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SHARD_FORMAT: &str = "humanify-index-shard/1";

/// What a shard holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShardKind {
    /// Whole-module keys of one bundler slice's builds.
    Modules(IndexSlice),
    /// Literal sets of the npm source files (bundler-independent).
    Literals,
}

impl ShardKind {
    /// The recorded spelling (and the store directory).
    pub fn name(self) -> &'static str {
        match self {
            ShardKind::Modules(slice) => slice.name(),
            ShardKind::Literals => "literals",
        }
    }
}

/// Everything that decides a shard's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardInputs {
    pub kind: ShardKind,
    pub package: String,
    pub version: String,
    /// `bundler@version` (None for literals).
    pub bundler: Option<String>,
    pub recipe: String,
    pub fingerprint: String,
}

#[derive(Serialize)]
struct KeyView<'a> {
    format: &'a str,
    kind: &'a str,
    package: &'a str,
    version: &'a str,
    bundler: Option<&'a str>,
    recipe: &'a str,
    fingerprint: &'a str,
}

impl ShardInputs {
    /// sha256 (hex) of the inputs in a fixed order.
    pub fn key(&self) -> String {
        let view = KeyView {
            format: SHARD_FORMAT,
            kind: self.kind.name(),
            package: &self.package,
            version: &self.version,
            bundler: self.bundler.as_deref(),
            recipe: &self.recipe,
            fingerprint: &self.fingerprint,
        };
        let bytes = serde_json::to_vec(&view).unwrap_or_default();
        hex_sha256(&bytes)
    }

    /// `<kind>/<package>@<version>` — the manifest's name for the shard.
    pub fn label(&self) -> String {
        format!("{}/{}@{}", self.kind.name(), self.package, self.version)
    }
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One bundled module.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShardModule {
    pub owner: String,
    pub version: String,
    pub file: String,
    pub key: String,
}

/// One statement or function unit of a bundled module (statement-level
/// library fingerprints, `humanify_core::libindex::units`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardUnit {
    pub owner: String,
    pub version: String,
    pub file: String,
    pub hash: String,
    pub nodes: u32,
    pub content: u32,
    /// The bundled size of the file the unit came from, in this build —
    /// the lookup's specificity rule compares it with the query's module.
    pub module_bytes: u64,
}

/// One source file's literal set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShardLiteral {
    pub owner: String,
    pub version: String,
    pub file: String,
    pub lits: Vec<String>,
}

/// What the build saw and skipped (reported, never indexed).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardNotes {
    /// entry → `ok` or the bundler's error (first line).
    pub builds: std::collections::BTreeMap<String, String>,
    /// Modules whose span mixed sources (no owner).
    pub mixed_spans: usize,
    /// Modules from outside `node_modules` (the app entry, runtime glue).
    pub unowned: usize,
    /// Modules the content key could not read.
    pub unkeyed: usize,
    /// Source files that did not parse.
    pub unread_files: usize,
    /// Pre-bundled ("sponge") source files dropped.
    pub sponge_files: usize,
    /// Statement units whose span mixed sources (no owner).
    #[serde(default)]
    pub unit_mixed_spans: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shard {
    pub format: String,
    pub key: String,
    pub kind: String,
    pub package: String,
    pub version: String,
    pub bundler: Option<String>,
    pub recipe: String,
    pub fingerprint: String,
    pub modules: Vec<ShardModule>,
    #[serde(default)]
    pub units: Vec<ShardUnit>,
    pub literals: Vec<ShardLiteral>,
    pub notes: ShardNotes,
}

impl Shard {
    /// An empty shard for `inputs`.
    pub fn new(inputs: &ShardInputs) -> Shard {
        Shard {
            format: SHARD_FORMAT.to_string(),
            key: inputs.key(),
            kind: inputs.kind.name().to_string(),
            package: inputs.package.clone(),
            version: inputs.version.clone(),
            bundler: inputs.bundler.clone(),
            recipe: inputs.recipe.clone(),
            fingerprint: inputs.fingerprint.clone(),
            modules: Vec::new(),
            units: Vec::new(),
            literals: Vec::new(),
            notes: ShardNotes::default(),
        }
    }

    /// `<kind>/<package>@<version>`.
    pub fn label(&self) -> String {
        format!("{}/{}@{}", self.kind, self.package, self.version)
    }
}

/// `store/shards/<kind>/<package>@<version>.json` (a scope's `/` is `__`).
pub fn shard_path(store: &Path, kind: &str, package: &str, version: &str) -> PathBuf {
    store
        .join("shards")
        .join(kind)
        .join(format!("{}@{version}.json", package.replace('/', "__")))
}

/// Write `shard` with its rows sorted and deduplicated.
pub fn write_shard(store: &Path, shard: &Shard) -> Result<(), String> {
    let mut shard = shard.clone();
    shard.modules.sort();
    shard.modules.dedup();
    shard.literals.sort();
    shard.literals.dedup();
    let path = shard_path(store, &shard.kind, &shard.package, &shard.version);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    let mut bytes = serde_json::to_vec(&shard).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    // write-then-rename: a crashed build never leaves a half shard
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", path.display()))
}

/// The stored shard for `inputs`, if one exists with the SAME key.
pub fn read_current(store: &Path, inputs: &ShardInputs) -> Option<Shard> {
    let path = shard_path(store, inputs.kind.name(), &inputs.package, &inputs.version);
    let text = std::fs::read_to_string(path).ok()?;
    let shard: Shard = serde_json::from_str(&text).ok()?;
    (shard.key == inputs.key()).then_some(shard)
}
