//! One corpus, opened for scoring (lib096 + truth_audit's loaders): its
//! manifest and truth-layer manifests (L1's committed loaders), its data
//! root on this host, and every per-entry file the scorer reads — each
//! checked against the sha its manifest froze, refused otherwise.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::committed::library::{self, CorpusEntry, LayerKind, LoadedCorpus};
use crate::committed::sha256_hex;

/// Scorer 7's origin-layer rules (origins.py `RULES7`), and scorers 10 and
/// 12's additions: a frozen origin layer must have been built with them.
pub const RULES7: &[(&str, bool)] = &[("lazybody", true), ("chunk", true)];
pub const RULES10: &[(&str, bool)] = &[("lazybody", true), ("chunk", true), ("via", true)];
pub const RULES12: &[(&str, bool)] = &[
    ("lazybody", true),
    ("chunk", true),
    ("via", true),
    ("patched", true),
];

/// The corpora whose per-bundle cards are never committed (lib096 `PRIVATE`).
pub const PRIVATE: &[&str] = &["test", "test2", "remin2", "bun-held"];

/// An opened corpus.
pub struct Corpus {
    pub loaded: LoadedCorpus,
    pub data: PathBuf,
    pub repo: PathBuf,
}

impl Corpus {
    /// Open `name`; `data_root` overrides data-roots.json's root.
    pub fn open(repo: &Path, name: &str, data_root: Option<&str>) -> Result<Corpus, String> {
        let loaded = library::load_corpus(repo, name)?;
        let data = match data_root {
            Some(d) => PathBuf::from(d),
            None => PathBuf::from(&library::data_roots(repo)?.value.roots[name]),
        };
        Ok(Corpus {
            loaded,
            data,
            repo: repo.to_path_buf(),
        })
    }

    pub fn name(&self) -> &'static str {
        self.loaded.corpus.name
    }

    pub fn is_private(&self) -> bool {
        PRIVATE.contains(&self.name())
    }

    pub fn entries(&self) -> &[CorpusEntry] {
        &self.loaded.manifest.value.entries
    }

    /// The layer of `kind`, or an error naming the missing file.
    pub fn layer(&self, kind: LayerKind) -> Result<&library::Layer, String> {
        self.loaded
            .layers
            .iter()
            .map(|l| &l.value)
            .find(|l| l.kind == kind)
            .ok_or_else(|| format!("corpus {} has no {} layer", self.name(), kind.prefix()))
    }

    /// A layer's canonical identity (`<kind>Sha256` in a summary).
    pub fn layer_id(&self, kind: LayerKind) -> Result<&str, String> {
        self.loaded
            .layer_ids
            .get(&kind.summary_key())
            .map(String::as_str)
            .ok_or_else(|| format!("corpus {} has no {} layer", self.name(), kind.prefix()))
    }

    /// A frozen corpus file: `<DATA>/corpus/<id>/<role file name>`.
    pub fn corpus_path(&self, entry: &CorpusEntry, role: Role) -> Result<PathBuf, String> {
        let f = match role {
            Role::Bundle => Some(&entry.files.bundle),
            Role::Map => entry.files.map.as_ref(),
            Role::InnerMap => entry.files.inner_map.as_ref(),
        }
        .ok_or_else(|| format!("{}: no {role:?} file", entry.id))?;
        Ok(self.data.join("corpus").join(&entry.id).join(&f.name))
    }

    /// The map the truth's sources index: the inner map when there is one.
    pub fn source_map_path(&self, entry: &CorpusEntry) -> Result<PathBuf, String> {
        let role = if entry.files.inner_map.is_some() {
            Role::InnerMap
        } else {
            Role::Map
        };
        self.corpus_path(entry, role)
    }

    /// `<DATA>/truth/<id><suffix>`.
    pub fn truth_file(&self, entry: &CorpusEntry, suffix: &str) -> PathBuf {
        self.data
            .join("truth")
            .join(format!("{}{suffix}", entry.id))
    }

    /// The entry's truth, its sha checked against the manifest.
    pub fn truth<T: DeserializeOwned>(&self, entry: &CorpusEntry) -> Result<T, String> {
        let p = self.truth_file(entry, ".truth.json.gz");
        read_gz_json(&p, Some(&entry.truth.sha256), "manifest")
    }

    /// A per-entry layer file, its sha checked against the layer manifest.
    pub fn layer_file<T: DeserializeOwned>(
        &self,
        entry: &CorpusEntry,
        kind: LayerKind,
        suffix: &str,
    ) -> Result<T, String> {
        let layer = self.layer(kind)?;
        let p = self.truth_file(entry, suffix);
        let want = layer.entries.get(&entry.id).map(String::as_str);
        let what = format!("{}{}.json", kind.prefix(), self.loaded.corpus.suffix);
        read_gz_json(&p, Some(want.unwrap_or("<absent>")), &what)
    }
}

/// A frozen corpus file's role.
#[derive(Clone, Copy, Debug)]
pub enum Role {
    Bundle,
    Map,
    InnerMap,
}

/// sha256 of a file.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

/// Read a `.json.gz`, refusing it when its sha is not `want`.
pub fn read_gz_json<T: DeserializeOwned>(
    path: &Path,
    want: Option<&str>,
    against: &str,
) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(want) = want {
        let got = sha256_hex(&bytes);
        if got != want {
            return Err(format!(
                "REFUSED: {} sha256 {} != {against} {}",
                path.display(),
                &got[..12],
                &want[..want.len().min(12)]
            ));
        }
    }
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// JSON text with every LONE surrogate escape (`\ud800` with no partner)
/// spelled `\ufffd`. Python's `json` keeps a lone surrogate in a `str`;
/// a Rust string cannot hold one. Every reader of the affected strings
/// (source contents, for tokens) treats both as a non-word character.
pub fn lone_surrogates_replaced(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("\\u") {
        return text.into();
    }
    let b = text.as_bytes();
    let hex = |at: usize| -> Option<u32> {
        let s = text.get(at..at + 4)?;
        u32::from_str_radix(s, 16).ok()
    };
    let is_u = |at: usize| b.get(at) == Some(&b'\\') && b.get(at + 1) == Some(&b'u');
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut k = 0;
    let mut changed = false;
    while k < b.len() {
        if b[k] != b'\\' {
            k += 1;
            continue;
        }
        if b.get(k + 1) != Some(&b'u') {
            k += 2;
            continue;
        }
        let Some(u) = hex(k + 2) else {
            k += 2;
            continue;
        };
        let high = (0xD800..0xDC00).contains(&u);
        let low = (0xDC00..0xE000).contains(&u);
        if high && is_u(k + 6) && hex(k + 8).is_some_and(|l| (0xDC00..0xE000).contains(&l)) {
            k += 12;
            continue;
        }
        if high || low {
            out.push_str(&text[last..k]);
            out.push_str("\\ufffd");
            k += 6;
            last = k;
            changed = true;
            continue;
        }
        k += 6;
    }
    if !changed {
        return text.into();
    }
    out.push_str(&text[last..]);
    out.into()
}

/// A source map, as far as the scorer reads it.
#[derive(Deserialize)]
pub struct SourceMap {
    #[serde(default)]
    pub sources: Vec<Value>,
    #[serde(rename = "sourcesContent", default)]
    pub sources_content: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub mappings: String,
}

/// Read a source map (lone surrogates read as U+FFFD).
pub fn read_map(path: &Path) -> Result<SourceMap, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&lone_surrogates_replaced(&text))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// A frozen layer manifest's `rules` block equals `want`.
pub fn rules_are(layer: &library::Layer, want: &[(&str, bool)]) -> bool {
    let want: BTreeMap<String, Value> = want
        .iter()
        .map(|(k, v)| (k.to_string(), Value::Bool(*v)))
        .collect();
    layer.blocks.get("rules") == Some(&Value::Object(want.into_iter().collect()))
}

/// `library corpus verify` (corpus.py `verify`): every frozen file, truth
/// and origin layer file against its manifest's sha. The lines name each
/// file that moved; empty when the corpus is intact.
pub fn verify(corpus: &Corpus) -> Vec<String> {
    let mut bad = Vec::new();
    let moved = |p: &Path, want: &str| sha256_file(p).map_or(true, |got| got != want);
    for e in corpus.entries() {
        let files = [
            ("bundle", Some(&e.files.bundle)),
            ("map", e.files.map.as_ref()),
            ("innerMap", e.files.inner_map.as_ref()),
        ];
        for (role, f) in files {
            let Some(f) = f else { continue };
            let p = corpus.data.join("corpus").join(&e.id).join(&f.name);
            if moved(&p, &f.sha256) {
                bad.push(format!("MOVED {} {role} {}", e.id, p.display()));
            }
        }
        if moved(&corpus.truth_file(e, ".truth.json.gz"), &e.truth.sha256) {
            bad.push(format!("MOVED {} truth", e.id));
        }
    }
    if let Ok(origins) = corpus.layer(LayerKind::Origins) {
        for e in corpus
            .entries()
            .iter()
            .filter(|e| e.truth.method == "sourcemap")
        {
            let want = origins.entries.get(&e.id).map_or("", String::as_str);
            if moved(&corpus.truth_file(e, ".origins.json.gz"), want) {
                bad.push(format!("MOVED {} origins", e.id));
            }
        }
    }
    bad
}

#[cfg(test)]
#[path = "corpus_test.rs"]
mod corpus_test;
