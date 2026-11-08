//! Scorer 12's truth layer `copies2` (experiments/129-helpers-copies/
//! copies2.py, schema `exp129-copies/2`): the library units that are a
//! COPY of another package's code, derived from the installed packages
//! and the index's evidence over every judged unit of a corpus. Byte for
//! byte what the Python froze (its three steps `hashes`, `rows`, `derive`
//! in one call; see its docstring for the rule).
//!
//! Inputs: the units of the reference run(s) (`unitsFrom`), the index's
//! unit tables (every slice) and helper table, the frozen bundle, map and
//! truth, the installed package.json files and the npm manifest caches.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::corpus::{Corpus, read_map};
use super::counter::Counter;
use super::families::Families;
use super::index::{Print, Unit, greedy_cover, hash_key};
use super::py::{compact, dict, flt, int, kv, round_n, s};
use super::pypath;
use super::pyre::is_word;
use super::truth::{self, Audit, AuditLayers, Label, NOT_LIBRARY};
use crate::committed::library::CorpusEntry;
use crate::json::py::PyValue;

pub const SCHEMA: &str = "exp129-copies/2";
const COVER: f64 = 0.5;
const MIN_NODES: i64 = 30;
const MAX_FAMILIES: usize = 3;
/// The manifest caches the frozen exp104 layers read: the fallback when
/// no install resolves.
pub const REGISTRIES: [&str; 4] = [
    "/work/exp104-data/registry",
    "/work/exp107-data/registry",
    "/work/exp112-data/registry",
    "/work/exp116-test2-data/registry",
];
const DEP_KEYS: [&str; 6] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
    "bundleDependencies",
    "bundledDependencies",
];
/// Where an index named by its basename in a committed manifest lives.
pub const INDEX_ROOT: &str = "/work/library-index";

/// `judged_units`: statement units, lazy bodies and factory body
/// statements that carry prints, bundler runtime excluded.
fn judged(units: Vec<Unit>) -> Vec<Unit> {
    units
        .into_iter()
        .filter(|u| {
            matches!(u.k.as_str(), "Statement" | "LazyBody" | "FactoryBody")
                && !u.rt
                && u.w.is_some()
        })
        .collect()
}

fn prints(u: &Unit) -> (&Print, &[Print]) {
    let (w, f) = u.w.as_ref().expect("a judged unit has prints");
    (w, f)
}

fn print_hashes(u: &Unit) -> impl Iterator<Item = u64> + '_ {
    let (w, f) = prints(u);
    std::iter::once(w)
        .filter(|w| w.nodes >= MIN_NODES)
        .chain(f.iter().filter(|f| f.nodes >= MIN_NODES))
        .map(|p| p.h)
}

/// `entry_units`: the judged units of every units label's run of
/// `entry`, once per span, renumbered; None when no run discovered
/// anything.
fn entry_units(
    corpus: &Corpus,
    labels: &[String],
    entry: &CorpusEntry,
) -> Result<Option<Vec<Unit>>, String> {
    let mut out: Vec<Unit> = Vec::new();
    let mut seen = HashSet::new();
    for label in labels {
        if !super::score::report_path(corpus, label, entry).exists() {
            continue;
        }
        let Some(units) = super::score::label_units(corpus, label, entry)? else {
            continue;
        };
        for mut u in judged(units) {
            if seen.insert((u.s, u.e)) {
                u.i = out.len();
                out.push(u);
            }
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

/// One row table: print hash -> its owners.
type Table = HashMap<u64, HashSet<String>>;

/// A row table with the commonness bound applied (`owners()`): a print
/// held by more than `MAX_FAMILIES` families holds no owner.
type Bounded = HashMap<u64, Vec<String>>;

fn bounded(t: Table, fams: &Families) -> Bounded {
    t.into_iter()
        .map(|(h, o)| {
            let n: HashSet<String> = o.iter().map(|x| fams.libix.family(x)).collect();
            let keep = if n.len() <= MAX_FAMILIES {
                o.into_iter().collect()
            } else {
                Vec::new()
            };
            (h, keep)
        })
        .collect()
}

#[derive(Deserialize)]
struct UnitLine {
    owner: String,
}

#[derive(Deserialize)]
struct HelperLine {
    hash: String,
    owners: Vec<String>,
}

/// `slice_rows`: the rows of one `units.<slice>.jsonl` holding any of
/// `hashes` (`line[9:25]`: the merge writes `hash` first).
fn slice_rows(path: &Path, hashes: &HashSet<u64>) -> Result<Table, String> {
    let fh = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = std::io::BufReader::with_capacity(1 << 22, fh);
    let mut rows: Table = HashMap::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        let Some(h) = line
            .get(9..25)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| hash_key(h).ok())
        else {
            continue;
        };
        if hashes.contains(&h) {
            let r: UnitLine =
                serde_json::from_slice(&line).map_err(|e| format!("{}: {e}", path.display()))?;
            rows.entry(h).or_default().insert(r.owner);
        }
    }
    Ok(rows)
}

fn helper_rows(ix: &Path, hashes: &HashSet<u64>) -> Result<Table, String> {
    let mut rows: Table = HashMap::new();
    let p = ix.join("helpers.jsonl");
    if !p.exists() {
        return Ok(rows);
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    for line in text.lines() {
        let r: HelperLine =
            serde_json::from_str(line).map_err(|e| format!("{}: {e}", p.display()))?;
        let h = hash_key(&r.hash)?;
        if hashes.contains(&h) {
            rows.entry(h).or_default().extend(r.owners);
        }
    }
    Ok(rows)
}

/// The evidence tables: one per (index, slice) plus each index's helper
/// table, in path order; and each index's identity (sha of manifest.json).
struct Tables {
    paths: Vec<String>,
    tables: Vec<Table>,
    indexes: BTreeMap<String, String>,
}

fn load_tables(ixs: &[PathBuf], hashes: &HashSet<u64>) -> Result<Tables, String> {
    let mut todo: Vec<PathBuf> = Vec::new();
    for ix in ixs {
        let mut names: Vec<String> = std::fs::read_dir(ix)
            .map_err(|e| format!("{}: {e}", ix.display()))?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|f| f.starts_with("units.") && f.ends_with(".jsonl"))
            .collect();
        names.sort();
        todo.extend(names.iter().map(|f| ix.join(f)));
    }
    let scanned = humanify_ag_core::par::map_ordered(&todo, |p| slice_rows(p, hashes));
    let mut by_path: BTreeMap<String, Table> = BTreeMap::new();
    for (p, r) in todo.iter().zip(scanned) {
        by_path.insert(p.to_string_lossy().to_string(), r?);
    }
    let mut indexes = BTreeMap::new();
    for ix in ixs {
        let h = helper_rows(ix, hashes)?;
        if !h.is_empty() {
            by_path.insert(ix.join("helpers.jsonl").to_string_lossy().to_string(), h);
        }
        let name = ix
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        indexes.insert(name, super::corpus::sha256_file(&ix.join("manifest.json"))?);
    }
    let (paths, tables) = by_path.into_iter().unzip();
    Ok(Tables {
        paths,
        tables,
        indexes,
    })
}

/// The rule's evidence context, shared by every entry.
struct Ctx<'a> {
    fams: &'a Families,
    tables: &'a [Bounded],
}

impl Ctx<'_> {
    fn same(&self, a: &str, b: &str) -> bool {
        self.fams.same(Some(a), Some(b))
    }

    /// `declared`: the same package, a derived link, or one base family
    /// that is not merely one npm scope.
    fn declared(&self, dep: &str, origin: &str) -> bool {
        if dep == origin || self.fams.linked(dep, origin) {
            return true;
        }
        let (a, b) = (self.fams.libix.family(dep), self.fams.libix.family(origin));
        a == b && !a.starts_with("scope:")
    }

    /// `owner_cover`: {owner: nodes its family's rows cover}, the best
    /// over the row tables.
    fn owner_cover(&self, u: &Unit) -> HashMap<String, i64> {
        let (whole, fns) = prints(u);
        let mut best: HashMap<String, i64> = HashMap::new();
        for rows in self.tables {
            let owners = |h: u64| -> HashSet<&str> {
                rows.get(&h)
                    .map(|o| o.iter().map(String::as_str).collect())
                    .unwrap_or_default()
            };
            let whole_owners = if whole.nodes >= MIN_NODES {
                owners(whole.h)
            } else {
                HashSet::new()
            };
            let fn_owners: Vec<(&Print, HashSet<&str>)> = fns
                .iter()
                .filter(|f| f.nodes >= MIN_NODES)
                .map(|f| (f, owners(f.h)))
                .collect();
            let mut cands: BTreeSet<&str> = whole_owners.iter().copied().collect();
            for (_, o) in &fn_owners {
                cands.extend(o.iter().copied());
            }
            for o in cands {
                let n = if whole_owners.iter().any(|w| self.same(o, w)) {
                    whole.nodes
                } else {
                    greedy_cover(
                        fn_owners
                            .iter()
                            .filter(|(_, os)| os.iter().any(|x| self.same(o, x)))
                            .map(|(f, _)| **f)
                            .collect(),
                    )
                };
                let slot = best.entry(o.to_string()).or_insert(0);
                *slot = (*slot).max(n);
            }
        }
        best
    }
}

/// Python's `str(v)` of a JSON value (package.json `version`, the truth's).
fn py_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".into(),
        Some(Value::String(x)) => x.clone(),
        Some(Value::Bool(true)) => "True".into(),
        Some(Value::Bool(false)) => "False".into(),
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => n.to_string(),
        Some(Value::Number(n)) => crate::json::py::float_repr(n.as_f64().unwrap_or(f64::NAN)),
        Some(other) => other.to_string(),
    }
}

fn read_deps(d: &serde_json::Map<String, Value>) -> HashSet<String> {
    let mut out = HashSet::new();
    for k in DEP_KEYS {
        match d.get(k) {
            Some(Value::Array(xs)) => {
                out.extend(xs.iter().filter_map(Value::as_str).map(String::from))
            }
            Some(Value::Object(m)) => out.extend(m.keys().cloned()),
            _ => {}
        }
    }
    out
}

fn read_obj(p: &Path) -> Option<serde_json::Map<String, Value>> {
    let text = std::fs::read_to_string(p).ok()?;
    match serde_json::from_str::<Value>(&super::corpus::lone_surrogates_replaced(&text)) {
        Ok(Value::Object(m)) => Some(m),
        Ok(_) => None,
        Err(_) => Some(serde_json::Map::new()),
    }
}

/// `manifest_deps`: (declared dependencies, where read).
fn manifest_deps(
    root: Option<&str>,
    pkg: &str,
    ver: Option<&str>,
) -> Option<(HashSet<String>, &'static str)> {
    let ver_s = ver.map_or_else(|| "None".to_string(), String::from);
    if let Some(root) = root {
        let p = Path::new(root).join("package.json");
        if p.exists() {
            let d = read_obj(&p).unwrap_or_default();
            if d.get("name").and_then(Value::as_str) == Some(pkg)
                && py_str(d.get("version")) == ver_s
            {
                return Some((read_deps(&d), "installed"));
            }
        }
    }
    for reg in REGISTRIES {
        let p = Path::new(reg).join(format!("{}@{ver_s}.json", pkg.replace('/', "__")));
        if p.exists()
            && let Some(d) = read_obj(&p)
            && !d.contains_key("__error")
        {
            return Some((read_deps(&d), "registry"));
        }
    }
    None
}

/// `install_root`: node_modules/<pkg> on the source's path (the LAST such
/// segment), or None.
fn install_root(source_path: &str, pkg: &str) -> Option<String> {
    let marker = format!("/node_modules/{pkg}/");
    let k = source_path.rfind(&marker)?;
    Some(source_path[..k + marker.len() - 1].to_string())
}

fn char_before(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

fn char_at(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

/// Python `\b` before position `at` where the next char is a word char.
fn word_start(text: &str, at: usize) -> bool {
    !char_before(text, at).is_some_and(is_word)
}

/// `\s*` (or `\s+` when `min` is 1) from `at`: the end, or None.
fn spaces(text: &str, at: usize, min: usize) -> Option<usize> {
    let mut n = 0;
    let mut k = at;
    while let Some(c) = char_at(text, k) {
        if !super::pyre::is_space(c) {
            break;
        }
        k += c.len_utf8();
        n += 1;
    }
    (n >= min).then_some(k)
}

/// After a module specifier's opening quote `q` at `at`: `name(?:/[^'"`]*)?\1`;
/// the end of the match, or None.
fn specifier_tail(text: &str, at: usize, q: char, name: &str) -> Option<usize> {
    let rest = text[at..].strip_prefix(name)?;
    let k = at + name.len();
    if rest.starts_with(q) {
        return Some(k + q.len_utf8());
    }
    if rest.starts_with('/') {
        let mut j = k + 1;
        while let Some(c) = char_at(text, j) {
            if c == '\'' || c == '"' || c == '`' {
                return (c == q).then_some(j + 1);
            }
            j += c.len_utf8();
        }
    }
    None
}

/// The static form at `at`: `(?:\bfrom\s*|\bimport\s+)(['"])name(?:/[^'"`]*)?\1`.
fn static_at(text: &str, at: usize, name: &str) -> Option<usize> {
    for (kw, min) in [("from", 0), ("import", 1)] {
        if !text[at..].starts_with(kw) || !word_start(text, at) {
            continue;
        }
        let Some(k) = spaces(text, at + kw.len(), min) else {
            continue;
        };
        if let Some(q) = char_at(text, k).filter(|c| *c == '\'' || *c == '"')
            && let Some(end) = specifier_tail(text, k + 1, q, name)
        {
            return Some(end);
        }
    }
    None
}

/// The call form at `at`: `(?:\brequire\s*\(\s*|\bimport\s*\(\s*)(['"`])name(?:/[^'"`]*)?\1`.
fn call_at(text: &str, at: usize, name: &str) -> Option<usize> {
    for kw in ["require", "import"] {
        if !text[at..].starts_with(kw) || !word_start(text, at) {
            continue;
        }
        let Some(k) = spaces(text, at + kw.len(), 0) else {
            continue;
        };
        if char_at(text, k) != Some('(') {
            continue;
        }
        let Some(k) = spaces(text, k + 1, 0) else {
            continue;
        };
        if let Some(q) = char_at(text, k).filter(|c| matches!(c, '\'' | '"' | '`'))
            && let Some(end) = specifier_tail(text, k + 1, q, name)
        {
            return Some(end);
        }
    }
    None
}

/// `re.sub(pattern, " ", text)` for a matcher returning a match's end.
fn sub(text: &str, matcher: impl Fn(&str, usize) -> Option<usize>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut k = 0;
    let mut last = 0;
    while k < text.len() {
        if let Some(end) = matcher(text, k) {
            out.push_str(&text[last..k]);
            out.push(' ');
            k = end;
            last = end;
            continue;
        }
        k += char_at(text, k).map_or(1, char::len_utf8);
    }
    out.push_str(&text[last..]);
    out
}

/// `named`: the shipped text names `name` as a word, not counting a module
/// specifier.
pub fn named(text: &str, name: &str) -> bool {
    if !name.is_empty() && !text.contains(name) {
        return false;
    }
    let t = sub(text, |t, k| static_at(t, k, name));
    let t = sub(&t, |t, k| call_at(t, k, name));
    let edge = |c: Option<char>| c.is_some_and(|c| is_word(c) || c == '-');
    // every start, overlapping ones included (Python's search)
    let mut from = 0;
    while let Some(i) = t[from..].find(name) {
        let at = from + i;
        if !edge(char_before(&t, at)) && !edge(char_at(&t, at + name.len())) {
            return true;
        }
        match char_at(&t, at) {
            Some(c) => from = at + c.len_utf8(),
            None => break,
        }
    }
    false
}

/// `source_paths`: each map source joined onto the map's origin dir.
fn source_paths(entry: &CorpusEntry, sources: &[Value], source_root: &str) -> Vec<String> {
    let base = entry
        .files
        .map
        .as_ref()
        .map_or_else(String::new, |m| pypath::dirname(&m.origin));
    sources
        .iter()
        .map(|x| {
            let x = x.as_str().unwrap_or("");
            pypath::normpath(&pypath::join(&pypath::join(&base, source_root), x))
        })
        .collect()
}

fn shipped_text(contents: &[Option<String>], paths: &[String], si: usize) -> String {
    if let Some(Some(c)) = contents.get(si)
        && !c.is_empty()
    {
        return c.clone();
    }
    if let Some(p) = paths.get(si)
        && Path::new(p).is_file()
        && let Ok(b) = std::fs::read(p)
    {
        return String::from_utf8_lossy(&b).into_owned();
    }
    String::new()
}

#[derive(Deserialize)]
struct MapExtra {
    #[serde(rename = "sourceRoot", default)]
    source_root: Option<Value>,
}

/// One entry's layer document and stats (copies2.py `entry_copies`).
fn entry_copies(
    corpus: &Corpus,
    entry: &CorpusEntry,
    labels: &[String],
    ctx: &Ctx,
) -> Result<(PyValue, Counter<String>), String> {
    let mut rows: Vec<(i64, i64, String, String, String, f64)> = Vec::new();
    let mut stats: Counter<String> = Counter::new();
    if let Some(units) = entry_units(corpus, labels, entry)? {
        let mut truth = truth::load_truth(corpus, entry)?;
        let o = truth::load_origins(corpus, entry)?;
        let origins = truth::redecode(corpus, entry, &mut truth, Some(o))?;
        let offs: Vec<i64> = truth.segments.iter().map(|x| x.0).collect();
        let mp = corpus.source_map_path(entry)?;
        let m = read_map(&mp)?;
        let contents = m.sources_content.clone().unwrap_or_default();
        let paths = if entry.files.inner_map.is_none() {
            let text =
                std::fs::read_to_string(&mp).map_err(|e| format!("{}: {e}", mp.display()))?;
            let extra: MapExtra =
                serde_json::from_str(&super::corpus::lone_surrogates_replaced(&text))
                    .map_err(|e| format!("{}: {e}", mp.display()))?;
            let root = match extra.source_root {
                Some(Value::String(r)) => r,
                _ => String::new(),
            };
            source_paths(entry, &m.sources, &root)
        } else {
            Vec::new()
        };
        let audit = Audit::with(
            corpus,
            entry,
            &truth,
            AuditLayers {
                copies: false,
                misattr13: false,
                mapdefect14: false,
            },
        )?;
        let subset: Vec<Unit> = units
            .iter()
            .cloned()
            .map(|mut u| {
                if u.k == "FactoryBody" {
                    u.k = "Statement".into();
                }
                u
            })
            .collect();
        let scored: Vec<&Unit> = super::score::scored_units(&subset);
        let labels_of: HashMap<usize, Label> =
            truth::label_units(&truth, &subset, &scored, origins.as_ref(), &audit)?;
        for u in &units {
            let Some(lab) = labels_of.get(&u.i) else {
                continue;
            };
            if NOT_LIBRARY.contains(&lab.label.as_str()) || lab.has_origin {
                continue;
            }
            if offs.partition_point(|&o| o < u.s) == offs.partition_point(|&o| o < u.e) {
                continue;
            }
            let wn = prints(u).0.nodes.max(1) as f64;
            let cov: HashMap<String, i64> = ctx
                .owner_cover(u)
                .into_iter()
                .filter(|(o, n)| !ctx.same(o, &lab.label) && *n as f64 >= COVER * wn)
                .collect();
            if cov.is_empty() {
                continue;
            }
            stats.add("codeMatch".into(), u.b);
            let k = offs.partition_point(|&o| o <= u.s);
            let seg = if k == 0 {
                truth.segments[truth.segments.len() - 1]
            } else {
                truth.segments[k - 1]
            };
            let si = seg.1 as usize;
            let pkg = lab.pkg.as_deref().unwrap_or("");
            let root = paths.get(si).and_then(|p| install_root(p, pkg));
            let Some((deps, where_)) = manifest_deps(root.as_deref(), pkg, lab.version.as_deref())
            else {
                stats.add("noManifest".into(), u.b);
                continue;
            };
            stats.add(format!("manifest:{where_}"), u.b);
            let mut ok: Vec<(String, i64, Vec<String>)> = cov
                .into_iter()
                .filter_map(|(o, n)| {
                    let mut ds: Vec<String> = deps
                        .iter()
                        .filter(|d| ctx.declared(d, &o))
                        .cloned()
                        .collect();
                    ds.sort();
                    (!ds.is_empty()).then_some((o, n, ds))
                })
                .collect();
            if ok.is_empty() {
                stats.add("notADependency".into(), u.b);
                continue;
            }
            let text = shipped_text(&contents, &paths, si);
            ok.retain(|(o, _, ds)| std::iter::once(o).chain(ds.iter()).any(|x| named(&text, x)));
            if ok.is_empty() {
                stats.add("notNamed".into(), u.b);
                continue;
            }
            let best = ok.iter().map(|x| x.1).max().unwrap_or(0);
            let mut top: Vec<&(String, i64, Vec<String>)> =
                ok.iter().filter(|x| x.1 == best).collect();
            top.sort_by(|a, b| a.0.cmp(&b.0));
            if top[1..].iter().any(|o| !ctx.same(&top[0].0, &o.0)) {
                stats.add("tie".into(), u.b);
                continue;
            }
            stats.add("copy".into(), u.b);
            rows.push((
                u.s,
                u.e,
                lab.label.clone(),
                top[0].0.clone(),
                top[0].2[0].clone(),
                round_n(best as f64 / wn, 4),
            ));
        }
    }
    rows.sort_by(|a, b| {
        (a.0, a.1, &a.2, &a.3, &a.4)
            .cmp(&(b.0, b.1, &b.2, &b.3, &b.4))
            .then(a.5.total_cmp(&b.5))
    });
    let ranges = PyValue::List(
        rows.into_iter()
            .map(|r| {
                PyValue::List(vec![
                    int(r.0),
                    int(r.1),
                    s(&r.2),
                    s(&r.3),
                    s(&r.4),
                    flt(r.5),
                ])
            })
            .collect(),
    );
    let st = dict(stats.items().map(|(k, v)| (k.clone(), int(v))).collect());
    Ok((
        dict(vec![
            kv("schema", s(SCHEMA)),
            kv("ranges", ranges),
            kv("stats", st),
        ]),
        stats,
    ))
}

/// Build the layer: every source-mapped entry of `corpus` into
/// `out/truth/<id>.copies2.json.gz`, with the units of `labels` and the
/// evidence of the index dirs `ixs`; (entry id -> sha, manifest text).
pub fn build(
    corpus: &Corpus,
    labels: &[String],
    ixs: &[PathBuf],
    out: &Path,
) -> Result<(BTreeMap<String, String>, String), String> {
    let entries: Vec<&CorpusEntry> = corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
        .collect();
    // step 1: every print hash the corpus's judged units hold
    let per_entry =
        humanify_ag_core::par::map_ordered(&entries, |e| -> Result<HashSet<u64>, String> {
            let mut hs = HashSet::new();
            for label in labels {
                if !super::score::report_path(corpus, label, e).exists() {
                    continue;
                }
                if let Some(units) = super::score::label_units(corpus, label, e)? {
                    for u in judged(units) {
                        hs.extend(print_hashes(&u));
                    }
                }
            }
            Ok(hs)
        });
    let mut hashes = HashSet::new();
    for h in per_entry {
        hashes.extend(h?);
    }
    // step 2: the index rows holding them, every slice
    let tables = load_tables(ixs, &hashes)?;
    // step 3: the layer
    let fams = Families::shipped();
    let bounded_tables: Vec<Bounded> = tables
        .tables
        .into_iter()
        .map(|t| bounded(t, &fams))
        .collect();
    let ctx = Ctx {
        fams: &fams,
        tables: &bounded_tables,
    };
    let docs =
        humanify_ag_core::par::map_ordered(&entries, |e| entry_copies(corpus, e, labels, &ctx));
    let mut shas = BTreeMap::new();
    let mut tot: Counter<String> = Counter::new();
    for (e, doc) in entries.iter().zip(docs) {
        let (doc, stats) = doc?;
        for (k, v) in stats.items() {
            tot.add(k.clone(), v);
        }
        let p = out.join("truth").join(format!("{}.copies2.json.gz", e.id));
        super::gz::write_gz(&p, compact(&doc).as_bytes())?;
        shas.insert(e.id.clone(), super::corpus::sha256_file(&p)?);
    }
    let table_names: Vec<PyValue> = tables
        .paths
        .iter()
        .map(|t| s(&pypath::relpath(t, &pypath::dirname(&pypath::dirname(t)))))
        .collect();
    let man = dict(vec![
        kv("schema", s(SCHEMA)),
        kv("corpusSha256", s(&corpus.loaded.manifest_id)),
        kv("unitsFrom", s(&labels.join(","))),
        kv(
            "indexes",
            dict(
                tables
                    .indexes
                    .iter()
                    .map(|(k, v)| (k.clone(), s(v)))
                    .collect(),
            ),
        ),
        kv("tables", PyValue::List(table_names)),
        kv("cover", flt(COVER)),
        kv("minNodes", int(MIN_NODES)),
        kv("maxFamilies", int(MAX_FAMILIES as i64)),
        kv("familiesSha256", s(&super::families::derived_sha())),
        kv(
            "bytes",
            dict(tot.items().map(|(k, v)| (k.clone(), int(v))).collect()),
        ),
        kv(
            "entries",
            dict(shas.iter().map(|(k, v)| (k.clone(), s(v))).collect()),
        ),
    ]);
    Ok((shas, crate::json::py_text(&man, true)))
}

#[cfg(test)]
#[path = "copies2_test.rs"]
mod copies2_test;
