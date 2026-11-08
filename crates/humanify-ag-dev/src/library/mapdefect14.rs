//! Scorer 14's truth layer `mapdefect14` (experiments/137-scorer-mapdefect
//! /scorer14/mapdefect14.py, schema `exp137-mapdefect14/1`): a unit whose
//! source-map label names a package whose installed closure does not hold
//! the unit's name evidence, while exactly one other installed package's
//! file does, is that package's. Byte for byte what the Python froze (see
//! its docstring for the rule); the evidence is `tokens14`'s, read by the
//! ported acorn tokenizer instead of Node.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::corpus::{Corpus, Role};
use super::counter::Counter;
use super::families::Families;
use super::index::Unit;
use super::py::{dict, flt, int, kv, list_s, opt_s, round_int, s};
use super::pypath;
use super::tokens14;
use super::truth::{self, Audit, AuditLayers, Label, NOT_LIBRARY, Truth};
use crate::committed::library::CorpusEntry;
use crate::json::py::PyValue;

pub const SCHEMA: &str = "exp137-mapdefect14/1";
const MIN_EVIDENCE: usize = 8;
const DF_MAX: usize = 5;
const LACK: f64 = 0.2;
const HOLD: f64 = 0.8;
const CANDIDATE: f64 = 0.3;
const SAMPLE: usize = 64;
const FILE_CAP: usize = 1000;
const EXTS: [&str; 4] = [".js", ".cjs", ".mjs", ".json"];
const MAX_FILE: u64 = 16 << 20;
const DEP_KEYS: [&str; 3] = ["dependencies", "optionalDependencies", "peerDependencies"];
pub const ABOUT: &str = "scorer 14 rule `mapdefect` (experiments/137-scorer-mapdefect/scorer14/mapdefect14.py): per entry, the sha256 of <DATA>/truth/<id>.mapdefect14.json.gz (rows: unit start, end, old package, new package, version, file, evidence tokens, closure share x1000, holder share x1000, old file; evaluated: every span the rule read, with its verdict)";
/// Property / string names nearly every file holds.
const COMMON: &str = "length prototype call apply bind push pop shift unshift slice splice concat join split indexOf
includes forEach map filter reduce some every keys values entries assign create freeze defineProperty
getOwnPropertyNames getOwnPropertyDescriptor hasOwnProperty toString valueOf constructor name type value
data default exports module then catch finally resolve reject done next return throw charCodeAt charAt
substring substr replace match test exec trim toLowerCase toUpperCase startsWith endsWith isArray
string number object function boolean undefined symbol bigint message stack error code options args
start end size from emit listeners parent node props state self window document process env";

/// The manifest's `rules` block.
pub fn rules() -> PyValue {
    dict(vec![
        kv("minEvidence", int(MIN_EVIDENCE as i64)),
        kv("dfMax", int(DF_MAX as i64)),
        kv("lack", flt(LACK)),
        kv("hold", flt(HOLD)),
        kv("candidate", flt(CANDIDATE)),
        kv("sample", int(SAMPLE as i64)),
        kv("fileCap", int(FILE_CAP as i64)),
        kv("depKeys", list_s(DEP_KEYS)),
    ])
}

/// A token as Python held it after `json.loads`: WTF-8 bytes.
type Token = Vec<u8>;

/// Python's code points of a WTF-8 token.
fn code_points(t: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < t.len() {
        let b = t[k];
        let (n, init) = match b {
            0..=0x7f => (1, b as u32),
            0xc0..=0xdf => (2, (b & 0x1f) as u32),
            0xe0..=0xef => (3, (b & 0x0f) as u32),
            _ => (4, (b & 0x07) as u32),
        };
        let mut cp = init;
        for j in 1..n {
            cp = (cp << 6) | (t.get(k + j).copied().unwrap_or(0x80) & 0x3f) as u32;
        }
        out.push(cp);
        k += n;
    }
    out
}

/// `clean`: verbatim-searchable tokens (4..200 characters, three ASCII
/// letters in a row, no quote / backslash / control character), less COMMON.
fn clean(toks: Vec<Vec<u16>>, common: &HashSet<&str>) -> HashSet<Token> {
    let mut out = HashSet::new();
    for t in toks {
        let w = tokens14::wtf8(&t);
        let cps = code_points(&w);
        if !(4..=200).contains(&cps.len()) {
            continue;
        }
        let letters = cps.windows(3).any(|x| {
            x.iter()
                .all(|&c| c < 128 && (c as u8).is_ascii_alphabetic())
        });
        let bad = cps.iter().any(|&c| {
            c == '"' as u32 || c == '\'' as u32 || c == '`' as u32 || c == '\\' as u32 || c < 32
        });
        if !letters || bad {
            continue;
        }
        if std::str::from_utf8(&w).is_ok_and(|x| common.contains(x)) {
            continue;
        }
        out.insert(w);
    }
    out
}

/// `package_of`: (package name, root dir) of a path under node_modules
/// (its innermost node_modules segment).
fn package_of(path: &str) -> Option<(String, String)> {
    let k = path.rfind("/node_modules/")?;
    let (head, rest) = (&path[..k], &path[k + "/node_modules/".len()..]);
    let parts: Vec<&str> = rest.split('/').collect();
    if parts[0].is_empty() || parts[0].starts_with('.') {
        return None;
    }
    let n = if parts[0].starts_with('@') && parts.len() > 1 {
        2
    } else {
        1
    };
    let mut root = pypath::join(head, "node_modules");
    for p in &parts[..n] {
        root = pypath::join(&root, p);
    }
    Some((parts[..n].join("/"), root))
}

fn realpath(p: &str) -> String {
    std::fs::canonicalize(p)
        .map_or_else(|_| pypath::normpath(p), |r| r.to_string_lossy().to_string())
}

/// One installed file: (package, real root, real path).
type File = (String, String, String);

/// `tree_files`: every .js/.cjs/.mjs/.json file (at most MAX_FILE bytes)
/// under the given node_modules dirs, symlinks followed, each real file
/// once, in os.walk(followlinks=True) order.
fn tree_files(tops: &[String]) -> Vec<File> {
    let mut seen_dirs = HashSet::new();
    let mut seen_files = HashSet::new();
    let mut out = Vec::new();
    for top in tops {
        walk(top, &mut seen_dirs, &mut seen_files, &mut out);
    }
    out
}

fn walk(
    d: &str,
    seen_dirs: &mut HashSet<String>,
    seen_files: &mut HashSet<String>,
    out: &mut Vec<File>,
) {
    let Ok(read) = std::fs::read_dir(d) else {
        return;
    };
    let mut dirs: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for e in read.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() {
            dirs.push(name);
        } else {
            names.push(name);
        }
    }
    let rd = realpath(d);
    if !seen_dirs.insert(rd) {
        return;
    }
    dirs.sort();
    names.sort();
    for n in names {
        if !EXTS.iter().any(|x| n.ends_with(x)) {
            continue;
        }
        let p = realpath(&pypath::join(d, &n));
        if seen_files.contains(&p) || !Path::new(&p).is_file() {
            continue;
        }
        seen_files.insert(p.clone());
        let Some((pkg, root)) = package_of(&p) else {
            continue;
        };
        match std::fs::metadata(&p) {
            Ok(m) if m.len() <= MAX_FILE => out.push((pkg, root, p)),
            _ => {}
        }
    }
    for x in dirs {
        walk(&pypath::join(d, &x), seen_dirs, seen_files, out);
    }
}

fn read_json(p: &Path) -> serde_json::Map<String, Value> {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| {
            serde_json::from_str::<Value>(&super::corpus::lone_surrogates_replaced(&t)).ok()
        })
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

/// `resolve`: the real root of `dep` as node resolves it from `from_root`.
fn resolve(dep: &str, from_root: &str, stop: &str) -> Vec<String> {
    let mut out = Vec::new();
    let real = realpath(from_root);
    let starts: Vec<&str> = if real == from_root {
        vec![from_root]
    } else {
        vec![from_root, &real]
    };
    for start in starts {
        let mut d = start.to_string();
        loop {
            let c = pypath::join(&pypath::join(&d, "node_modules"), dep);
            if Path::new(&c).join("package.json").is_file() {
                out.push(realpath(&c));
                break;
            }
            let up = pypath::dirname(&d);
            if d == stop || up == d || !d.starts_with(stop) {
                break;
            }
            d = up;
        }
    }
    out
}

/// `closure`: the real roots of the package at `root` and of every
/// dependency resolved from it, transitively.
fn closure(root: &str, stop: &str) -> HashSet<String> {
    let mut todo = vec![realpath(root)];
    let mut done = HashSet::new();
    while let Some(r) = todo.pop() {
        if done.contains(&r) {
            continue;
        }
        done.insert(r.clone());
        let pj = read_json(&Path::new(&r).join("package.json"));
        for k in DEP_KEYS {
            let mut deps: Vec<String> = match pj.get(k) {
                Some(Value::Object(m)) => m.keys().cloned().collect(),
                Some(Value::Array(xs)) => xs
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect(),
                _ => Vec::new(),
            };
            deps.sort();
            for dep in deps {
                todo.extend(resolve(&dep, &r, stop));
            }
        }
    }
    done
}

fn version_of(root: &str) -> PyValue {
    match read_json(&Path::new(root).join("package.json")).get("version") {
        None | Some(Value::Null) => PyValue::Null,
        Some(Value::String(v)) => s(v),
        Some(Value::Bool(b)) => s(if *b { "True" } else { "False" }),
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => s(&n.to_string()),
        Some(Value::Number(n)) => s(&crate::json::py::float_repr(n.as_f64().unwrap_or(f64::NAN))),
        Some(other) => s(&other.to_string()),
    }
}

fn sha1_hex(b: &[u8]) -> String {
    // sha1 is not in the workspace; Python's hashlib.sha1 written out
    sha1::hex(b)
}

mod sha1 {
    /// SHA-1 (FIPS 180-4) of `data`, lowercase hex.
    pub fn hex(data: &[u8]) -> String {
        let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
        let mut msg = data.to_vec();
        let bits = (data.len() as u64).wrapping_mul(8);
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bits.to_be_bytes());
        for chunk in msg.chunks(64) {
            let mut w = [0u32; 80];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[4 * i],
                    chunk[4 * i + 1],
                    chunk[4 * i + 2],
                    chunk[4 * i + 3],
                ]);
            }
            for i in 16..80 {
                w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
            }
            let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
            for (i, wi) in w.iter().enumerate() {
                let (f, k) = match i {
                    0..=19 => ((b & c) | (!b & d), 0x5A827999),
                    20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                    40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                    _ => (b ^ c ^ d, 0xCA62C1D6),
                };
                let t = a
                    .rotate_left(5)
                    .wrapping_add(f)
                    .wrapping_add(e)
                    .wrapping_add(k)
                    .wrapping_add(*wi);
                e = d;
                d = c;
                c = b.rotate_left(30);
                b = a;
                a = t;
            }
            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
        }
        h.iter().map(|x| format!("{x:08x}")).collect()
    }
}

/// `sample`: at most SAMPLE tokens, the first by sha1 (Python's
/// `t.encode()` refuses a lone surrogate; so does this).
fn sample(tokens: Vec<Token>) -> Result<Vec<Token>, String> {
    if tokens.len() <= SAMPLE {
        return Ok(tokens);
    }
    let mut keyed = Vec::with_capacity(tokens.len());
    for t in tokens {
        if std::str::from_utf8(&t).is_err() {
            return Err(
                "a sampled token holds a lone surrogate (Python's encode() raised here)".into(),
            );
        }
        keyed.push((sha1_hex(&t), t));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(keyed.into_iter().take(SAMPLE).map(|x| x.1).collect())
}

fn share_in(tokens: &[Token], text: &[u8]) -> f64 {
    if tokens.is_empty() {
        return 0.0;
    }
    let n = tokens
        .iter()
        .filter(|t| memchr::memmem::find(text, t).is_some())
        .count();
    n as f64 / tokens.len() as f64
}

/// `decide`: the rule's verdict for one unit; (reason, chosen holder).
fn decide<'h>(
    label: &str,
    evidence: usize,
    closure_share: f64,
    holders: &'h [Holder],
    fams: &Families,
    app_pkg: Option<&str>,
) -> (&'static str, Option<&'h Holder>) {
    let same = |a: &str, b: &str| fams.same(Some(a), Some(b));
    if evidence < MIN_EVIDENCE {
        return ("few-evidence", None);
    }
    if closure_share >= LACK {
        return ("label-holds", None);
    }
    if holders.is_empty() {
        return ("nobody-holds", None);
    }
    if holders.iter().any(|h| same(&h.pkg, label)) {
        return ("label-family-elsewhere", None);
    }
    if let Some(app) = app_pkg
        && !app.is_empty()
        && holders.iter().any(|h| h.pkg == app || same(&h.pkg, app))
    {
        return ("app-holds", None);
    }
    let mut seen: Vec<&str> = Vec::new();
    for h in holders {
        if !seen.iter().any(|f| same(&h.pkg, f)) {
            seen.push(&h.pkg);
        }
    }
    if seen.len() > 1 {
        return ("ambiguous", None);
    }
    let best = holders
        .iter()
        .min_by(|a, b| {
            b.share
                .total_cmp(&a.share)
                .then(a.size.cmp(&b.size))
                .then(a.path.cmp(&b.path))
        })
        .expect("holders is not empty");
    ("relabel", Some(best))
}

struct Holder {
    pkg: String,
    share: f64,
    size: u64,
    path: String,
    root: String,
}

/// `source_paths` for the map of `role`.
fn source_paths(entry: &CorpusEntry, map_path: &Path, role: Role) -> Result<Vec<String>, String> {
    let origin = match role {
        Role::InnerMap => entry.files.inner_map.as_ref(),
        _ => entry.files.map.as_ref(),
    }
    .map_or_else(String::new, |f| f.origin.clone());
    let base = pypath::dirname(&origin);
    let text =
        std::fs::read_to_string(map_path).map_err(|e| format!("{}: {e}", map_path.display()))?;
    let m: Value = serde_json::from_str(&super::corpus::lone_surrogates_replaced(&text))
        .map_err(|e| format!("{}: {e}", map_path.display()))?;
    let root = m.get("sourceRoot").and_then(Value::as_str).unwrap_or("");
    let root = if root.is_empty() { "" } else { root };
    Ok(m.get("sources")
        .and_then(Value::as_array)
        .map(|xs| {
            xs.iter()
                .map(|x| match x.as_str() {
                    Some(x) => pypath::normpath(&pypath::join(&pypath::join(&base, root), x)),
                    None => String::new(),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// `label_source`: the source of package `lab` holding most of [s, e).
fn label_source(truth: &Truth, offs: &[i64], s: i64, e: i64, lab: &str) -> Option<i64> {
    let segs = &truth.segments;
    let mut k = offs.partition_point(|&o| o <= s).saturating_sub(1);
    let mut files: Counter<i64> = Counter::new();
    while k < segs.len() && segs[k].0 < e {
        let a = segs[k].0.max(s);
        let b = if k + 1 < segs.len() { segs[k + 1].0 } else { e }.min(e);
        if b > a {
            files.add(segs[k].1, b - a);
        }
        k += 1;
    }
    files
        .most_common()
        .into_iter()
        .map(|(si, _)| *si)
        .find(|si| truth.sources[*si as usize].label == lab)
}

/// The reference runs' units by span, each with its label.
type RefUnits = BTreeMap<(i64, i64), (Unit, Label)>;

/// `reference_units`: {(start, end): (unit, label)} over the reference
/// labels' runs, each labelled with scorer 13's truth (this rule off).
fn reference_units(
    corpus: &Corpus,
    entry: &CorpusEntry,
    labels: &[String],
    truth: &Truth,
    origins: Option<&truth::Origins>,
) -> Result<RefUnits, String> {
    let mut out = BTreeMap::new();
    for label in labels {
        if !super::score::report_path(corpus, label, entry).exists() {
            continue;
        }
        let Some(units) = super::score::label_units(corpus, label, entry)? else {
            continue;
        };
        let audit = Audit::with(
            corpus,
            entry,
            truth,
            AuditLayers {
                copies: true,
                misattr13: true,
                mapdefect14: false,
            },
        )?;
        let scored = super::score::scored_units(&units);
        let mut rows = truth::label_units(truth, &units, &scored, origins, &audit)?;
        for u in scored {
            let row = rows.remove(&u.i).ok_or("a scored unit has no label")?;
            out.entry((u.s, u.e)).or_insert_with(|| (u.clone(), row));
        }
    }
    Ok(out)
}

/// The app's install tree as the rule reads it: every installed file,
/// which of them hold each wanted token (the first FILE_CAP), the tokens
/// held by more, each token's package count, and the read caches.
struct Tree {
    files: Vec<File>,
    hits: HashMap<Token, Vec<usize>>,
    many: HashSet<Token>,
    df: HashMap<Token, usize>,
    stop: String,
    texts: HashMap<String, Vec<u8>>,
    closure_text: HashMap<String, Vec<u8>>,
    closures: HashMap<String, HashSet<String>>,
}

/// One relabel: (package, version, file, evidence tokens, closure share,
/// holder share).
struct Relabel {
    pkg: String,
    version: PyValue,
    file: String,
    tokens: usize,
    closure_share: f64,
    share: f64,
}

impl Tree {
    fn new(tops: &[String], ev: &[HashSet<Token>]) -> Tree {
        let files = tree_files(tops);
        let mut want: Vec<Token> = ev
            .iter()
            .flatten()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        want.sort();
        let res = tokens14::file_hits(
            files.len(),
            |i| std::fs::read(&files[i].2).ok(),
            &want,
            FILE_CAP,
        );
        let many = res.many.iter().map(|&i| want[i].clone()).collect();
        let mut hits = HashMap::new();
        for (t, fis) in want.into_iter().zip(res.hits) {
            if !fis.is_empty() {
                hits.insert(t, fis);
            }
        }
        let df = hits
            .iter()
            .map(|(t, fis)| {
                let pk: HashSet<&str> = fis.iter().map(|&fi| files[fi].0.as_str()).collect();
                (t.clone(), pk.len())
            })
            .collect();
        Tree {
            stop: commonpath(&tops.iter().map(|t| pypath::dirname(t)).collect::<Vec<_>>()),
            files,
            hits,
            many,
            df,
            texts: HashMap::new(),
            closure_text: HashMap::new(),
            closures: HashMap::new(),
        }
    }

    /// `sample(distinctive(tokens))`.
    fn evidence(&self, tokens: &HashSet<Token>) -> Result<Vec<Token>, String> {
        let mut dist: Vec<Token> = tokens
            .iter()
            .filter(|t| !self.many.contains(*t) && self.df.get(*t).copied().unwrap_or(0) <= DF_MAX)
            .cloned()
            .collect();
        dist.sort();
        sample(dist)
    }

    fn hits_of(&self, t: &Token) -> &[usize] {
        self.hits.get(t).map_or(&[], Vec::as_slice)
    }

    /// The share of `toks` the closure of the package at `root` holds.
    fn closure_share(&mut self, root: &str, cl: &HashSet<String>, toks: &[Token]) -> f64 {
        if !self.closure_text.contains_key(root) {
            if self.closure_text.len() > 64 {
                self.closure_text.clear();
            }
            let mut ctext: Vec<u8> = Vec::new();
            let paths: Vec<String> = self
                .files
                .iter()
                .filter(|f| cl.contains(&f.1))
                .map(|f| f.2.clone())
                .collect();
            for (i, p) in paths.iter().enumerate() {
                if i > 0 {
                    ctext.push(b'\n');
                }
                ctext.extend_from_slice(text_of(&mut self.texts, p));
            }
            self.closure_text.insert(root.to_string(), ctext);
        }
        share_in(toks, &self.closure_text[root])
    }

    /// The files outside the closure holding at least HOLD of `toks`.
    fn holders(&mut self, cl: &HashSet<String>, toks: &[Token]) -> Vec<Holder> {
        let mut per_file: Counter<usize> = Counter::new();
        for t in toks {
            for &fi in self.hits_of(t) {
                if !cl.contains(&self.files[fi].1) {
                    per_file.add(fi, 1);
                }
            }
        }
        let mut holders = Vec::new();
        for (fi, n) in per_file.items() {
            if (n as f64) < CANDIDATE * toks.len() as f64 {
                continue;
            }
            let (pkg, root, path) = self.files[*fi].clone();
            let sh = share_in(toks, text_of(&mut self.texts, &path));
            if sh >= HOLD {
                holders.push(Holder {
                    pkg,
                    share: sh,
                    size: std::fs::metadata(&path).map_or(0, |m| m.len()),
                    path,
                    root,
                });
            }
        }
        holders
    }

    /// The rule on one unit labelled `lab`, its label's source at `path`:
    /// (reason, the relabel when there is one).
    fn judge(
        &mut self,
        lab: &str,
        path: &str,
        tokens: &HashSet<Token>,
        fams: &Families,
        app_pkg: Option<&str>,
    ) -> Result<(&'static str, Option<Relabel>), String> {
        let toks = self.evidence(tokens)?;
        let marker = format!("/node_modules/{lab}/");
        let root = path
            .rfind(&marker)
            .map(|k| path[..k + marker.len() - 1].to_string());
        let Some(root) = root.filter(|r| Path::new(r).join("package.json").is_file()) else {
            return Ok(("label-not-installed", None));
        };
        if toks.len() < MIN_EVIDENCE {
            return Ok(("few-evidence", None));
        }
        let stop = self.stop.clone();
        let cl = self
            .closures
            .entry(root.clone())
            .or_insert_with(|| closure(&root, &stop))
            .clone();
        // token-set prefilter: exact values held by the closure
        let held_set = toks
            .iter()
            .filter(|t| {
                self.hits_of(t)
                    .iter()
                    .any(|&fi| cl.contains(&self.files[fi].1))
            })
            .count();
        if held_set as f64 >= LACK * toks.len() as f64 {
            return Ok(("label-holds", None));
        }
        let cshare = self.closure_share(&root, &cl, &toks);
        let holders = self.holders(&cl, &toks);
        let (reason, best) = decide(lab, toks.len(), cshare, &holders, fams, app_pkg);
        Ok((
            reason,
            best.map(|b| Relabel {
                pkg: b.pkg.clone(),
                version: version_of(&b.root),
                file: pypath::relpath(&b.path, &b.root),
                tokens: toks.len(),
                closure_share: cshare,
                share: b.share,
            }),
        ))
    }
}

/// The app's install tree(s): the node_modules the map's sources live in
/// (the first segment of each path).
fn install_tops(paths: &[String]) -> Vec<String> {
    let mut tops: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            p.find("/node_modules/")
                .map(|k| format!("{}/node_modules", &p[..k]))
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    tops.sort();
    tops.retain(|t| Path::new(t).is_dir());
    tops
}

/// The units the rule reads: library labels with no copy origin and no
/// scorer 13 relabel; the byte counts of the rest.
fn candidates<'u>(
    units: &'u BTreeMap<(i64, i64), (Unit, Label)>,
    agg: &mut Counter<String>,
) -> Vec<((i64, i64), &'u Unit, &'u Label)> {
    let mut cands = Vec::new();
    for (span, (u, row)) in units {
        agg.add("units judged".into(), 1);
        if NOT_LIBRARY.contains(&row.label.as_str()) {
            continue;
        }
        agg.add("library units".into(), 1);
        agg.add("library bytes".into(), u.b);
        if row.has_origin || row.misattributed {
            agg.add(
                "skipped: copy origin or scorer 13 relabel (bytes)".into(),
                u.b,
            );
            continue;
        }
        cands.push((*span, u, row));
    }
    cands
}

/// One entry's layer document and byte counts (mapdefect14.py `entry_layer`).
fn entry_layer(
    corpus: &Corpus,
    entry: &CorpusEntry,
    labels: &[String],
    fams: &Families,
) -> Result<(PyValue, Counter<String>), String> {
    let common: HashSet<&str> = COMMON.split_whitespace().collect();
    let mut truth = truth::load_truth(corpus, entry)?;
    let o = truth::load_origins(corpus, entry)?;
    let origins = truth::redecode(corpus, entry, &mut truth, Some(o))?;
    let mut agg: Counter<String> = Counter::new();
    let units = reference_units(corpus, entry, labels, &truth, origins.as_ref())?;
    let cands = candidates(&units, &mut agg);
    let role = if entry.files.inner_map.is_some() {
        Role::InnerMap
    } else {
        Role::Map
    };
    let paths = source_paths(entry, &corpus.corpus_path(entry, role)?, role)?;
    let tops = install_tops(&paths);
    let mut rows: Vec<(i64, i64, Vec<PyValue>)> = Vec::new();
    let mut why: HashMap<(i64, i64), &'static str> = HashMap::new();
    if !cands.is_empty() && !tops.is_empty() {
        let bundle = std::fs::read(corpus.corpus_path(entry, Role::Bundle)?)
            .map_err(|e| format!("{}: {e}", entry.id))?;
        let ev: Vec<HashSet<Token>> = humanify_ag_core::par::map_ordered(&cands, |c| {
            let (a, b) = (
                (c.0.0.max(0) as usize).min(bundle.len()),
                (c.0.1.max(0) as usize).min(bundle.len()),
            );
            clean(
                tokens14::unit_evidence(&tokens14::utf16_lossy(&bundle[a..b.max(a)])),
                &common,
            )
        });
        let mut tree = Tree::new(&tops, &ev);
        let offs: Vec<i64> = truth.segments.iter().map(|x| x.0).collect();
        for (ci, ((s0, e0), _u, row)) in cands.iter().enumerate() {
            let lab = row.label.as_str();
            let path = label_source(&truth, &offs, *s0, *e0, lab)
                .and_then(|si| paths.get(si as usize))
                .map_or("", String::as_str);
            let (reason, relabel) =
                tree.judge(lab, path, &ev[ci], fams, truth.app_package.as_deref())?;
            why.insert((*s0, *e0), reason);
            let Some(r) = relabel else {
                continue;
            };
            agg.add("relabelled units".into(), 1);
            rows.push((
                *s0,
                *e0,
                vec![
                    int(*s0),
                    int(*e0),
                    s(lab),
                    s(&r.pkg),
                    r.version,
                    s(&r.file),
                    int(r.tokens as i64),
                    int(round_int(1000.0 * r.closure_share)),
                    int(round_int(1000.0 * r.share)),
                    opt_s(row.file.as_deref()),
                ],
            ));
        }
    }
    for (span, u, _) in &cands {
        let w = *why.entry(*span).or_insert("no-install-tree");
        agg.add(format!("{w} (bytes)"), u.b);
    }
    rows.sort_by_key(|r| (r.0, r.1));
    let mut evaluated: Vec<(i64, i64, &str)> =
        cands.iter().map(|c| (c.0.0, c.0.1, why[&c.0])).collect();
    evaluated.sort();
    let doc = dict(vec![
        kv("schema", s(SCHEMA)),
        kv(
            "rows",
            PyValue::List(rows.into_iter().map(|r| PyValue::List(r.2)).collect()),
        ),
        kv(
            "evaluated",
            PyValue::List(
                evaluated
                    .into_iter()
                    .map(|(a, b, w)| PyValue::List(vec![int(a), int(b), s(w)]))
                    .collect(),
            ),
        ),
    ]);
    Ok((doc, agg))
}

/// An installed file's text (Python: `open(path, errors="replace").read()`),
/// read once.
fn text_of<'a>(texts: &'a mut HashMap<String, Vec<u8>>, path: &str) -> &'a [u8] {
    if texts.len() > 4000 && !texts.contains_key(path) {
        texts.clear();
    }
    texts.entry(path.to_string()).or_insert_with(|| {
        std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned().into_bytes())
            .unwrap_or_default()
    })
}

/// `os.path.commonpath` of absolute paths.
fn commonpath(paths: &[String]) -> String {
    let split: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| {
            p.split('/')
                .filter(|x| !x.is_empty() && *x != ".")
                .collect()
        })
        .collect();
    let Some(first) = split.first() else {
        return String::new();
    };
    let mut n = first.len();
    for p in &split[1..] {
        n = n.min(p.iter().zip(first).take_while(|(a, b)| a == b).count());
    }
    format!("/{}", first[..n].join("/"))
}

/// Build the layer for every source-mapped entry of `corpus` into
/// `out/truth/<id>.mapdefect14.json.gz`; (entry id -> sha, manifest text).
pub fn build(
    corpus: &Corpus,
    labels: &[String],
    out: &Path,
) -> Result<(BTreeMap<String, String>, String), String> {
    let fams = Families::shipped();
    let entries: Vec<&CorpusEntry> = corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
        .collect();
    let mut shas = BTreeMap::new();
    let mut by_set: BTreeMap<String, Counter<String>> = BTreeMap::new();
    for e in entries {
        let (doc, agg) = entry_layer(corpus, e, labels, &fams)?;
        by_set.entry(e.set.clone()).or_default().update(&agg);
        let p: PathBuf = out
            .join("truth")
            .join(format!("{}.mapdefect14.json.gz", e.id));
        super::gz::write_gz(&p, crate::json::py_text(&doc, true).as_bytes())?;
        shas.insert(e.id.clone(), super::corpus::sha256_file(&p)?);
    }
    let bytes = dict(
        by_set
            .iter()
            .map(|(set, c)| {
                (
                    set.clone(),
                    dict(c.items().map(|(k, v)| (k.clone(), int(v))).collect()),
                )
            })
            .collect(),
    );
    let man = dict(vec![
        kv("schema", s(SCHEMA)),
        kv("corpusSha256", s(&corpus.loaded.manifest_id)),
        kv("unitsLabels", list_s(labels.iter().map(String::as_str))),
        kv(
            "entries",
            dict(shas.iter().map(|(k, v)| (k.clone(), s(v))).collect()),
        ),
        kv("rules", rules()),
        kv("bytes", bytes),
        kv("about", s(ABOUT)),
    ]);
    Ok((shas, crate::json::py_text(&man, true)))
}

#[cfg(test)]
#[path = "mapdefect14_test.rs"]
mod mapdefect14_test;
