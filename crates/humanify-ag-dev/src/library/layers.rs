//! `library layers <corpus>`: re-freeze a corpus's truth layers into a
//! scratch directory and compare every per-entry sha with the committed
//! layer manifest (docs/dev-harness.md §4 proof b'). Ported builders so
//! far: the origin layers of scorers 4, 7, 10 and 12 (corpus.py
//! `build_origins`, origins7/10/12.py `build`). The other layers' builders
//! are listed by `unported()`; their frozen files are read as they are.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::corpus::{Corpus, RULES7, RULES10, RULES12, Role, read_map};
use super::decoder::{self, Inner};
use super::families::Families;
use super::origins::{self, ChunkCtx, Inputs, Origins, Rules};
use super::py::{compact, dict, int, kv, s};
use crate::committed::library::{CorpusEntry, LayerKind};
use crate::json::py::PyValue;

/// Where scorer 7's cached npm metadata lives (fetch_metadata.py and
/// bundled.py fill it; this host's copy).
pub const REGISTRY: &str = "/work/exp112-data/registry";
pub const BUNDLED: &str = "/work/exp112-data/bundled";

#[derive(Deserialize)]
struct RawTruth {
    #[serde(default)]
    segments: Vec<(i64, i64)>,
    #[serde(default)]
    sources: Vec<Vec<Value>>,
}

/// origins7.py `load`: the raw truth and the source map's sources /
/// contents, the segments re-decoded with scorer 4's decoder and checked
/// against the frozen truth.
struct Loaded {
    segs: Vec<(i64, i64, i64)>,
    shippers: Vec<String>,
    versions: Vec<Option<String>>,
    paths: Vec<Option<String>>,
    contents: Vec<Option<String>>,
}

fn load(corpus: &Corpus, entry: &CorpusEntry) -> Result<Loaded, String> {
    let truth: RawTruth = corpus.truth(entry)?;
    let bpath = corpus.corpus_path(entry, Role::Bundle)?;
    let text = std::fs::read_to_string(&bpath).map_err(|e| format!("{}: {e}", bpath.display()))?;
    let smap = read_map(&corpus.corpus_path(entry, Role::Map)?)?;
    let (srcmap, inner) = match entry.files.inner_map {
        Some(_) => {
            let m = read_map(&corpus.corpus_path(entry, Role::InnerMap)?)?;
            let inner = Inner {
                lines: decoder::decode(&m.mappings, false)?,
                offset: 1,
            };
            (m, Some(inner))
        }
        None => (read_map(&corpus.corpus_path(entry, Role::Map)?)?, None),
    };
    let segs = decoder::segments(&text, &smap.mappings, inner.as_ref(), false, false)?;
    let same = segs.len() == truth.segments.len()
        && segs
            .iter()
            .zip(&truth.segments)
            .all(|(a, b)| a.0 == b.0 && a.1 == Some(b.1));
    if !same {
        return Err(format!(
            "REFUSED: {}: re-decoded segments differ from the frozen truth",
            entry.id
        ));
    }
    let col = |r: &Vec<Value>, i: usize| r.get(i).and_then(Value::as_str).map(String::from);
    Ok(Loaded {
        segs: segs
            .iter()
            .map(|s| (s.1.unwrap_or(-1), s.2.unwrap_or(-1), s.3.unwrap_or(-1)))
            .collect(),
        shippers: truth
            .sources
            .iter()
            .map(|r| col(r, 0).unwrap_or_default())
            .collect(),
        versions: truth.sources.iter().map(|r| col(r, 1)).collect(),
        paths: srcmap
            .sources
            .iter()
            .map(|v| v.as_str().map(String::from))
            .collect(),
        contents: srcmap.sources_content.unwrap_or_default(),
    })
}

/// One origin layer's rules and file names.
#[derive(Clone, Copy)]
pub struct OriginLayer {
    pub kind: LayerKind,
    pub rules: Option<Rules>,
    pub schema: &'static str,
    pub about: &'static str,
    pub rules_json: &'static [(&'static str, bool)],
}

pub const ORIGIN_LAYERS: [OriginLayer; 4] = [
    OriginLayer {
        kind: LayerKind::Origins,
        rules: None,
        schema: origins::SCHEMA,
        about: "",
        rules_json: &[],
    },
    OriginLayer {
        kind: LayerKind::Origins7,
        rules: Some(Rules {
            lazybody: true,
            chunk: true,
            via: false,
            patched: false,
        }),
        schema: "exp112-origins7/1",
        about: "scorer 7 origin layer (experiments/112-test-precision/scorer7/origins7.py): origins.py with RULES7; per entry the sha256 of <DATA>/truth/<id>.origins7.json.gz",
        rules_json: RULES7,
    },
    OriginLayer {
        kind: LayerKind::Origins10,
        rules: Some(Rules {
            lazybody: true,
            chunk: true,
            via: true,
            patched: false,
        }),
        schema: "exp125-origins10/1",
        about: "scorer 10 origin layer (experiments/125-nested-chunk-publisher/scorer10/origins10.py): origins.py with RULES10; per entry the sha256 of <DATA>/truth/<id>.origins10.json.gz",
        rules_json: RULES10,
    },
    OriginLayer {
        kind: LayerKind::Origins12,
        rules: Some(Rules {
            lazybody: true,
            chunk: true,
            via: true,
            patched: true,
        }),
        schema: "exp129-origins12/1",
        about: "scorer 12 origin layer (experiments/129-helpers-copies/scorer12/origins12.py): origins.py with RULES12; per entry the sha256 of <DATA>/truth/<id>.origins12.json.gz",
        rules_json: RULES12,
    },
];

/// One entry's origin layer document.
fn origin_doc(
    corpus: &Corpus,
    entry: &CorpusEntry,
    layer: &OriginLayer,
    inputs: &Inputs,
    fam: &Families,
) -> Result<PyValue, String> {
    let l = load(corpus, entry)?;
    let ctx = ChunkCtx {
        versions: &l.versions,
        inputs,
        fam,
    };
    let o = Origins::new(
        &l.paths,
        &l.contents,
        &l.shippers,
        layer.rules.unwrap_or_default(),
        layer.rules.map(|_| &ctx),
    );
    let (names, kinds, runs) = origins::origin_runs(&l.segs, &o);
    let strs = |v: Vec<String>| PyValue::List(v.into_iter().map(PyValue::Str).collect());
    Ok(dict(vec![
        kv("schema", s(origins::SCHEMA)),
        kv("names", strs(names)),
        kv("kinds", strs(kinds)),
        kv(
            "runs",
            PyValue::List(
                runs.into_iter()
                    .map(|(a, b, c)| PyValue::List(vec![int(a), int(b), int(c)]))
                    .collect(),
            ),
        ),
        kv("segments", int(l.segs.len() as i64)),
    ]))
}

/// The outcome of re-freezing one layer.
pub struct Refrozen {
    pub kind: LayerKind,
    pub entries: usize,
    pub same: usize,
    pub differ: Vec<String>,
    /// The re-frozen manifest's canonical id equals the committed one's.
    pub manifest_same: bool,
}

/// Re-freeze one origin layer of `corpus` into `out` (gz files under
/// `out/truth/`, the manifest at `out/<prefix><suffix>.json`).
pub fn refreeze_origin_layer(
    corpus: &Corpus,
    layer: &OriginLayer,
    out: &Path,
    registry: &Path,
    bundled: &Path,
) -> Result<Refrozen, String> {
    let inputs = Inputs::new(registry, bundled);
    let fam = Families::shipped();
    let committed = corpus.layer(layer.kind)?;
    let mut shas: BTreeMap<String, String> = BTreeMap::new();
    let mut differ = Vec::new();
    let entries: Vec<&CorpusEntry> = corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
        .collect();
    for e in &entries {
        let doc = origin_doc(corpus, e, layer, &inputs, &fam)?;
        let suffix = format!(".{}.json.gz", layer.kind.prefix());
        let p: PathBuf = out.join("truth").join(format!("{}{suffix}", e.id));
        super::gz::write_gz(&p, compact(&doc).as_bytes())?;
        let sha = super::corpus::sha256_file(&p)?;
        if committed.entries.get(&e.id) != Some(&sha) {
            differ.push(e.id.clone());
        }
        shas.insert(e.id.clone(), sha);
    }
    let entries_v = dict(shas.iter().map(|(k, v)| (k.clone(), s(v))).collect());
    let manifest = if layer.rules.is_none() {
        dict(vec![
            kv("schema", s(layer.schema)),
            kv("corpusSha256", s(&corpus.loaded.manifest_id)),
            kv("entries", entries_v),
        ])
    } else {
        dict(vec![
            kv("schema", s(layer.schema)),
            kv("corpusSha256", s(&corpus.loaded.manifest_id)),
            kv(
                "rules",
                dict(
                    layer
                        .rules_json
                        .iter()
                        .map(|(k, v)| kv(k, PyValue::Bool(*v)))
                        .collect(),
                ),
            ),
            kv("about", s(layer.about)),
            kv("entries", entries_v),
        ])
    };
    let text = crate::json::py_text(&manifest, true);
    let mpath = out.join(format!(
        "{}{}.json",
        layer.kind.prefix(),
        corpus.loaded.corpus.suffix
    ));
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    std::fs::write(&mpath, &text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let id = crate::committed::sha256_hex(text.as_bytes());
    Ok(Refrozen {
        kind: layer.kind,
        entries: entries.len(),
        same: entries.len() - differ.len(),
        differ,
        manifest_same: corpus.layer_id(layer.kind)? == id,
    })
}

/// Re-freeze a chained layer and compare it with the committed manifest.
pub fn refreeze_chained_layer(
    corpus: &Corpus,
    kind: LayerKind,
    out: &Path,
    npm_cache: &Path,
) -> Result<Refrozen, String> {
    let committed = corpus.layer(kind)?;
    let (shas, text) = super::chained::refreeze(corpus, kind, out, npm_cache)?;
    let differ: Vec<String> = shas
        .iter()
        .filter(|(id, sha)| committed.entries.get(*id) != Some(*sha))
        .map(|(id, _)| id.clone())
        .collect();
    let mpath = out.join(format!(
        "{}{}.json",
        kind.prefix(),
        corpus.loaded.corpus.suffix
    ));
    std::fs::write(&mpath, &text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    Ok(Refrozen {
        kind,
        entries: shas.len(),
        same: shas.len() - differ.len(),
        differ,
        manifest_same: corpus.layer_id(kind)? == crate::committed::sha256_hex(text.as_bytes()),
    })
}

/// The layer builders not ported yet (each layer's frozen files stay
/// readable; only re-freezing them is not available).
pub fn unported() -> &'static [(&'static str, &'static str)] {
    &[
        ("copies2", "experiments/129-helpers-copies/copies2.py"),
        (
            "misattr13",
            "experiments/132-factory-precision/scorer13/misattr13.py",
        ),
        (
            "mapdefect14",
            "experiments/137-scorer-mapdefect/scorer14/mapdefect14.py + tokens14.mjs",
        ),
    ]
}
