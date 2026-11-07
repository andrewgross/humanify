//! The exp096 library scorer's committed files: each corpus's frozen
//! manifest and its truth-layer manifests (each layer's per-entry sha),
//! and `bands.json`. One typed loader per file KIND; the corpus table
//! replaces lib096's `CORPORA` dict (its per-host data roots included,
//! overridable by `libscore.data_root`).
//!
//! The layer manifests' scorer-specific blocks (`rules`, `tables`,
//! `npmVerdicts`, ...) are kept as opaque JSON here: their KEYS are
//! checked (deny_unknown_fields), their contents are typed by the library
//! scorer that reads them (lane L2b).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::{Loaded, load_json};
use crate::paths::LIBRARY_DIR;

/// One corpus: its name, the suffix of its files, and its data root.
#[derive(Clone, Copy, Debug)]
pub struct Corpus {
    pub name: &'static str,
    /// The manifests' file-name suffix: `corpus<suffix>.json`.
    pub suffix: &'static str,
    /// Where the frozen bundles, truth and raw reports live (per host,
    /// never committed).
    pub data_root: &'static str,
}

/// The sixteen corpora (lib096 `CORPORA`).
pub const CORPORA: &[Corpus] = &[
    corpus("dev", "", "/work/exp096-data"),
    corpus("test", "-test", "/work/exp100-test-data/frozen"),
    corpus("test2", "-test2", "/work/exp116-test2-data/frozen"),
    corpus("bundlers", "-bundlers", "/work/exp105-data/frozen"),
    corpus("apps", "-apps", "/work/exp107-data/frozen"),
    corpus("pop", "-pop", "/work/exp112-data/pop/frozen"),
    corpus("remin", "-remin", "/work/exp126-data/remin/frozen"),
    corpus("remin2", "-remin2", "/work/exp140-data/remin2/frozen"),
    corpus("remin3", "-remin3", "/work/exp141-data/remin3/frozen"),
    corpus("remin4", "-remin4", "/work/exp146-data/remin4/frozen"),
    corpus("bun-dev", "-bun-dev", "/work/exp150-data/bun-dev/frozen"),
    corpus("bun-held", "-bun-held", "/work/exp150-data/bun-held/frozen"),
    corpus(
        "kind-esbuild",
        "-kind-esbuild",
        "/work/exp181-data/kind-esbuild/frozen",
    ),
    corpus(
        "kind-webpack",
        "-kind-webpack",
        "/work/exp181-data/kind-webpack/frozen",
    ),
    corpus(
        "kind-rollup",
        "-kind-rollup",
        "/work/exp181-data/kind-rollup/frozen",
    ),
    corpus("cc", "-cc", "/work/cc-library-truth/frozen"),
];

const fn corpus(name: &'static str, suffix: &'static str, data_root: &'static str) -> Corpus {
    Corpus {
        name,
        suffix,
        data_root,
    }
}

/// The corpus named `name`.
pub fn corpus_named(name: &str) -> Result<&'static Corpus, String> {
    CORPORA.iter().find(|c| c.name == name).ok_or_else(|| {
        let all: Vec<&str> = CORPORA.iter().map(|c| c.name).collect();
        format!("unknown corpus {name:?} — valid: {}", all.join(", "))
    })
}

/// A corpus manifest (`corpus-<c>.json`): the frozen entries.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema: String,
    /// Absent on the original `dev` manifest.
    pub corpus: Option<String>,
    pub frozen: Option<Value>,
    pub note: Option<String>,
    pub entries: Vec<CorpusEntry>,
}

/// One frozen bundle of a corpus.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CorpusEntry {
    pub id: String,
    pub set: String,
    pub bundler: String,
    pub description: String,
    pub files: CorpusFiles,
    pub truth: Truth,
    pub app_package: Option<String>,
    pub built_with: Option<String>,
    pub kind: Option<String>,
    pub minifier: Option<String>,
    pub stratum: Option<String>,
    pub target: Option<String>,
}

/// An entry's frozen files.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CorpusFiles {
    pub bundle: FrozenFile,
    pub map: Option<FrozenFile>,
    pub inner_map: Option<FrozenFile>,
}

/// One frozen file: name, size, sha, and where it was copied from.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub origin: String,
}

/// How an entry's truth was made.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Truth {
    pub method: String,
    pub sha256: String,
}

/// The truth layers, by file prefix (scorer generations 1-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Origins,
    Origins7,
    Origins10,
    Origins12,
    Copies,
    Copies2,
    Chained,
    Chained9,
    Chained11,
    Misattr13,
    Mapdefect14,
}

impl LayerKind {
    pub const ALL: [LayerKind; 11] = [
        LayerKind::Origins,
        LayerKind::Origins7,
        LayerKind::Origins10,
        LayerKind::Origins12,
        LayerKind::Copies,
        LayerKind::Copies2,
        LayerKind::Chained,
        LayerKind::Chained9,
        LayerKind::Chained11,
        LayerKind::Misattr13,
        LayerKind::Mapdefect14,
    ];

    /// The file prefix (`origins7` in `origins7-bun-dev.json`).
    pub fn prefix(self) -> &'static str {
        match self {
            LayerKind::Origins => "origins",
            LayerKind::Origins7 => "origins7",
            LayerKind::Origins10 => "origins10",
            LayerKind::Origins12 => "origins12",
            LayerKind::Copies => "copies",
            LayerKind::Copies2 => "copies2",
            LayerKind::Chained => "chained",
            LayerKind::Chained9 => "chained9",
            LayerKind::Chained11 => "chained11",
            LayerKind::Misattr13 => "misattr13",
            LayerKind::Mapdefect14 => "mapdefect14",
        }
    }

    /// The key the library summary records this layer's sha under
    /// (`summary.json` `config.<key>`).
    /// Scorer 12 replaced the copies layer with v2 under the SAME key
    /// (truth_audit.py `copies_sha`), so `copies2` records as
    /// `copiesSha256` and the retired v1 layer as `copiesV1Sha256`.
    pub fn summary_key(self) -> String {
        match self {
            LayerKind::Copies => "copiesV1Sha256".to_string(),
            LayerKind::Copies2 => "copiesSha256".to_string(),
            other => format!("{}Sha256", other.prefix()),
        }
    }

    /// The layer's own blocks besides `schema`, `corpusSha256` and
    /// `entries` — exactly these keys, every one required (a missing or
    /// extra key fails the load, the `deny_unknown_fields` rule).
    pub fn blocks(self) -> &'static [&'static str] {
        match self {
            LayerKind::Origins => &[],
            LayerKind::Chained | LayerKind::Chained11 => &["about"],
            LayerKind::Origins7 | LayerKind::Origins10 | LayerKind::Origins12 => {
                &["about", "rules"]
            }
            LayerKind::Chained9 => &["about", "npmVerdicts"],
            LayerKind::Copies => &[
                "bytes",
                "cover",
                "familiesSha256",
                "indexId",
                "maxFamilies",
                "minNodes",
                "reference",
            ],
            LayerKind::Copies2 => &[
                "bytes",
                "cover",
                "familiesSha256",
                "indexes",
                "maxFamilies",
                "minNodes",
                "tables",
                "unitsFrom",
            ],
            LayerKind::Misattr13 => &["about", "rules", "unitsLabel"],
            LayerKind::Mapdefect14 => &["about", "bytes", "rules", "unitsLabels"],
        }
    }
}

/// A truth layer's manifest: the per-entry shas, the corpus it belongs
/// to, and the layer kind's own blocks (kept as JSON: the library scorer,
/// lane L2b, types what it reads).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    #[serde(skip, default = "default_kind")]
    pub kind: LayerKind,
    pub schema: String,
    pub corpus_sha256: String,
    pub entries: BTreeMap<String, String>,
    #[serde(flatten)]
    pub blocks: BTreeMap<String, Value>,
}

fn default_kind() -> LayerKind {
    LayerKind::Origins
}

fn layer_file(dir: &Path, kind: LayerKind, corpus: &Corpus) -> std::path::PathBuf {
    dir.join(format!("{}{}.json", kind.prefix(), corpus.suffix))
}

/// Load one layer of `corpus`; Ok(None) when the corpus has no such layer
/// (the `copies` layer exists for seven corpora only).
pub fn layer(
    repo: &Path,
    kind: LayerKind,
    corpus: &Corpus,
) -> Result<Option<Loaded<Layer>>, String> {
    let path = layer_file(&repo.join(LIBRARY_DIR), kind, corpus);
    if !path.exists() {
        return Ok(None);
    }
    let mut loaded: Loaded<Layer> = load_json(&path)?;
    loaded.value.kind = kind;
    let got: Vec<&str> = loaded.value.blocks.keys().map(String::as_str).collect();
    let mut want = kind.blocks().to_vec();
    want.sort_unstable();
    if got != want {
        return Err(format!(
            "{}: a {} layer has the blocks {want:?}, this file has {got:?}",
            path.display(),
            kind.prefix()
        ));
    }
    Ok(Some(loaded))
}

/// A corpus with its manifest and every layer it has.
#[derive(Clone, Debug)]
pub struct LoadedCorpus {
    pub corpus: &'static Corpus,
    pub manifest: Loaded<CorpusManifest>,
    /// The manifest's IDENTITY (`corpusSha256`): the sha of its canonical
    /// Python spelling, not of the file's bytes (lib096 `manifest_sha`: the
    /// commit hook re-spells committed JSON).
    pub manifest_id: String,
    pub layers: Vec<Loaded<Layer>>,
    /// Each layer's identity by the summary's key (`originsSha256`), the
    /// same canonical sha.
    pub layer_ids: BTreeMap<String, String>,
}

/// A library file's identity: sha256 of `json.dumps(obj, sort_keys=True,
/// indent=1, ensure_ascii=True) + "\n"` (lib096 `dump`).
pub fn canonical_id(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value =
        crate::json::py::PyValue::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(super::sha256_hex(
        crate::json::py_text(&value, true).as_bytes(),
    ))
}

/// Load a corpus: its manifest and all its layers. Every layer must name
/// the same corpus sha.
pub fn load_corpus(repo: &Path, name: &str) -> Result<LoadedCorpus, String> {
    let corpus = corpus_named(name)?;
    let dir = repo.join(LIBRARY_DIR);
    let manifest: Loaded<CorpusManifest> =
        load_json(&dir.join(format!("corpus{}.json", corpus.suffix)))?;
    let mut layers = Vec::new();
    for kind in LayerKind::ALL {
        if let Some(l) = layer(repo, kind, corpus)? {
            layers.push(l);
        }
    }
    if let Some(first) = layers.first() {
        let sha = &first.value.corpus_sha256;
        if let Some(odd) = layers.iter().find(|l| &l.value.corpus_sha256 != sha) {
            return Err(format!(
                "{}: corpusSha256 {} differs from {}'s {}",
                odd.path.display(),
                odd.value.corpus_sha256,
                first.path.display(),
                sha
            ));
        }
    }
    let manifest_id = canonical_id(&manifest.path)?;
    let mut layer_ids = BTreeMap::new();
    for l in &layers {
        layer_ids.insert(l.value.kind.summary_key(), canonical_id(&l.path)?);
    }
    Ok(LoadedCorpus {
        corpus,
        manifest,
        manifest_id,
        layers,
        layer_ids,
    })
}

/// `bands.json`: the library score's band (zero) and its provenance.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryBands {
    pub band: f64,
    pub meaning: String,
    pub provenance: LibraryBandsProvenance,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LibraryBandsProvenance {
    pub binary_sha256: String,
    pub build_commit: String,
    pub corpus_sha256: String,
    pub index_id: String,
    pub labels: Vec<String>,
    pub method: String,
    pub rule: String,
}

/// Load `bands.json`.
pub fn bands(repo: &Path) -> Result<Loaded<LibraryBands>, String> {
    load_json(&repo.join(LIBRARY_DIR).join("bands.json"))
}
