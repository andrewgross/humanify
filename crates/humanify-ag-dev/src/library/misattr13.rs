//! Scorer 13's truth layer `misattr13` (experiments/132-factory-precision
//! /scorer13/misattr13.py, schema `exp132-misattr13/4`): the bundled
//! CommonJS modules a source map credits to a file that does not hold
//! them, relabelled to the file that does. Byte for byte what the Python
//! froze: the rule, its constants and its file walk are unchanged (see the
//! Python's docstring for the rule).
//!
//! Inputs: the frozen bundle, map and truth (scorer 5/9/11's sources,
//! scorer 6's segments), the installed files of the packages the map
//! names, and the Factory spans of one reference run (`unitsLabel`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::corpus::{Corpus, Role, read_map};
use super::counter::Counter;
use super::index::Unit;
use super::py::{dict, int, kv, opt_s, s as pys};
use super::pypath;
use super::pyre;
use super::truth::{self, Truth};
use crate::committed::library::CorpusEntry;
use crate::json::py::PyValue;

pub const SCHEMA: &str = "exp132-misattr13/4";
const MIN_TOKENS: usize = 5;
const MIN_STRINGS: usize = 3;
const OWN: f64 = 0.5;
const FOUND: f64 = 0.8;
const RARE_MIN: f64 = 5.0;
const RARE_SHARE: f64 = 0.02;
const EXTS: [&str; 4] = [".js", ".cjs", ".mjs", ".json"];
const MAX_FILE: u64 = 16 << 20;
const SIZE_FACTOR: i64 = 8;
const SIZE_SLACK: i64 = 8192;
pub const ABOUT: &str = "scorer 13 rule `misattributed` (experiments/132-factory-precision/scorer13/misattr13.py): per entry, the sha256 of <DATA>/truth/<id>.misattr13.json.gz (rows: factory start, end, old package, new package, version, file, evidence, old file)";

/// The manifest's `rules` block.
pub fn rules() -> PyValue {
    use super::py::flt;
    dict(vec![
        kv("minTokens", int(MIN_TOKENS as i64)),
        kv("minStrings", int(MIN_STRINGS as i64)),
        kv("own", flt(OWN)),
        kv("found", flt(FOUND)),
        kv("rareMin", int(RARE_MIN as i64)),
        kv("rareShare", flt(RARE_SHARE)),
        kv("sizeFactor", int(SIZE_FACTOR)),
        kv("sizeSlack", int(SIZE_SLACK)),
    ])
}

/// `package_root`: (package name, root dir) of a path under node_modules.
fn package_root(path: &str) -> Option<(String, String)> {
    let k = path.rfind("/node_modules/")?;
    let (head, rest) = (&path[..k], &path[k + "/node_modules/".len()..]);
    let parts: Vec<&str> = rest.split('/').collect();
    let n = if parts[0].starts_with('@') && parts.len() > 1 {
        2
    } else {
        1
    };
    let name = parts[..n].join("/");
    let mut root = pypath::join(head, "node_modules");
    for p in &parts[..n] {
        root = pypath::join(&root, p);
    }
    Some((name, root))
}

/// `os.walk(root)` skipping `node_modules` dirs, sorted; files ending in
/// one of `EXTS`, not symlinks, at most `MAX_FILE` bytes.
fn walk_package(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<(std::ffi::OsString, bool)> = Vec::new();
    let mut files: Vec<std::ffi::OsString> = Vec::new();
    for e in read.flatten() {
        let name = e.file_name();
        let is_link = e.file_type().is_ok_and(|t| t.is_symlink());
        if e.path().is_dir() {
            if name != "node_modules" {
                dirs.push((name, is_link));
            }
        } else {
            files.push(name);
        }
    }
    files.sort();
    dirs.sort();
    for n in files {
        let p = dir.join(&n);
        let ns = n.to_string_lossy();
        if !EXTS.iter().any(|x| ns.ends_with(x)) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.file_type().is_symlink() || meta.len() > MAX_FILE {
            continue;
        }
        out.push(p);
    }
    for (d, is_link) in dirs {
        if !is_link {
            walk_package(&dir.join(d), out);
        }
    }
}

/// `Installed`: the installed files of the packages a bundle's map names,
/// read once (lazily) per entry.
struct Installed {
    roots: BTreeMap<String, String>,
    files: Option<Vec<(String, String, PathBuf)>>,
}

impl Installed {
    fn new(base: &str, srcs: &[Value]) -> Installed {
        let mut roots = BTreeMap::new();
        for x in srcs {
            if let Value::String(x) = x
                && let Some((pkg, root)) = package_root(&pypath::normpath(&pypath::join(base, x)))
                && Path::new(&root).is_dir()
            {
                roots.insert(root, pkg);
            }
        }
        Installed { roots, files: None }
    }

    fn files(&mut self) -> &[(String, String, PathBuf)] {
        if self.files.is_none() {
            let mut out = Vec::new();
            for (root, pkg) in &self.roots {
                let mut ps = Vec::new();
                walk_package(Path::new(root), &mut ps);
                out.extend(ps.into_iter().map(|p| (pkg.clone(), root.clone(), p)));
            }
            self.files = Some(out);
        }
        self.files.as_deref().unwrap_or(&[])
    }

    /// `holders(needles, need, not_pkg, only_pkg)`: (file bytes, path,
    /// package, root) of each file holding at least `need` needles, sorted.
    fn holders(
        &mut self,
        needles: &[Vec<u8>],
        need: f64,
        not_pkg: Option<&str>,
        only_pkg: Option<&str>,
    ) -> Vec<(usize, PathBuf, String, String)> {
        let mut out = Vec::new();
        for (pkg, root, p) in self.files() {
            if Some(pkg.as_str()) == not_pkg || only_pkg.is_some_and(|o| o != pkg) {
                continue;
            }
            let Ok(b) = std::fs::read(p) else {
                continue;
            };
            let n = needles
                .iter()
                .filter(|x| memchr::memmem::find(&b, x).is_some())
                .count();
            if n as f64 >= need {
                out.push((b.len(), p.clone(), pkg.clone(), root.clone()));
            }
        }
        out.sort();
        out
    }
}

fn module_sized(file_bytes: usize, module_bytes: i64) -> bool {
    file_bytes as i64 <= SIZE_FACTOR * module_bytes + SIZE_SLACK
}

/// `version_of`: package.json's `version`, as JSON (None when unreadable).
fn version_of(root: &str) -> PyValue {
    let p = Path::new(root).join("package.json");
    let Ok(text) = std::fs::read_to_string(&p) else {
        return PyValue::Null;
    };
    match PyValue::parse(&text) {
        Ok(PyValue::Dict(d)) => d
            .into_iter()
            .rev()
            .find(|(k, _)| k == "version")
            .map_or(PyValue::Null, |(_, v)| v),
        _ => PyValue::Null,
    }
}

/// The majority source of [s, e) by mapped bytes (ties: lowest index).
pub fn majority(segs: &[(i64, i64)], offs: &[i64], s: i64, e: i64) -> Option<i64> {
    let mut k = offs.partition_point(|&o| o <= s).saturating_sub(1);
    let mut votes: Counter<i64> = Counter::new();
    while k < segs.len() && segs[k].0 < e {
        let a = segs[k].0.max(s);
        let b = if k + 1 < segs.len() { segs[k + 1].0 } else { e }.min(e);
        if b > a {
            votes.add(segs[k].1, b - a);
        }
        k += 1;
    }
    votes
        .items()
        .min_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(y.0)))
        .map(|(k, _)| *k)
}

/// Python's `bytes[s:e]`.
fn slice(b: &[u8], s: i64, e: i64) -> &[u8] {
    let fix = |i: i64| -> usize {
        let i = if i < 0 { i + b.len() as i64 } else { i };
        i.clamp(0, b.len() as i64) as usize
    };
    let (a, z) = (fix(s), fix(e));
    &b[a..z.max(a)]
}

/// `STRING.finditer(body)`: `"([^"\\\n]{20,})"|'([^'\\\n]{20,})'` over
/// bytes, the captured contents.
fn long_strings(body: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < body.len() {
        let q = body[k];
        if q == b'"' || q == b'\'' {
            let mut j = k + 1;
            while j < body.len() && body[j] != q && body[j] != b'\\' && body[j] != b'\n' {
                j += 1;
            }
            if j - k > 20 && j < body.len() && body[j] == q {
                out.push(body[k + 1..j].to_vec());
                k = j + 1;
                continue;
            }
        }
        k += 1;
    }
    out
}

/// One entry's evidence (misattr13.py `entry_layer`'s locals).
struct Evidence<'a> {
    srcs: &'a [super::truth::Src],
    tok: BTreeMap<usize, BTreeSet<String>>,
    raw: HashMap<usize, Vec<u8>>,
    df: HashMap<String, usize>,
    common: f64,
    text: Vec<u8>,
    installed: Installed,
    by_pkg: HashMap<String, (BTreeSet<String>, Vec<usize>)>,
}

impl Evidence<'_> {
    /// `{x for x in TOKEN.findall(span) if 0 < df[x] <= common}`.
    fn rare_tokens(&self, s: i64, e: i64) -> BTreeSet<String> {
        let t = String::from_utf8_lossy(slice(&self.text, s, e)).into_owned();
        pyre::tokens(&t)
            .into_iter()
            .filter(|x| {
                let n = self.df.get(*x).copied().unwrap_or(0);
                n > 0 && n as f64 <= self.common
            })
            .map(String::from)
            .collect()
    }

    /// `matches(span)`: the span holds enough of source `si`'s rare tokens.
    fn holds_source(&self, span: (i64, i64), si: usize) -> bool {
        let tt = self.rare_tokens(span.0, span.1);
        tt.len() >= MIN_TOKENS
            && tt.intersection(&self.tok[&si]).count() as f64 >= OWN * tt.len() as f64
    }

    /// (tokens, sources) of package `pkg` among the map's sources.
    fn package(&mut self, pkg: &str) -> &(BTreeSet<String>, Vec<usize>) {
        if !self.by_pkg.contains_key(pkg) {
            let mine: Vec<usize> = self
                .tok
                .keys()
                .copied()
                .filter(|sj| *sj < self.srcs.len() && self.srcs[*sj].label == pkg)
                .collect();
            let mut set = BTreeSet::new();
            for sj in &mine {
                set.extend(self.tok[sj].iter().cloned());
            }
            self.by_pkg.insert(pkg.to_string(), (set, mine));
        }
        &self.by_pkg[pkg]
    }

    /// The token evidence's relabel: the smallest map source of another
    /// package holding the factory's rare tokens.
    fn by_tokens(&mut self, t: &BTreeSet<String>, pkg: &str, size: i64) -> Option<Vec<PyValue>> {
        if t.len() < MIN_TOKENS {
            return None;
        }
        let own = t.intersection(&self.package(pkg).0).count() as f64 / t.len() as f64;
        if own >= OWN {
            return None;
        }
        let need = FOUND.max(own + OWN) * t.len() as f64;
        let srcs = self.srcs;
        let sj = self
            .tok
            .iter()
            .filter(|(sj, tj)| {
                **sj < srcs.len()
                    && !srcs[**sj].label.starts_with('<')
                    && srcs[**sj].label != pkg
                    && t.intersection(tj).count() as f64 >= need
                    && module_sized(self.raw[*sj].len(), size)
            })
            .map(|(sj, _)| *sj)
            .min_by_key(|x| (self.raw[x].len(), *x))?;
        let r = &srcs[sj];
        Some(vec![
            pys(&r.label),
            opt_s(r.version.as_deref()),
            opt_s(r.file.as_deref()),
            pys("map"),
        ])
    }

    /// The data-module evidence's relabel: the smallest installed file of
    /// another package holding the factory's long strings.
    fn by_strings(&mut self, strs: &[Vec<u8>], pkg: &str, size: i64) -> Option<Vec<PyValue>> {
        if strs.len() < MIN_STRINGS {
            return None;
        }
        let mine = self.package(pkg).1.clone();
        let raw = &self.raw;
        let own = strs
            .iter()
            .filter(|x| {
                mine.iter()
                    .any(|sj| memchr::memmem::find(&raw[sj], x).is_some())
            })
            .count() as f64
            / strs.len() as f64;
        if own >= OWN {
            return None;
        }
        let need = FOUND.max(own + OWN) * strs.len() as f64;
        let (_, p, hp, root) = self
            .installed
            .holders(strs, need, Some(pkg), None)
            .into_iter()
            .find(|h| module_sized(h.0, size))?;
        let rel = pypath::relpath(&p.to_string_lossy(), &root);
        Some(vec![
            pys(&hp),
            version_of(&root),
            pys(&rel),
            pys("installed"),
        ])
    }
}

/// `factories` with their majority source, once per span (a later
/// factory of the same span replaces the source, keeping the position).
fn majorities(units: &[Unit], segs: &[(i64, i64)]) -> Vec<((i64, i64), i64)> {
    let offs: Vec<i64> = segs.iter().map(|x| x.0).collect();
    let mut major: Vec<((i64, i64), i64)> = Vec::new();
    let mut seen: HashMap<(i64, i64), usize> = HashMap::new();
    for u in units.iter().filter(|u| u.k == "Factory" && !u.rt) {
        let Some(si) = majority(segs, &offs, u.s, u.e) else {
            continue;
        };
        match seen.get(&(u.s, u.e)) {
            Some(&i) => major[i].1 = si,
            None => {
                seen.insert((u.s, u.e), major.len());
                major.push(((u.s, u.e), si));
            }
        }
    }
    major
}

fn evidence<'a>(
    corpus: &Corpus,
    entry: &CorpusEntry,
    truth: &'a Truth,
) -> Result<Evidence<'a>, String> {
    let smap = read_map(&corpus.source_map_path(entry)?)?;
    let mut tok: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut raw: HashMap<usize, Vec<u8>> = HashMap::new();
    for (si, c) in smap.sources_content.iter().flatten().enumerate() {
        if let Some(c) = c.as_deref().filter(|c| !c.is_empty()) {
            tok.insert(si, pyre::tokens(c).into_iter().map(String::from).collect());
            raw.insert(si, c.as_bytes().to_vec());
        }
    }
    let mut df: HashMap<String, usize> = HashMap::new();
    for x in tok.values().flatten() {
        *df.entry(x.clone()).or_default() += 1;
    }
    let bpath = corpus.corpus_path(entry, Role::Bundle)?;
    let base = pypath::dirname(&entry.files.bundle.origin);
    Ok(Evidence {
        srcs: &truth.sources,
        common: RARE_MIN.max(RARE_SHARE * tok.len() as f64),
        tok,
        raw,
        df,
        text: std::fs::read(&bpath).map_err(|e| format!("{}: {e}", bpath.display()))?,
        installed: Installed::new(&base, &smap.sources),
        by_pkg: HashMap::new(),
    })
}

/// One entry's rows (misattr13.py `entry_layer`).
fn entry_rows(
    corpus: &Corpus,
    entry: &CorpusEntry,
    truth: &Truth,
    units: &[Unit],
) -> Result<Vec<PyValue>, String> {
    let mut ev = evidence(corpus, entry, truth)?;
    let major = majorities(units, &truth.segments);
    let mut credited: HashMap<i64, usize> = HashMap::new();
    let mut sharing: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for (span, si) in &major {
        *credited.entry(*si).or_default() += 1;
        sharing.entry(*si).or_default().push(*span);
    }
    let mut sorted = major.clone();
    sorted.sort();
    let mut rows = Vec::new();
    for ((s, e), si) in sorted {
        let siu = si as usize;
        let pkg = truth
            .sources
            .get(siu)
            .map_or("<unmapped>", |x| x.label.as_str())
            .to_string();
        if credited[&si] < 2 || pkg.starts_with('<') || !ev.tok.contains_key(&siu) {
            continue;
        }
        // S's own module is elsewhere in the bundle
        if !sharing[&si]
            .iter()
            .any(|sp| *sp != (s, e) && ev.holds_source(*sp, siu))
        {
            continue;
        }
        ev.package(&pkg);
        let t = ev.rare_tokens(s, e);
        let mut strs = long_strings(slice(&ev.text, s, e));
        strs.sort();
        strs.dedup();
        if strs.len() >= MIN_STRINGS
            && !ev
                .installed
                .holders(&strs, OWN * strs.len() as f64, None, Some(&pkg))
                .is_empty()
        {
            continue;
        }
        let new = match ev.by_tokens(&t, &pkg, e - s) {
            Some(n) => Some(n),
            None => ev.by_strings(&strs, &pkg, e - s),
        };
        let Some(new) = new else {
            continue;
        };
        let mut row = vec![int(s), int(e), pys(&pkg)];
        row.extend(new);
        row.push(opt_s(truth.sources[siu].file.as_deref()));
        rows.push(PyValue::List(row));
    }
    Ok(rows)
}

/// Build the layer for every source-mapped entry of `corpus` into
/// `out/truth/<id>.misattr13.json.gz`; (entry id -> sha, manifest text).
pub fn build(
    corpus: &Corpus,
    units_label: &str,
    out: &Path,
) -> Result<(BTreeMap<String, String>, String), String> {
    let mut shas = BTreeMap::new();
    for entry in corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
    {
        let units = super::score::label_units(corpus, units_label, entry)?.unwrap_or_default();
        let mut truth = truth::load_truth(corpus, entry)?;
        truth::redecode(corpus, entry, &mut truth, None)?;
        let rows = entry_rows(corpus, entry, &truth, &units)?;
        let doc = dict(vec![
            kv("schema", pys(SCHEMA)),
            kv("rows", PyValue::List(rows)),
        ]);
        let p = out
            .join("truth")
            .join(format!("{}.misattr13.json.gz", entry.id));
        super::gz::write_gz(&p, crate::json::py_text(&doc, true).as_bytes())?;
        shas.insert(entry.id.clone(), super::corpus::sha256_file(&p)?);
    }
    let man = dict(vec![
        kv("schema", pys(SCHEMA)),
        kv("corpusSha256", pys(&corpus.loaded.manifest_id)),
        kv("unitsLabel", pys(units_label)),
        kv(
            "entries",
            dict(shas.iter().map(|(k, v)| (k.clone(), pys(v))).collect()),
        ),
        kv("rules", rules()),
        kv("about", pys(ABOUT)),
    ]);
    Ok((shas, crate::json::py_text(&man, true)))
}

#[cfg(test)]
#[path = "misattr13_test.rs"]
mod misattr13_test;
