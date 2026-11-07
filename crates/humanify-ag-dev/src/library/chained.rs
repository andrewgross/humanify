//! The chained-source layers (scorers 5, 9 and 11): a truth source the
//! bundler reached through an installed package's OWN source map takes
//! that package. Ported from experiments/106 `scorer5/chained.py`,
//! experiments/120 `scorer9/chained9.py` and experiments/128
//! `scorer11/chained11.py`; each reads the frozen corpus map and truth plus
//! the app's install on this host (the entry's `origin` directory).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::corpus::{Corpus, read_map};
use super::py::{dict, from_json, kv, s};
use super::pypath::{basename, dirname, join, normpath, relpath, walk_files};
use crate::committed::library::{CorpusEntry, LayerKind};
use crate::json::py::PyValue;

/// The npm "does `name@latest` exist" cache scorer 9 reads (fetch_metadata.py
/// fills it; this host's copy).
pub const NPM_REGISTRY: &str = "/work/exp104-data/registry";

/// Every `*.map` under the node_modules dirs, in os.walk order.
fn package_maps(nm_dirs: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in nm_dirs {
        let p = Path::new(d);
        if p.is_dir() {
            let mut files = Vec::new();
            walk_files(p, &mut files);
            out.extend(
                files
                    .into_iter()
                    .filter(|f| f.to_string_lossy().ends_with(".map")),
            );
        }
    }
    out
}

/// chained.py `install_dirs`.
fn install_dirs(base: &str, srcs: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = srcs
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| s.contains("node_modules/"))
        .map(|s| {
            let p = normpath(&join(base, s));
            let head = p
                .rsplit_once("/node_modules/")
                .map_or(p.as_str(), |x| x.0)
                .to_string();
            format!("{head}/node_modules")
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A package map's JSON, if it is a readable object with a `sources` list
/// and mentions one of `stems` (chained.py's cheap prefilter).
fn read_package_map(mp: &Path, stems: &[Vec<u8>]) -> Option<(Vec<Value>, String)> {
    let size = std::fs::metadata(mp).ok()?.len();
    if size > 64 << 20 {
        return None;
    }
    let raw = std::fs::read(mp).ok()?;
    if !stems
        .iter()
        .any(|st| memchr::memmem::find(&raw, st).is_some())
    {
        return None;
    }
    let text = String::from_utf8(raw).ok()?;
    let m: Value = serde_json::from_str(&super::corpus::lone_surrogates_replaced(&text)).ok()?;
    let sources = m.as_object()?.get("sources")?.as_array()?.clone();
    let root = match m.get("sourceRoot") {
        Some(Value::String(r)) => r.clone(),
        _ => String::new(),
    };
    Some((sources, root))
}

/// URL-like source (`raw-loader:/...`, `webpack://...`): chained11 `SCHEME`.
fn is_scheme(s: &str) -> bool {
    let mut it = s.char_indices();
    match it.next() {
        Some((_, c)) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for (_, c) in it {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

/// chained.py `map_holders` (`both_bases`: chained11's variant, which also
/// resolves against the map FILE's path and matches URL sources verbatim).
fn map_holders(
    missing: &BTreeMap<String, Vec<usize>>,
    nm: &[String],
    both_bases: bool,
) -> HashMap<String, Vec<String>> {
    let mut holders: HashMap<String, Vec<String>> = HashMap::new();
    let stems: Vec<Vec<u8>> = {
        let mut v: Vec<Vec<u8>> = missing
            .keys()
            .map(|p| basename(p).as_bytes().to_vec())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    for mp in package_maps(nm) {
        let Some((sources, root)) = read_package_map(&mp, &stems) else {
            continue;
        };
        let mps = mp.to_string_lossy().to_string();
        let mut bases = vec![join(&dirname(&mps), &root)];
        if both_bases && !root.is_empty() {
            bases.push(join(&mps, &root));
        }
        for src in sources.iter().filter_map(Value::as_str) {
            if both_bases && is_scheme(src) {
                if missing.contains_key(src) {
                    let h = holders.entry(src.to_string()).or_default();
                    if !h.contains(&mps) {
                        h.push(mps.clone());
                    }
                }
                continue;
            }
            for b in &bases {
                let p = normpath(&join(b, src));
                if missing.contains_key(&p) {
                    let h = holders.entry(p).or_default();
                    if both_bases && h.contains(&mps) {
                        continue;
                    }
                    h.push(mps.clone());
                }
            }
        }
    }
    holders
}

/// truth.py `package_root`: (package, its root path, file inside).
fn package_root(source: &str) -> Option<(String, String, String)> {
    let (head, rest) = source.rsplit_once("node_modules/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    let n = if parts[0].starts_with('@') { 2 } else { 1 };
    let pkg = parts[..n.min(parts.len())].join("/");
    let file = parts.get(n..).map_or(String::new(), |x| x.join("/"));
    Some((pkg.clone(), format!("{head}node_modules/{pkg}"), file))
}

/// chained.py `holder_row`: [package, version, path relative to its
/// node_modules] of the first holder map's package.
fn holder_row(maps: Option<&Vec<String>>, p: &str) -> Option<Vec<PyValue>> {
    let (pkg, root, _) = package_root(maps?.first()?)?;
    let ver = std::fs::read_to_string(join(&root, "package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("version").cloned())
        .map_or(PyValue::Null, |v| from_json(&v).unwrap_or(PyValue::Null));
    Some(vec![s(&pkg), ver, s(&relpath(p, &dirname(&root)))])
}

/// The truth a builder reads: its source labels (and the map's sources).
struct EntryInput {
    base: String,
    srcs: Vec<Value>,
    labels: Vec<String>,
}

fn entry_input(
    corpus: &Corpus,
    entry: &CorpusEntry,
    through_chained9: bool,
) -> Result<EntryInput, String> {
    let base = dirname(&entry.files.bundle.origin);
    let srcs = read_map(&corpus.source_map_path(entry)?)?.sources;
    let labels = if through_chained9 {
        super::truth::load_truth_through(corpus, entry, false)?
            .sources
            .into_iter()
            .map(|s| s.label)
            .collect()
    } else {
        super::truth::load_truth_raw_labels(corpus, entry)?
    };
    Ok(EntryInput { base, srcs, labels })
}

fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

/// Scorer 9/11's npm verdicts: does `name@latest` exist on npm?
pub struct Npm {
    cache: PathBuf,
    pub verdicts: BTreeMap<String, bool>,
}

/// validate-npm-package-name's shape (chained9 `VALID`, case-insensitive).
fn valid_npm_name(name: &str) -> bool {
    let ok_first = |c: char| c.is_ascii_alphanumeric() || c == '~' || c == '-';
    let ok_rest = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '~' | '-');
    let seg = |x: &str| x.chars().next().is_some_and(ok_first) && x.chars().skip(1).all(ok_rest);
    match name.strip_prefix('@') {
        Some(rest) => rest
            .split_once('/')
            .is_some_and(|(sc, n)| seg(sc) && seg(n)),
        None => seg(name),
    }
}

impl Npm {
    pub fn new(cache: &Path) -> Npm {
        Npm {
            cache: cache.to_path_buf(),
            verdicts: BTreeMap::new(),
        }
    }

    fn real(&mut self, name: &str) -> Result<bool, String> {
        if let Some(v) = self.verdicts.get(name) {
            return Ok(*v);
        }
        let v = if !valid_npm_name(name) {
            false
        } else {
            let p = self
                .cache
                .join(format!("{}@latest.json", name.replace('/', "__")));
            let text = std::fs::read_to_string(&p).map_err(|_| {
                format!(
                    "REFUSED: no cached npm answer for {name}@latest ({}): fetching is not ported",
                    p.display()
                )
            })?;
            let d: Value =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
            d.get("__error").is_none()
        };
        self.verdicts.insert(name.to_string(), v);
        Ok(v)
    }
}

/// Missing sources -> their indices, sorted by index list (Python's
/// `sorted(missing.items(), key=lambda kv: kv[1])`).
fn by_index(missing: &BTreeMap<String, Vec<usize>>) -> Vec<(&String, &Vec<usize>)> {
    let mut v: Vec<(&String, &Vec<usize>)> = missing.iter().collect();
    v.sort_by(|a, b| a.1.cmp(b.1));
    v
}

/// chained.py `entry_layer`.
fn layer5(e: &EntryInput) -> Vec<(String, PyValue)> {
    let mut missing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (src, label)) in e.srcs.iter().zip(&e.labels).enumerate() {
        let Some(src) = src.as_str() else { continue };
        let p = normpath(&join(&e.base, src));
        if label == "<app>" && !src.contains("?commonjs-") && !exists(&p) {
            missing.entry(p).or_default().push(i);
        }
    }
    if missing.is_empty() {
        return Vec::new();
    }
    let holders = map_holders(&missing, &install_dirs(&e.base, &e.srcs), false);
    let mut out = Vec::new();
    for (p, idx) in by_index(&missing) {
        let Some(row) = holder_row(holders.get(p), p) else {
            continue;
        };
        for i in idx {
            out.push((i.to_string(), PyValue::List(row.clone())));
        }
    }
    out
}

/// chained9.py `entry_layer`.
fn layer9(e: &EntryInput, npm: &mut Npm) -> Result<Vec<(String, PyValue)>, String> {
    let mut missing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (src, label)) in e.srcs.iter().zip(&e.labels).enumerate() {
        let Some(src) = src.as_str() else { continue };
        if label.is_empty() || label.starts_with('<') {
            continue;
        }
        let p = normpath(&join(&e.base, src));
        if !exists(&p) {
            missing.entry(p).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    if missing.is_empty() {
        return Ok(out);
    }
    let holders = map_holders(&missing, &install_dirs(&e.base, &e.srcs), false);
    for (p, idx) in by_index(&missing) {
        let Some(row) = holder_row(holders.get(p), p) else {
            continue;
        };
        let holder = match &row[0] {
            PyValue::Str(h) => h.clone(),
            _ => unreachable!(),
        };
        for &i in idx {
            let label = &e.labels[i];
            if &holder == label || npm.real(label)? || !npm.real(&holder)? {
                continue;
            }
            let mut r = row.clone();
            r.push(s(label));
            out.push((i.to_string(), PyValue::List(r)));
        }
    }
    Ok(out)
}

/// chained11.py `entry_layer`.
fn layer11(e: &EntryInput, npm: &mut Npm) -> Result<Vec<(String, PyValue)>, String> {
    let dirs = install_dirs(&e.base, &e.srcs);
    let installed = |pkg: &str| {
        dirs.iter()
            .any(|d| exists(&join(&join(d, pkg), "package.json")))
    };
    let mut missing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, (src, label)) in e.srcs.iter().zip(&e.labels).enumerate() {
        let Some(src) = src.as_str() else { continue };
        if label.is_empty() {
            continue;
        }
        if label == "<app>" && is_scheme(src) {
            missing.entry(src.to_string()).or_default().push(i);
            continue;
        }
        if label.starts_with('<') {
            continue;
        }
        let p = normpath(&join(&e.base, src));
        if !exists(&p) && !installed(label) {
            missing.entry(p).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    if missing.is_empty() {
        return Ok(out);
    }
    let holders = map_holders(&missing, &dirs, true);
    for (p, idx) in by_index(&missing) {
        let Some(mut row) = holder_row(holders.get(p), p) else {
            continue;
        };
        if is_scheme(p) {
            row[2] = s(p);
        }
        let holder = match &row[0] {
            PyValue::Str(h) => h.clone(),
            _ => unreachable!(),
        };
        for &i in idx {
            let label = &e.labels[i];
            if &holder == label || !npm.real(&holder)? {
                continue;
            }
            let mut r = row.clone();
            r.push(s(label));
            out.push((i.to_string(), PyValue::List(r)));
        }
    }
    Ok(out)
}

/// Re-freeze one chained layer (`Chained`, `Chained9`, `Chained11`) of
/// `corpus` into `out`; (entry id -> sha, the manifest text).
pub fn refreeze(
    corpus: &Corpus,
    kind: LayerKind,
    out: &Path,
    npm_cache: &Path,
) -> Result<(BTreeMap<String, String>, String), String> {
    let (schema, about) = match kind {
        LayerKind::Chained => (
            "exp106-chained/1",
            "scorer 5 rule `chained` (experiments/106-matcher-hygiene/scorer5/chained.py): per entry, the sha256 of <DATA>/truth/<id>.chained.json.gz",
        ),
        LayerKind::Chained9 => (
            "exp120-chained9/1",
            "scorer 9 rule `npmchain` (experiments/120-truth-families/scorer9/chained9.py): per entry, the sha256 of <DATA>/truth/<id>.chained9.json.gz; npmVerdicts = every `name@latest exists on npm` answer the rule read",
        ),
        LayerKind::Chained11 => (
            "exp128-chained11/2",
            "scorer 11 rule `notinstalled` (experiments/128-cjs-scope/scorer11/chained11.py): per entry, the sha256 of <DATA>/truth/<id>.chained11.json.gz",
        ),
        other => return Err(format!("{} is not a chained layer", other.prefix())),
    };
    let mut npm = Npm::new(npm_cache);
    let mut shas = BTreeMap::new();
    for entry in corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
    {
        let input = entry_input(corpus, entry, kind == LayerKind::Chained11)?;
        let rows = match kind {
            LayerKind::Chained => layer5(&input),
            LayerKind::Chained9 => layer9(&input, &mut npm)?,
            _ => layer11(&input, &mut npm)?,
        };
        let doc = dict(vec![
            kv("schema", s(schema)),
            kv("sources", PyValue::Dict(rows)),
        ]);
        let p = out
            .join("truth")
            .join(format!("{}.{}.json.gz", entry.id, kind.prefix()));
        super::gz::write_gz(&p, crate::json::py_text(&doc, true).as_bytes())?;
        shas.insert(entry.id.clone(), super::corpus::sha256_file(&p)?);
    }
    let mut man = vec![
        kv("schema", s(schema)),
        kv("corpusSha256", s(&corpus.loaded.manifest_id)),
        kv(
            "entries",
            dict(shas.iter().map(|(k, v)| (k.clone(), s(v))).collect()),
        ),
        kv("about", s(about)),
    ];
    if kind == LayerKind::Chained9 {
        let verdicts = dict(
            npm.verdicts
                .iter()
                .map(|(k, v)| (k.clone(), PyValue::Bool(*v)))
                .collect(),
        );
        if corpus.is_private() {
            let sha =
                crate::committed::sha256_hex(crate::json::py_text(&verdicts, true).as_bytes());
            man.push(kv(
                "npmVerdicts",
                dict(vec![
                    kv("count", super::py::int(npm.verdicts.len() as i64)),
                    kv("sha256", s(&sha)),
                ]),
            ));
        } else {
            man.push(kv("npmVerdicts", verdicts));
        }
    }
    Ok((shas, crate::json::py_text(&dict(man), true)))
}
